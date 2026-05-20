mod components;
mod systems;
mod network;
mod room;
mod room_manager;
mod spatial;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::info;
use tracing_subscriber;

use network::server::{WtServer, InboundEvent, OutboundEvent, ClientSenders, encode_snapshot};
use network::messages::*;
use room_manager::RoomManager;

#[derive(bevy_ecs::resource::Resource)]
pub struct GameTime {
    elapsed: std::time::Duration,
    delta: std::time::Duration,
    last_tick: std::time::Instant,
}

impl GameTime {
    pub fn new() -> Self {
        Self {
            elapsed: std::time::Duration::ZERO,
            delta: std::time::Duration::ZERO,
            last_tick: std::time::Instant::now(),
        }
    }

    pub fn tick(&mut self) {
        let now = std::time::Instant::now();
        self.delta = now.duration_since(self.last_tick);
        self.elapsed += self.delta;
        self.last_tick = now;
    }

    pub fn elapsed_secs_f64(&self) -> f64 {
        self.elapsed.as_secs_f64()
    }

    pub fn delta_secs(&self) -> f32 {
        self.delta.as_secs_f32()
    }
}

/// Tracks which room each client is in
struct ClientState {
    room_id: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    info!("Starting Space AI vs AI Server (Room-based)");

    let (inbound_tx, mut inbound_rx) = mpsc::unbounded_channel::<InboundEvent>();
    let client_senders: ClientSenders = Arc::new(RwLock::new(HashMap::new()));

    let wt_server = WtServer::new();
    if let Err(e) = wt_server.start(4433, inbound_tx, client_senders.clone()).await {
        tracing::error!("Failed to start WebTransport server: {}", e);
        return;
    }

    let mut room_manager = RoomManager::new();
    let mut client_states: HashMap<usize, ClientState> = HashMap::new();

    let tick_rate = 30.0;
    let tick_duration = std::time::Duration::from_secs_f64(1.0 / tick_rate);

    info!("Game loop running at {} Hz", tick_rate);

    loop {
        let tick_start = std::time::Instant::now();

        // Process all pending inbound events
        while let Ok(event) = inbound_rx.try_recv() {
            match event {
                InboundEvent::ClientConnected { client_id } => {
                    client_states.insert(client_id, ClientState { room_id: None });
                    info!("Client {} connected", client_id);
                }
                InboundEvent::ClientDisconnected { client_id } => {
                    if let Some(state) = client_states.remove(&client_id) {
                        if let Some(room_id) = state.room_id {
                            room_manager.leave_room(&room_id, client_id);
                        }
                    }
                    room_manager.remove_client_from_all(client_id);
                    info!("Client {} disconnected", client_id);
                }
                InboundEvent::Message { client_id, msg } => {
                    handle_message(client_id, msg, &mut room_manager, &mut client_states, &client_senders).await;
                }
            }
        }

        // Tick all playing rooms
        let ended = room_manager.tick_all();
        for (room_id, winner) in &ended {
            if let Some(room) = room_manager.rooms.get(room_id) {
                let msg = OutboundEvent::Control(ServerMessage::GameOver { winner_team: *winner });
                send_to_room(room, &client_senders, msg).await;
            }
            info!("Room {} ended, winner: team {}", room_id, winner);
        }

        // Send snapshots to all clients in playing rooms
        for room in room_manager.rooms.values_mut() {
            if room.state == RoomState::Playing {
                let snapshot = room.build_snapshot();
                let encoded = encode_snapshot(&snapshot);
                let msg = OutboundEvent::Snapshot(encoded);
                let client_ids = room.all_client_ids();
                let senders = client_senders.read().await;
                for cid in client_ids {
                    if let Some(tx) = senders.get(&cid) {
                        let _ = tx.send(msg.clone());
                    }
                }
            }
        }

        // Cleanup empty ended rooms
        room_manager.cleanup();

        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            tokio::time::sleep(tick_duration - elapsed).await;
        }
    }
}

async fn handle_message(
    client_id: usize,
    msg: ClientMessage,
    room_manager: &mut RoomManager,
    client_states: &mut HashMap<usize, ClientState>,
    client_senders: &ClientSenders,
) {
    match msg {
        ClientMessage::ListRooms => {
            let rooms = room_manager.list_rooms();
            send_to_client(client_id, client_senders, OutboundEvent::Control(
                ServerMessage::RoomList { rooms }
            )).await;
        }
        ClientMessage::CreateRoom { mode } => {
            let room_id = room_manager.create_room(mode.clone());
            info!("Client {} created room {}", client_id, room_id);

            // Auto-join as player (unless AI vs AI)
            let team = match mode {
                GameMode::AIVsAI => {
                    // AI vs AI starts immediately, client joins as spectator
                    if let Some(room) = room_manager.rooms.get_mut(&room_id) {
                        room.join_as_spectator(client_id);
                        room.start();
                    }
                    if let Some(state) = client_states.get_mut(&client_id) {
                        state.room_id = Some(room_id.clone());
                    }
                    send_to_client(client_id, client_senders, OutboundEvent::Control(
                        ServerMessage::RoomCreated { room_id: room_id.clone(), team: 255 }
                    )).await;
                    send_to_client(client_id, client_senders, OutboundEvent::Control(
                        ServerMessage::GameStarted { mode: GameMode::AIVsAI }
                    )).await;
                    return;
                }
                _ => {
                    match room_manager.join_room(&room_id, client_id, &ClientRole::Player) {
                        Ok((_, team)) => team,
                        Err(_) => return,
                    }
                }
            };

            if let Some(state) = client_states.get_mut(&client_id) {
                state.room_id = Some(room_id.clone());
            }
            send_to_client(client_id, client_senders, OutboundEvent::Control(
                ServerMessage::RoomCreated { room_id, team }
            )).await;

            // Check if game should start (HumanVsAI starts immediately since AI slot is pre-filled)
            check_and_notify_start(client_id, room_manager, client_states, client_senders).await;
        }
        ClientMessage::JoinRoom { room_id, role } => {
            match room_manager.join_room(&room_id, client_id, &role) {
                Ok((_, team)) => {
                    if let Some(state) = client_states.get_mut(&client_id) {
                        state.room_id = Some(room_id.clone());
                    }
                    send_to_client(client_id, client_senders, OutboundEvent::Control(
                        ServerMessage::RoomJoined { room_id, team, role }
                    )).await;
                    check_and_notify_start(client_id, room_manager, client_states, client_senders).await;
                }
                Err(reason) => {
                    send_to_client(client_id, client_senders, OutboundEvent::Control(
                        ServerMessage::JoinError { reason }
                    )).await;
                }
            }
        }
        ClientMessage::LeaveRoom => {
            if let Some(state) = client_states.get_mut(&client_id) {
                if let Some(room_id) = state.room_id.take() {
                    room_manager.leave_room(&room_id, client_id);
                }
            }
        }
        ClientMessage::PlayAgain => {
            if let Some(state) = client_states.get(&client_id) {
                if let Some(room_id) = &state.room_id {
                    if let Some(room) = room_manager.rooms.get_mut(room_id) {
                        if room.state == RoomState::Ended {
                            room.reset();
                            let msg = OutboundEvent::Control(ServerMessage::GameStarted { mode: room.mode.clone() });
                            let client_ids = room.all_client_ids();
                            let senders = client_senders.read().await;
                            for cid in client_ids {
                                if let Some(tx) = senders.get(&cid) {
                                    let _ = tx.send(msg.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        ClientMessage::Command { command } => {
            if let Some(state) = client_states.get(&client_id) {
                if let Some(room_id) = &state.room_id {
                    if let Some(room) = room_manager.rooms.get_mut(room_id) {
                        if let Some(response) = room.handle_command(client_id, &command) {
                            send_to_client(client_id, client_senders, OutboundEvent::Control(response)).await;
                        }
                    }
                }
            }
        }
    }
}

async fn check_and_notify_start(
    _client_id: usize,
    room_manager: &mut RoomManager,
    _client_states: &HashMap<usize, ClientState>,
    client_senders: &ClientSenders,
) {
    // Find rooms that just became ready
    for room in room_manager.rooms.values_mut() {
        if room.state == RoomState::Playing {
            let msg = OutboundEvent::Control(ServerMessage::GameStarted { mode: room.mode.clone() });
            let client_ids = room.all_client_ids();
            let senders = client_senders.read().await;
            for cid in client_ids {
                if let Some(tx) = senders.get(&cid) {
                    let _ = tx.send(msg.clone());
                }
            }
        }
    }
}

async fn send_to_client(client_id: usize, client_senders: &ClientSenders, msg: OutboundEvent) {
    let senders = client_senders.read().await;
    if let Some(tx) = senders.get(&client_id) {
        let _ = tx.send(msg);
    }
}

async fn send_to_room(room: &room::Room, client_senders: &ClientSenders, msg: OutboundEvent) {
    let client_ids = room.all_client_ids();
    let senders = client_senders.read().await;
    for cid in client_ids {
        if let Some(tx) = senders.get(&cid) {
            let _ = tx.send(msg.clone());
        }
    }
}
