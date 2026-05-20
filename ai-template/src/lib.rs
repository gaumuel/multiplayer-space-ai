//! STRIKE 2.5D — AI Template
//!
//! Build with: cargo build --target wasm32-unknown-unknown --release
//! Output: target/wasm32-unknown-unknown/release/strike_ai_template.wasm
//!
//! Upload the .wasm file to the game server to use as your AI.

use std::alloc::{alloc as std_alloc, dealloc as std_dealloc, Layout};

// === Memory management (required exports) ===

#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32 {
    let layout = Layout::from_size_align(size as usize, 8).unwrap();
    unsafe { std_alloc(layout) as u32 }
}

#[no_mangle]
pub extern "C" fn dealloc_mem(ptr: u32, size: u32) {
    let layout = Layout::from_size_align(size as usize, 8).unwrap();
    unsafe { std_dealloc(ptr as *mut u8, layout) }
}

// === AI Logic ===

/// Called every tick by the game server.
/// `state_ptr` points to serialized GameView, `state_len` is its length.
/// Returns (commands_ptr << 32) | commands_len as u64.
#[no_mangle]
pub extern "C" fn on_tick(state_ptr: u32, state_len: u32) -> u64 {
    let state = unsafe { std::slice::from_raw_parts(state_ptr as *const u8, state_len as usize) };
    let game = parse_game_view(state);
    let commands = think(&game);
    let encoded = encode_commands(&commands);

    let ptr = alloc(encoded.len() as u32);
    unsafe {
        std::ptr::copy_nonoverlapping(encoded.as_ptr(), ptr as *mut u8, encoded.len());
    }

    ((ptr as u64) << 32) | (encoded.len() as u64)
}

// === Game State Parsing ===

struct GameView {
    tick: u32,
    my_ships: Vec<ShipInfo>,
    enemy_ships: Vec<EnemyShipInfo>,
    my_base: BaseInfo,
    enemy_base: BaseInfo,
}

struct ShipInfo {
    id: u32,
    x: f32,
    y: f32,
    health: f32,
    class: u8,
}

struct EnemyShipInfo {
    id: u32,
    x: f32,
    y: f32,
    health: f32,
}

struct BaseInfo {
    x: f32,
    y: f32,
    health: f32,
}

fn parse_game_view(data: &[u8]) -> GameView {
    let mut off = 0;
    let tick = read_u32(data, &mut off);
    let _my_team = data[off]; off += 1;
    let my_ship_count = read_u16(data, &mut off) as usize;
    let enemy_ship_count = read_u16(data, &mut off) as usize;
    let _bullet_count = read_u16(data, &mut off);

    let my_base = BaseInfo { x: read_f32(data, &mut off), y: read_f32(data, &mut off), health: read_f32(data, &mut off) };
    let enemy_base = BaseInfo { x: read_f32(data, &mut off), y: read_f32(data, &mut off), health: read_f32(data, &mut off) };

    let mut my_ships = Vec::with_capacity(my_ship_count);
    for _ in 0..my_ship_count {
        my_ships.push(ShipInfo {
            id: read_u32(data, &mut off),
            x: read_f32(data, &mut off),
            y: read_f32(data, &mut off),
            health: read_f32(data, &mut off),
            class: { let v = data[off]; off += 1; v },
        });
    }

    let mut enemy_ships = Vec::with_capacity(enemy_ship_count);
    for _ in 0..enemy_ship_count {
        enemy_ships.push(EnemyShipInfo {
            id: read_u32(data, &mut off),
            x: read_f32(data, &mut off),
            y: read_f32(data, &mut off),
            health: read_f32(data, &mut off),
        });
    }

    GameView { tick, my_ships, enemy_ships, my_base, enemy_base }
}

// === AI Decision Making (customize this!) ===

enum Command {
    MoveShip { ship_id: u32, dx: f32, dy: f32 },
    ShootFrom { ship_id: u32, dx: f32, dy: f32 },
}

fn think(game: &GameView) -> Vec<Command> {
    let mut commands = Vec::new();

    for ship in &game.my_ships {
        // Default strategy: move toward enemy base
        let dx = game.enemy_base.x - ship.x;
        let dy = game.enemy_base.y - ship.y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist > 100.0 {
            commands.push(Command::MoveShip {
                ship_id: ship.id,
                dx: dx / dist,
                dy: dy / dist,
            });
        }

        // Shoot at nearest enemy ship, or enemy base if no ships
        let (tx, ty) = if let Some(enemy) = find_nearest_enemy(ship, &game.enemy_ships) {
            (enemy.x - ship.x, enemy.y - ship.y)
        } else {
            (dx, dy)
        };

        let tdist = (tx * tx + ty * ty).sqrt();
        if tdist > 0.0 {
            commands.push(Command::ShootFrom {
                ship_id: ship.id,
                dx: tx / tdist,
                dy: ty / tdist,
            });
        }
    }

    commands
}

fn find_nearest_enemy<'a>(ship: &ShipInfo, enemies: &'a [EnemyShipInfo]) -> Option<&'a EnemyShipInfo> {
    enemies.iter().min_by(|a, b| {
        let da = (a.x - ship.x).powi(2) + (a.y - ship.y).powi(2);
        let db = (b.x - ship.x).powi(2) + (b.y - ship.y).powi(2);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    })
}

// === Command Encoding ===

fn encode_commands(commands: &[Command]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(commands.len() as u16).to_le_bytes());

    for cmd in commands {
        match cmd {
            Command::MoveShip { ship_id, dx, dy } => {
                buf.push(0);
                buf.extend_from_slice(&ship_id.to_le_bytes());
                buf.extend_from_slice(&dx.to_le_bytes());
                buf.extend_from_slice(&dy.to_le_bytes());
            }
            Command::ShootFrom { ship_id, dx, dy } => {
                buf.push(1);
                buf.extend_from_slice(&ship_id.to_le_bytes());
                buf.extend_from_slice(&dx.to_le_bytes());
                buf.extend_from_slice(&dy.to_le_bytes());
            }
        }
    }

    buf
}

// === Helpers ===

fn read_u32(data: &[u8], off: &mut usize) -> u32 {
    let v = u32::from_le_bytes(data[*off..*off+4].try_into().unwrap());
    *off += 4; v
}
fn read_u16(data: &[u8], off: &mut usize) -> u16 {
    let v = u16::from_le_bytes(data[*off..*off+2].try_into().unwrap());
    *off += 2; v
}
fn read_f32(data: &[u8], off: &mut usize) -> f32 {
    let v = f32::from_le_bytes(data[*off..*off+4].try_into().unwrap());
    *off += 4; v
}
