/// WASM AI Interface
///
/// The WASM module must export:
///   - `alloc(size: u32) -> u32` — allocate memory for the host to write into
///   - `dealloc(ptr: u32, size: u32)` — free memory
///   - `on_tick(state_ptr: u32, state_len: u32) -> u64` — process tick, returns (ptr << 32 | len)
///
/// The host calls `on_tick` each game tick with a serialized GameView.
/// The module returns a pointer+length to serialized commands.

/// Binary format for game state passed to WASM (little-endian):
///
/// Header:
///   tick: u32
///   my_team: u8
///   my_ship_count: u16
///   enemy_ship_count: u16
///   bullet_count: u16
///   my_base_x: f32, my_base_y: f32, my_base_health: f32
///   enemy_base_x: f32, enemy_base_y: f32, enemy_base_health: f32
///
/// Per my_ship (repeated my_ship_count times):
///   id: u32, x: f32, y: f32, health: f32, ship_class: u8
///
/// Per enemy_ship (repeated enemy_ship_count times):
///   id: u32, x: f32, y: f32, health: f32
///
/// Per bullet (repeated bullet_count times):
///   x: f32, y: f32, vx: f32, vy: f32, team: u8

/// Binary format for commands returned from WASM:
///
/// Header:
///   command_count: u16
///
/// Per command:
///   type: u8
///     0 = MoveShip: ship_id: u32, dx: f32, dy: f32
///     1 = ShootFrom: ship_id: u32, dx: f32, dy: f32
///     2 = SetSpawnType: ship_type: u8 (0=Scout, 1=Tank, 2=Sniper)

use crate::components::*;
use crate::room::{Room, PlayerSlot};
use bevy_ecs::prelude::*;

/// Serialize game state into the binary format for WASM
pub fn serialize_game_view(room: &mut Room, slot: PlayerSlot) -> Vec<u8> {
    let team = slot.team();
    let mut buf = Vec::with_capacity(1024);

    // Header
    buf.extend_from_slice(&room.tick_count.to_le_bytes());
    buf.push(slot.team_id());

    // Collect ships
    let mut my_ships = Vec::new();
    let mut enemy_ships = Vec::new();
    let mut ship_query = room.world.query::<(Entity, &Position, &Ship, &Health)>();
    for (entity, pos, ship, health) in ship_query.iter(&room.world) {
        if ship.team == team {
            my_ships.push((entity, pos.x, pos.y, health.current, ship.class));
        } else {
            enemy_ships.push((entity, pos.x, pos.y, health.current));
        }
    }

    // Collect bullets
    let mut bullets = Vec::new();
    let mut bullet_query = room.world.query::<(&Position, &Velocity, &Bullet)>();
    for (pos, vel, bullet) in bullet_query.iter(&room.world) {
        bullets.push((pos.x, pos.y, vel.x, vel.y, bullet.team));
    }

    // Collect bases
    let mut my_base = (0.0f32, 0.0f32, 0.0f32);
    let mut enemy_base = (0.0f32, 0.0f32, 0.0f32);
    let mut base_query = room.world.query::<(&Position, &Base, &Health)>();
    for (pos, base, health) in base_query.iter(&room.world) {
        if base.team == team {
            my_base = (pos.x, pos.y, health.current);
        } else {
            enemy_base = (pos.x, pos.y, health.current);
        }
    }

    buf.extend_from_slice(&(my_ships.len() as u16).to_le_bytes());
    buf.extend_from_slice(&(enemy_ships.len() as u16).to_le_bytes());
    buf.extend_from_slice(&(bullets.len() as u16).to_le_bytes());
    buf.extend_from_slice(&my_base.0.to_le_bytes());
    buf.extend_from_slice(&my_base.1.to_le_bytes());
    buf.extend_from_slice(&my_base.2.to_le_bytes());
    buf.extend_from_slice(&enemy_base.0.to_le_bytes());
    buf.extend_from_slice(&enemy_base.1.to_le_bytes());
    buf.extend_from_slice(&enemy_base.2.to_le_bytes());

    // My ships
    for (entity, x, y, health, class) in &my_ships {
        buf.extend_from_slice(&(entity.to_bits() as u32).to_le_bytes());
        buf.extend_from_slice(&x.to_le_bytes());
        buf.extend_from_slice(&y.to_le_bytes());
        buf.extend_from_slice(&health.to_le_bytes());
        buf.push(match class {
            ShipClass::Scout => 0,
            ShipClass::Tank => 1,
            ShipClass::Sniper => 2,
        });
    }

    // Enemy ships
    for (entity, x, y, health) in &enemy_ships {
        buf.extend_from_slice(&(entity.to_bits() as u32).to_le_bytes());
        buf.extend_from_slice(&x.to_le_bytes());
        buf.extend_from_slice(&y.to_le_bytes());
        buf.extend_from_slice(&health.to_le_bytes());
    }

    // Bullets
    for (x, y, vx, vy, bteam) in &bullets {
        buf.extend_from_slice(&x.to_le_bytes());
        buf.extend_from_slice(&y.to_le_bytes());
        buf.extend_from_slice(&vx.to_le_bytes());
        buf.extend_from_slice(&vy.to_le_bytes());
        buf.push(if *bteam == team { 0 } else { 1 });
    }

    buf
}

/// Parse commands returned from WASM
pub fn parse_wasm_commands(data: &[u8]) -> Vec<WasmCommand> {
    if data.len() < 2 { return Vec::new(); }

    let count = u16::from_le_bytes([data[0], data[1]]) as usize;
    let mut commands = Vec::with_capacity(count);
    let mut offset = 2;

    for _ in 0..count {
        if offset >= data.len() { break; }
        let cmd_type = data[offset];
        offset += 1;

        match cmd_type {
            0 => { // MoveShip
                if offset + 12 > data.len() { break; }
                let ship_id = u32::from_le_bytes(data[offset..offset+4].try_into().unwrap());
                let dx = f32::from_le_bytes(data[offset+4..offset+8].try_into().unwrap());
                let dy = f32::from_le_bytes(data[offset+8..offset+12].try_into().unwrap());
                offset += 12;
                commands.push(WasmCommand::MoveShip { ship_id, dx, dy });
            }
            1 => { // ShootFrom
                if offset + 12 > data.len() { break; }
                let ship_id = u32::from_le_bytes(data[offset..offset+4].try_into().unwrap());
                let dx = f32::from_le_bytes(data[offset+4..offset+8].try_into().unwrap());
                let dy = f32::from_le_bytes(data[offset+8..offset+12].try_into().unwrap());
                offset += 12;
                commands.push(WasmCommand::ShootFrom { ship_id, dx, dy });
            }
            2 => { // SetSpawnType
                if offset + 1 > data.len() { break; }
                let ship_type = data[offset];
                offset += 1;
                commands.push(WasmCommand::SetSpawnType { ship_type });
            }
            _ => break,
        }
    }

    commands
}

#[derive(Debug)]
pub enum WasmCommand {
    MoveShip { ship_id: u32, dx: f32, dy: f32 },
    ShootFrom { ship_id: u32, dx: f32, dy: f32 },
    SetSpawnType { ship_type: u8 },
}
