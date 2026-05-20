use serde::{Deserialize, Serialize};

fn default_true() -> bool { true }

// === Client → Server messages ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// List available rooms
    ListRooms,
    /// Create a new room with a game mode
    CreateRoom { mode: GameMode, #[serde(default = "default_true")] obstacles: bool },
    /// Join an existing room
    JoinRoom { room_id: String, role: ClientRole },
    /// Leave current room
    LeaveRoom,
    /// Request to play again in the same room
    PlayAgain,
    /// Upload a WASM AI module (base64-encoded bytes)
    UploadWasm { wasm_base64: String },
    /// Player command (only valid when in a room as Player)
    Command { command: PlayerCommand },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameMode {
    /// Both slots are human players
    HumanVsHuman,
    /// One human, one AI
    HumanVsAI,
    /// Both AI (spectator only)
    AIVsAI,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientRole {
    Player,
    Spectator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PlayerCommand {
    /// Move selected ship in a direction (dx, dy normalized)
    Move { dx: f32, dy: f32 },
    /// Stop moving selected ship
    StopMove,
    /// Fire from selected ship in aim direction
    Shoot,
    /// Set aim direction (normalized dx, dy from ship toward mouse)
    Aim { dx: f32, dy: f32 },
    /// Toggle auto-fire on selected ship
    ToggleAutoFire,
    /// Select next ship (Tab)
    SelectNextShip,
    /// Select a specific ship by entity id
    SelectShip { ship_id: u32 },
    /// Set the next spawn ship type
    SetSpawnType { ship_type: SpawnShipType },
    /// [Bot] Move a specific ship by ID
    MoveShip { ship_id: u32, dx: f32, dy: f32 },
    /// [Bot] Shoot from a specific ship by ID in a direction
    ShootFrom { ship_id: u32, dx: f32, dy: f32 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SpawnShipType {
    Scout,
    Tank,
    Sniper,
}

// === Server → Client messages ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Response to ListRooms
    RoomList { rooms: Vec<RoomInfo> },
    /// Room was created, client auto-joins
    RoomCreated { room_id: String, team: u8 },
    /// Successfully joined a room
    RoomJoined { room_id: String, team: u8, role: ClientRole },
    /// Room is full or doesn't exist
    JoinError { reason: String },
    /// Game has started (both slots filled)
    GameStarted { mode: GameMode },
    /// A ship was selected
    ShipSelected { ship_id: u32 },
    /// No more ships to select
    NoShipAvailable,
    /// Game ended
    GameOver { winner_team: u8 },
    /// Generic error
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub id: String,
    pub mode: GameMode,
    pub state: RoomState,
    pub players: u8,
    pub spectators: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RoomState {
    Waiting,
    Playing,
    Ended,
}
