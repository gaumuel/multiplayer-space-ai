mod components;
mod systems;
mod network;
mod room;
mod room_manager;
mod spatial;
mod wasm_ai;

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
        ClientMessage::CreateRoom { mode, obstacles } => {
            let room_id = room_manager.create_room(mode.clone(), obstacles);
            info!("Client {} created room {} (obstacles: {})", client_id, room_id, obstacles);

            // Join as player (or spectator for AI vs AI)
            let role = if matches!(mode, GameMode::AIVsAI) { ClientRole::Spectator } else { ClientRole::Player };
            let team = match room_manager.join_room(&room_id, client_id, &role) {
                Ok((_, team)) => team,
                Err(_) => return,
            };

            if let Some(state) = client_states.get_mut(&client_id) {
                state.room_id = Some(room_id.clone());
            }
            send_to_client(client_id, client_senders, OutboundEvent::Control(
                ServerMessage::RoomCreated { room_id, team }
            )).await;
        }
        ClientMessage::JoinRoom { room_id, role } => {
            match room_manager.join_room(&room_id, client_id, &role) {
                Ok((_, team)) => {
                    if let Some(state) = client_states.get_mut(&client_id) {
                        state.room_id = Some(room_id.clone());
                    }
                    send_to_client(client_id, client_senders, OutboundEvent::Control(
                        ServerMessage::RoomJoined { room_id: room_id.clone(), team, role }
                    )).await;

                    // If room is already playing, notify immediately
                    if let Some(room) = room_manager.rooms.get(&room_id) {
                        if room.state == RoomState::Playing {
                            send_to_client(client_id, client_senders, OutboundEvent::Control(
                                ServerMessage::GameStarted { mode: room.mode.clone() }
                            )).await;
                        }
                    }
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
        ClientMessage::UploadWasm { wasm_base64, slot } => {
            if let Some(state) = client_states.get(&client_id) {
                if let Some(room_id) = &state.room_id {
                    if let Some(room) = room_manager.rooms.get_mut(room_id) {
                        if room.state != RoomState::Waiting {
                            send_to_client(client_id, client_senders, OutboundEvent::Control(
                                ServerMessage::Error { message: "Can only upload WASM before game starts".to_string() }
                            )).await;
                            return;
                        }

                        let slot_idx = slot as usize;
                        if slot_idx > 1 {
                            send_to_client(client_id, client_senders, OutboundEvent::Control(
                                ServerMessage::Error { message: "Invalid slot (0 or 1)".to_string() }
                            )).await;
                            return;
                        }

                        let wasm_bytes = match base64_decode(&wasm_base64) {
                            Some(b) => b,
                            None => {
                                send_to_client(client_id, client_senders, OutboundEvent::Control(
                                    ServerMessage::Error { message: "Invalid base64".to_string() }
                                )).await;
                                return;
                            }
                        };

                        match wasm_ai::runner::WasmAiRunner::new(&wasm_bytes) {
                            Ok(runner) => {
                                room.players[slot_idx].controller = room::SlotController::Wasm {
                                    runner: std::sync::Arc::new(std::sync::Mutex::new(runner)),
                                };
                                info!("Client {} uploaded WASM AI for slot {}", client_id, slot_idx);
                                send_to_client(client_id, client_senders, OutboundEvent::Control(
                                    ServerMessage::Error { message: format!("WASM AI loaded for slot {}", slot_idx) }
                                )).await;
                            }
                            Err(e) => {
                                send_to_client(client_id, client_senders, OutboundEvent::Control(
                                    ServerMessage::Error { message: format!("WASM load error: {}", e) }
                                )).await;
                            }
                        }
                    }
                }
            }
        }
        ClientMessage::StartGame => {
            if let Some(state) = client_states.get(&client_id) {
                if let Some(room_id) = &state.room_id {
                    if let Some(room) = room_manager.rooms.get_mut(room_id) {
                        if room.state == RoomState::Waiting {
                            // Fill empty slots with built-in AI
                            for player in &mut room.players {
                                if matches!(player.controller, room::SlotController::Empty) {
                                    player.controller = room::SlotController::AI;
                                }
                            }
                            room.start();
                            let msg = OutboundEvent::Control(ServerMessage::GameStarted { mode: room.mode.clone() });
                            let client_ids = room.all_client_ids();
                            let senders = client_senders.read().await;
                            for cid in client_ids {
                                if let Some(tx) = senders.get(&cid) {
                                    let _ = tx.send(msg.clone());
                                }
                            }
                            info!("Room {} started by client {}", room_id, client_id);
                        }
                    }
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
                        if room.state == RoomState::Playing {
                            if let Some(response) = room.handle_command(client_id, &command) {
                                send_to_client(client_id, client_senders, OutboundEvent::Control(response)).await;
                            }
                        }
                    }
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

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: [u8; 128] = {
        let mut t = [255u8; 128];
        let mut i = 0u8;
        while i < 26 { t[(b'A' + i) as usize] = i; i += 1; }
        i = 0;
        while i < 26 { t[(b'a' + i) as usize] = 26 + i; i += 1; }
        i = 0;
        while i < 10 { t[(b'0' + i) as usize] = 52 + i; i += 1; }
        t[b'+' as usize] = 62;
        t[b'/' as usize] = 63;
        t
    };

    let input = input.trim_end_matches('=');
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;

    for &b in input.as_bytes() {
        if b >= 128 { return None; }
        let val = TABLE[b as usize];
        if val == 255 { return None; }
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}
