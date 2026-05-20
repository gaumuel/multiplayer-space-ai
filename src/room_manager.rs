use std::collections::HashMap;
use crate::room::Room;
use crate::network::messages::{GameMode, RoomInfo, RoomState, ClientRole};
use crate::room::PlayerSlot;
use rand::Rng;

pub struct RoomManager {
    pub rooms: HashMap<String, Room>,
}

impl RoomManager {
    pub fn new() -> Self {
        Self { rooms: HashMap::new() }
    }

    pub fn create_room(&mut self, mode: GameMode) -> String {
        let id = generate_room_id();
        let room = Room::new(id.clone(), mode);
        self.rooms.insert(id.clone(), room);
        id
    }

    pub fn list_rooms(&self) -> Vec<RoomInfo> {
        self.rooms.values().map(|r| {
            let players = r.players.iter()
                .filter(|p| !matches!(p.controller, crate::room::SlotController::Empty))
                .count() as u8;
            RoomInfo {
                id: r.id.clone(),
                mode: r.mode.clone(),
                state: r.state,
                players,
                spectators: r.spectators.len() as u8,
            }
        }).collect()
    }

    pub fn join_room(&mut self, room_id: &str, client_id: usize, role: &ClientRole) -> Result<(PlayerSlot, u8), String> {
        let room = self.rooms.get_mut(room_id)
            .ok_or_else(|| "Room not found".to_string())?;

        match role {
            ClientRole::Player => {
                let slot = room.join_as_player(client_id)
                    .ok_or_else(|| "Room is full".to_string())?;
                let team = slot.team_id();

                // Auto-start if ready
                if room.is_ready() && room.state == RoomState::Waiting {
                    room.start();
                }

                Ok((slot, team))
            }
            ClientRole::Spectator => {
                room.join_as_spectator(client_id);
                Ok((PlayerSlot::Player1, 255)) // team 255 = spectator
            }
        }
    }

    pub fn leave_room(&mut self, room_id: &str, client_id: usize) {
        if let Some(room) = self.rooms.get_mut(room_id) {
            room.remove_client(client_id);
        }
    }

    pub fn remove_client_from_all(&mut self, client_id: usize) {
        for room in self.rooms.values_mut() {
            room.remove_client(client_id);
        }
    }

    /// Tick all playing rooms, returns list of (room_id, winner_team) for ended games
    pub fn tick_all(&mut self) -> Vec<(String, u8)> {
        let mut ended = Vec::new();

        for room in self.rooms.values_mut() {
            if room.state == RoomState::Playing {
                room.tick();

                if let Some(winner) = room.check_game_over() {
                    room.state = RoomState::Ended;
                    ended.push((room.id.clone(), winner));
                }
            }
        }

        ended
    }

    /// Remove ended rooms that have no clients
    pub fn cleanup(&mut self) {
        self.rooms.retain(|_, r| {
            !(r.state == RoomState::Ended && r.all_client_ids().is_empty())
        });
    }
}

fn generate_room_id() -> String {
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    (0..6).map(|_| chars[rng.gen_range(0..chars.len())]).collect()
}
