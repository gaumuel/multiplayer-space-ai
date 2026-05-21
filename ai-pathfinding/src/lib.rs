//! STRIKE 2.5D — Pathfinding AI
//!
//! Uses steering-based obstacle avoidance:
//! 1. Compute desired direction toward target
//! 2. Check for obstacles in path (ray-circle and ray-rect intersection)
//! 3. If blocked, steer around the obstacle
//! 4. Shoot at nearest enemy if line-of-sight is clear

use std::alloc::{alloc as std_alloc, dealloc as std_dealloc, Layout};

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

#[no_mangle]
pub extern "C" fn on_tick(state_ptr: u32, state_len: u32) -> u64 {
    let state = unsafe { std::slice::from_raw_parts(state_ptr as *const u8, state_len as usize) };
    let game = parse_game_view(state);
    let commands = think(&game);
    let encoded = encode_commands(&commands);

    let ptr = alloc(encoded.len() as u32);
    unsafe { std::ptr::copy_nonoverlapping(encoded.as_ptr(), ptr as *mut u8, encoded.len()); }
    ((ptr as u64) << 32) | (encoded.len() as u64)
}

// === Data Structures ===

struct GameView {
    my_ships: Vec<ShipInfo>,
    enemy_ships: Vec<EnemyShipInfo>,
    my_base: Base,
    enemy_base: Base,
    obstacles: Vec<ObstacleInfo>,
}

struct ShipInfo { id: u32, x: f32, y: f32, health: f32, class: u8 }
struct EnemyShipInfo { id: u32, x: f32, y: f32, health: f32 }
struct Base { x: f32, y: f32, health: f32 }

struct ObstacleInfo {
    x: f32, y: f32,
    w: f32, h: f32,  // width and height (for both circle diameter and rect dimensions)
    is_rect: bool,
}

enum Command {
    MoveShip { ship_id: u32, dx: f32, dy: f32 },
    ShootFrom { ship_id: u32, dx: f32, dy: f32 },
}

// === AI Logic ===

fn think(game: &GameView) -> Vec<Command> {
    let mut commands = Vec::new();

    for ship in &game.my_ships {
        // Pick target: nearest enemy ship, or enemy base
        let (target_x, target_y) = pick_target(ship, &game.enemy_ships, &game.enemy_base);

        // Compute direction with obstacle avoidance
        let (move_dx, move_dy) = avoid_obstacles(
            ship.x, ship.y, target_x, target_y, &game.obstacles
        );

        // Move toward target (with avoidance)
        let dist_to_target = dist(ship.x, ship.y, target_x, target_y);
        if dist_to_target > 80.0 {
            commands.push(Command::MoveShip { ship_id: ship.id, dx: move_dx, dy: move_dy });
        }

        // Shoot at nearest enemy if we have line of sight
        if let Some((shoot_dx, shoot_dy)) = find_shoot_target(ship, &game.enemy_ships, &game.enemy_base, &game.obstacles) {
            commands.push(Command::ShootFrom { ship_id: ship.id, dx: shoot_dx, dy: shoot_dy });
        }
    }

    commands
}

fn pick_target(ship: &ShipInfo, enemies: &[EnemyShipInfo], enemy_base: &Base) -> (f32, f32) {
    // Target nearest enemy within 800 units, otherwise go for base
    let mut nearest_dist = 800.0f32;
    let mut target = (enemy_base.x, enemy_base.y);

    for enemy in enemies {
        let d = dist(ship.x, ship.y, enemy.x, enemy.y);
        if d < nearest_dist {
            nearest_dist = d;
            target = (enemy.x, enemy.y);
        }
    }

    target
}

fn avoid_obstacles(sx: f32, sy: f32, tx: f32, ty: f32, obstacles: &[ObstacleInfo]) -> (f32, f32) {
    let dx = tx - sx;
    let dy = ty - sy;
    let d = (dx * dx + dy * dy).sqrt();
    if d < 0.001 { return (0.0, 0.0); }

    let dir_x = dx / d;
    let dir_y = dy / d;

    // Check if any obstacle is in our path (within look-ahead distance)
    let look_ahead = 250.0f32;
    let mut best_avoidance = (dir_x, dir_y);
    let mut closest_hit = look_ahead;

    for obs in obstacles {
        let margin = if obs.is_rect {
            (obs.w.max(obs.h)) / 2.0 + 30.0
        } else {
            obs.w / 2.0 + 30.0
        };

        // Vector from ship to obstacle center
        let to_obs_x = obs.x - sx;
        let to_obs_y = obs.y - sy;

        // Project onto movement direction
        let proj = to_obs_x * dir_x + to_obs_y * dir_y;
        if proj < 0.0 || proj > look_ahead { continue; } // Behind us or too far

        // Perpendicular distance from our path to obstacle center
        let perp_x = to_obs_x - proj * dir_x;
        let perp_y = to_obs_y - proj * dir_y;
        let perp_dist = (perp_x * perp_x + perp_y * perp_y).sqrt();

        // Check if we'd hit it
        let hit_radius = if obs.is_rect {
            // Approximate: use half-diagonal
            ((obs.w / 2.0).powi(2) + (obs.h / 2.0).powi(2)).sqrt() + 20.0
        } else {
            obs.w / 2.0 + 20.0
        };

        if perp_dist < hit_radius && proj < closest_hit {
            closest_hit = proj;

            // Steer away: perpendicular to the direction toward obstacle
            if perp_dist > 0.001 {
                // Steer in the direction we're already offset
                let steer_x = -perp_x / perp_dist;
                let steer_y = -perp_y / perp_dist;

                // Blend: more steering when closer to obstacle
                let urgency = 1.0 - (proj / look_ahead);
                let blend = urgency.clamp(0.3, 0.9);

                let ax = dir_x * (1.0 - blend) + steer_x * blend;
                let ay = dir_y * (1.0 - blend) + steer_y * blend;
                let len = (ax * ax + ay * ay).sqrt();
                if len > 0.001 {
                    best_avoidance = (ax / len, ay / len);
                }
            } else {
                // Directly in front — steer right
                best_avoidance = (dir_y, -dir_x);
            }
        }
    }

    best_avoidance
}

fn find_shoot_target(ship: &ShipInfo, enemies: &[EnemyShipInfo], enemy_base: &Base, obstacles: &[ObstacleInfo]) -> Option<(f32, f32)> {
    // Find nearest enemy with clear line of sight
    let mut best: Option<(f32, f32, f32)> = None; // (dx, dy, dist)

    for enemy in enemies {
        let d = dist(ship.x, ship.y, enemy.x, enemy.y);
        if d > 2000.0 { continue; } // Out of bullet range

        if !line_blocked(ship.x, ship.y, enemy.x, enemy.y, obstacles) {
            if best.map_or(true, |(_, _, bd)| d < bd) {
                let dx = enemy.x - ship.x;
                let dy = enemy.y - ship.y;
                best = Some((dx / d, dy / d, d));
            }
        }
    }

    // If no enemy in sight, shoot at base if clear
    if best.is_none() {
        let d = dist(ship.x, ship.y, enemy_base.x, enemy_base.y);
        if d < 2000.0 && !line_blocked(ship.x, ship.y, enemy_base.x, enemy_base.y, obstacles) {
            let dx = enemy_base.x - ship.x;
            let dy = enemy_base.y - ship.y;
            best = Some((dx / d, dy / d, d));
        }
    }

    best.map(|(dx, dy, _)| (dx, dy))
}

fn line_blocked(x1: f32, y1: f32, x2: f32, y2: f32, obstacles: &[ObstacleInfo]) -> bool {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let d = (dx * dx + dy * dy).sqrt();
    if d < 0.001 { return false; }

    let dir_x = dx / d;
    let dir_y = dy / d;

    for obs in obstacles {
        let to_obs_x = obs.x - x1;
        let to_obs_y = obs.y - y1;
        let proj = to_obs_x * dir_x + to_obs_y * dir_y;
        if proj < 0.0 || proj > d { continue; }

        let perp_x = to_obs_x - proj * dir_x;
        let perp_y = to_obs_y - proj * dir_y;
        let perp_dist = (perp_x * perp_x + perp_y * perp_y).sqrt();

        let radius = if obs.is_rect {
            (obs.w / 2.0).min(obs.h / 2.0) // Use min for tighter check
        } else {
            obs.w / 2.0
        };

        if perp_dist < radius {
            return true;
        }
    }

    false
}

fn dist(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    (dx * dx + dy * dy).sqrt()
}

// === Parsing ===

fn parse_game_view(data: &[u8]) -> GameView {
    let mut off = 0;
    let _tick = read_u32(data, &mut off);
    let _my_team = data[off]; off += 1;
    let my_ship_count = read_u16(data, &mut off) as usize;
    let enemy_ship_count = read_u16(data, &mut off) as usize;
    let bullet_count = read_u16(data, &mut off) as usize;

    let my_base = Base { x: read_f32(data, &mut off), y: read_f32(data, &mut off), health: read_f32(data, &mut off) };
    let enemy_base = Base { x: read_f32(data, &mut off), y: read_f32(data, &mut off), health: read_f32(data, &mut off) };

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

    // Skip bullets
    for _ in 0..bullet_count {
        off += 4 * 4 + 1; // x, y, vx, vy, team
    }

    // Parse obstacles
    let mut obstacles = Vec::new();
    if off + 2 <= data.len() {
        let obs_count = read_u16(data, &mut off) as usize;
        for _ in 0..obs_count {
            if off + 17 > data.len() { break; }
            let x = read_f32(data, &mut off);
            let y = read_f32(data, &mut off);
            let w = read_f32(data, &mut off);
            let h = read_f32(data, &mut off);
            let is_rect = data[off] == 1; off += 1;
            obstacles.push(ObstacleInfo { x, y, w, h, is_rect });
        }
    }

    GameView { my_ships, enemy_ships, my_base, enemy_base, obstacles }
}

// === Encoding ===

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

fn read_u32(data: &[u8], off: &mut usize) -> u32 {
    let v = u32::from_le_bytes(data[*off..*off+4].try_into().unwrap()); *off += 4; v
}
fn read_u16(data: &[u8], off: &mut usize) -> u16 {
    let v = u16::from_le_bytes(data[*off..*off+2].try_into().unwrap()); *off += 2; v
}
fn read_f32(data: &[u8], off: &mut usize) -> f32 {
    let v = f32::from_le_bytes(data[*off..*off+4].try_into().unwrap()); *off += 4; v
}
