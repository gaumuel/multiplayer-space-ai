//! STRIKE 2.5D — Bot Client with Pathfinding AI
//!
//! Connects via WebTransport, joins a room, and controls ships with obstacle avoidance.
//!
//! Usage: cargo run -- [room_id]

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wtransport::{ClientConfig, Endpoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let room_id = std::env::args().nth(1);
    let wait_mode = std::env::args().any(|a| a == "--wait");

    let hash = fetch_cert_hash().await?;
    println!("[bot] Got cert hash, connecting...");

    let config = ClientConfig::builder()
        .with_bind_default()
        .with_server_certificate_hashes([wtransport::tls::Sha256Digest::new(
            hash.try_into().map_err(|_| "bad hash length")?,
        )])
        .build();

    let connection = Endpoint::client(config)?
        .connect("https://localhost:4433")
        .await?;

    println!("[bot] Connected!");

    let msg = if let Some(id) = &room_id {
        format!(r#"{{"type":"JoinRoom","room_id":"{}","role":"Player"}}"#, id)
    } else {
        r#"{"type":"CreateRoom","mode":"HumanVsHuman","obstacles":true}"#.to_string()
    };
    send_message(&connection, &msg).await.map_err(|e| -> Box<dyn std::error::Error> { e })?;
    println!("[bot] Sent: {}", msg);

    if room_id.is_none() && !wait_mode {
        tokio::time::sleep(Duration::from_millis(500)).await;
        send_message(&connection, r#"{"type":"StartGame"}"#).await.map_err(|e| -> Box<dyn std::error::Error> { e })?;
        println!("[bot] Sent: StartGame");
    } else if room_id.is_some() {
        // Joining an existing room — send StartGame to begin
        tokio::time::sleep(Duration::from_millis(500)).await;
        send_message(&connection, r#"{"type":"StartGame"}"#).await.map_err(|e| -> Box<dyn std::error::Error> { e })?;
        println!("[bot] Sent: StartGame");
    } else if wait_mode {
        println!("[bot] Waiting mode — join this room and press Start from another client");
    }

    let conn = connection.clone();
    let listen_handle = tokio::spawn(async move {
        let mut my_team: Option<u8> = None;
        let mut playing = false;

        loop {
            let stream = match conn.accept_uni().await {
                Ok(s) => s,
                Err(_) => break,
            };

            let data = read_stream(stream).await;
            if data.len() < 5 { continue; }

            let payload = &data[4..];
            let prefix = payload[0];
            let body = &payload[1..];

            if prefix == b'C' {
                if let Ok(msg) = serde_json::from_slice::<serde_json::Value>(body) {
                    let msg_type = msg["type"].as_str().unwrap_or("");
                    println!("[bot] Control: {}", msg_type);
                    match msg_type {
                        "RoomCreated" | "RoomJoined" => {
                            my_team = msg["team"].as_u64().map(|t| t as u8);
                            println!("[bot] Joined as team {}", my_team.unwrap_or(255));
                        }
                        "GameStarted" => { playing = true; println!("[bot] Game started!"); }
                        "GameOver" => {
                            println!("[bot] Game over! Winner: team {}", msg["winner_team"].as_u64().unwrap_or(255));
                            playing = false;
                        }
                        _ => {}
                    }
                }
            } else if prefix == b'S' && playing {
                if let Some(team) = my_team {
                    let commands = process_snapshot(body, team);
                    for cmd in commands {
                        let _ = send_message(&conn, &cmd).await;
                    }
                }
            }
        }
    });

    listen_handle.await?;
    Ok(())
}

// === AI with Pathfinding ===

struct Ship { id: u32, x: f32, y: f32 }
struct Enemy { x: f32, y: f32 }
struct Obstacle { x: f32, y: f32, w: f32, h: f32, is_rect: bool }

fn process_snapshot(data: &[u8], my_team: u8) -> Vec<String> {
    if data.len() < 8 { return vec![]; }

    let mut offset = 0;
    let _tick = read_u32(data, &mut offset);
    let entity_count = read_u32(data, &mut offset) as usize;

    let mut my_ships: Vec<Ship> = Vec::new();
    let mut enemy_ships: Vec<Enemy> = Vec::new();
    let mut enemy_base: Option<(f32, f32)> = None;
    let mut obstacles: Vec<Obstacle> = Vec::new();

    for _ in 0..entity_count {
        if offset + 18 > data.len() { break; }

        let id = read_u32(data, &mut offset);
        let x = read_f32(data, &mut offset);
        let y = read_f32(data, &mut offset);
        let _z = read_f32(data, &mut offset);
        let entity_type = data[offset]; offset += 1;
        let team = data[offset]; offset += 1;

        let has_health = data[offset]; offset += 1;
        let mut health_val = 0.0f32;
        if has_health == 1 { health_val = read_f32(data, &mut offset); }
        let has_max_health = data[offset]; offset += 1;
        let mut max_health_val = 0.0f32;
        if has_max_health == 1 { max_health_val = read_f32(data, &mut offset); }

        match entity_type {
            0 => { // Ship
                if team == my_team {
                    my_ships.push(Ship { id, x, y });
                } else {
                    enemy_ships.push(Enemy { x, y });
                }
            }
            2 => { // Base
                if team != my_team {
                    enemy_base = Some((x, y));
                }
            }
            4 => { // Obstacle
                // team byte: 0=rect, 1=circle
                // health=width, max_health=height
                let is_rect = team == 0;
                obstacles.push(Obstacle { x, y, w: health_val, h: max_health_val, is_rect });
            }
            _ => {}
        }
    }

    let target_base = enemy_base.unwrap_or((1500.0, 0.0));
    let mut commands = Vec::new();

    for ship in &my_ships {
        // Pick target: nearest enemy within 800 units, or enemy base
        let (target_x, target_y) = pick_target(ship, &enemy_ships, target_base);

        // Move with obstacle avoidance
        let (move_dx, move_dy) = avoid_obstacles(ship.x, ship.y, target_x, target_y, &obstacles);
        let dist_to_target = dist(ship.x, ship.y, target_x, target_y);

        if dist_to_target > 80.0 {
            commands.push(format!(
                r#"{{"type":"Command","command":{{"type":"MoveShip","ship_id":{},"dx":{},"dy":{}}}}}"#,
                ship.id, move_dx, move_dy
            ));
        }

        // Shoot at nearest enemy with line of sight
        if let Some((sdx, sdy)) = find_shoot_target(ship, &enemy_ships, target_base, &obstacles) {
            commands.push(format!(
                r#"{{"type":"Command","command":{{"type":"ShootFrom","ship_id":{},"dx":{},"dy":{}}}}}"#,
                ship.id, sdx, sdy
            ));
        }
    }

    commands
}

fn pick_target(ship: &Ship, enemies: &[Enemy], base: (f32, f32)) -> (f32, f32) {
    let mut nearest_dist = 800.0f32;
    let mut target = base;
    for e in enemies {
        let d = dist(ship.x, ship.y, e.x, e.y);
        if d < nearest_dist { nearest_dist = d; target = (e.x, e.y); }
    }
    target
}

fn avoid_obstacles(sx: f32, sy: f32, tx: f32, ty: f32, obstacles: &[Obstacle]) -> (f32, f32) {
    let dx = tx - sx;
    let dy = ty - sy;
    let d = (dx * dx + dy * dy).sqrt();
    if d < 0.001 { return (0.0, 0.0); }

    let dir_x = dx / d;
    let dir_y = dy / d;

    // Check if direct path to target is blocked
    let blocking_obs = find_blocking_obstacle(sx, sy, tx, ty, obstacles);

    if let Some(obs) = blocking_obs {
        // Path is blocked — compute a waypoint around the obstacle
        let (wp_x, wp_y) = compute_waypoint(sx, sy, tx, ty, obs);
        let wp_dx = wp_x - sx;
        let wp_dy = wp_y - sy;
        let wp_d = (wp_dx * wp_dx + wp_dy * wp_dy).sqrt();
        if wp_d > 0.001 {
            let mut final_x = wp_dx / wp_d;
            let mut final_y = wp_dy / wp_d;

            // Add repulsion from very close obstacles
            add_repulsion(sx, sy, obstacles, &mut final_x, &mut final_y);

            let len = (final_x * final_x + final_y * final_y).sqrt();
            if len > 0.001 { return (final_x / len, final_y / len); }
        }
    }

    // No blocking obstacle — go direct, but still avoid nearby ones
    let mut final_x = dir_x;
    let mut final_y = dir_y;
    add_repulsion(sx, sy, obstacles, &mut final_x, &mut final_y);

    let len = (final_x * final_x + final_y * final_y).sqrt();
    if len > 0.001 { (final_x / len, final_y / len) } else { (dir_x, dir_y) }
}

fn find_blocking_obstacle<'a>(sx: f32, sy: f32, tx: f32, ty: f32, obstacles: &'a [Obstacle]) -> Option<&'a Obstacle> {
    let dx = tx - sx;
    let dy = ty - sy;
    let d = (dx * dx + dy * dy).sqrt();
    if d < 0.001 { return None; }
    let dir_x = dx / d;
    let dir_y = dy / d;

    let mut closest: Option<(&Obstacle, f32)> = None;

    for obs in obstacles {
        let to_x = obs.x - sx;
        let to_y = obs.y - sy;
        let proj = to_x * dir_x + to_y * dir_y;
        if proj < 20.0 || proj > d { continue; }

        let perp_x = to_x - proj * dir_x;
        let perp_y = to_y - proj * dir_y;
        let perp_dist = (perp_x * perp_x + perp_y * perp_y).sqrt();

        let hit_radius = if obs.is_rect {
            ((obs.w / 2.0).powi(2) + (obs.h / 2.0).powi(2)).sqrt() + 25.0
        } else {
            obs.w / 2.0 + 25.0
        };

        if perp_dist < hit_radius {
            if closest.map_or(true, |(_, cp)| proj < cp) {
                closest = Some((obs, proj));
            }
        }
    }

    closest.map(|(obs, _)| obs)
}

fn compute_waypoint(sx: f32, sy: f32, tx: f32, ty: f32, obs: &Obstacle) -> (f32, f32) {
    // Go around the obstacle — pick the side that's closer to the target
    let clearance = if obs.is_rect {
        obs.w.max(obs.h) / 2.0 + 50.0
    } else {
        obs.w / 2.0 + 50.0
    };

    // Two candidate waypoints: left and right of obstacle
    let to_target_x = tx - obs.x;
    let to_target_y = ty - obs.y;
    let to_target_d = (to_target_x * to_target_x + to_target_y * to_target_y).sqrt().max(0.001);

    // Perpendicular to the ship→obstacle direction
    let to_obs_x = obs.x - sx;
    let to_obs_y = obs.y - sy;
    let to_obs_d = (to_obs_x * to_obs_x + to_obs_y * to_obs_y).sqrt().max(0.001);
    let perp1_x = -to_obs_y / to_obs_d;
    let perp1_y = to_obs_x / to_obs_d;

    let wp1_x = obs.x + perp1_x * clearance;
    let wp1_y = obs.y + perp1_y * clearance;
    let wp2_x = obs.x - perp1_x * clearance;
    let wp2_y = obs.y - perp1_y * clearance;

    // Pick the waypoint closer to the target
    let d1 = dist(wp1_x, wp1_y, tx, ty) + dist(sx, sy, wp1_x, wp1_y);
    let d2 = dist(wp2_x, wp2_y, tx, ty) + dist(sx, sy, wp2_x, wp2_y);

    if d1 < d2 { (wp1_x, wp1_y) } else { (wp2_x, wp2_y) }
}

fn add_repulsion(sx: f32, sy: f32, obstacles: &[Obstacle], fx: &mut f32, fy: &mut f32) {
    for obs in obstacles {
        let to_x = sx - obs.x;
        let to_y = sy - obs.y;
        let obs_dist = (to_x * to_x + to_y * to_y).sqrt();

        let danger_radius = if obs.is_rect {
            ((obs.w / 2.0).powi(2) + (obs.h / 2.0).powi(2)).sqrt() + 40.0
        } else {
            obs.w / 2.0 + 40.0
        };

        if obs_dist < danger_radius && obs_dist > 0.001 {
            let strength = (1.0 - obs_dist / danger_radius).powi(2) * 2.0;
            *fx += (to_x / obs_dist) * strength;
            *fy += (to_y / obs_dist) * strength;
        }
    }
}

fn find_shoot_target(ship: &Ship, enemies: &[Enemy], base: (f32, f32), obstacles: &[Obstacle]) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32, f32)> = None;

    for e in enemies {
        let d = dist(ship.x, ship.y, e.x, e.y);
        if d > 2000.0 { continue; }
        if !line_blocked(ship.x, ship.y, e.x, e.y, obstacles) {
            if best.map_or(true, |(_, _, bd)| d < bd) {
                let dx = e.x - ship.x;
                let dy = e.y - ship.y;
                best = Some((dx / d, dy / d, d));
            }
        }
    }

    if best.is_none() {
        let d = dist(ship.x, ship.y, base.0, base.1);
        if d < 2000.0 && !line_blocked(ship.x, ship.y, base.0, base.1, obstacles) {
            let dx = base.0 - ship.x;
            let dy = base.1 - ship.y;
            best = Some((dx / d, dy / d, d));
        }
    }

    best.map(|(dx, dy, _)| (dx, dy))
}

fn line_blocked(x1: f32, y1: f32, x2: f32, y2: f32, obstacles: &[Obstacle]) -> bool {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let d = (dx * dx + dy * dy).sqrt();
    if d < 0.001 { return false; }
    let dir_x = dx / d;
    let dir_y = dy / d;

    for obs in obstacles {
        let to_x = obs.x - x1;
        let to_y = obs.y - y1;
        let proj = to_x * dir_x + to_y * dir_y;
        if proj < 0.0 || proj > d { continue; }
        let perp_x = to_x - proj * dir_x;
        let perp_y = to_y - proj * dir_y;
        let perp_dist = (perp_x * perp_x + perp_y * perp_y).sqrt();
        let radius = if obs.is_rect { obs.w.min(obs.h) / 2.0 } else { obs.w / 2.0 };
        if perp_dist < radius { return true; }
    }
    false
}

fn dist(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1; let dy = y2 - y1; (dx * dx + dy * dy).sqrt()
}

// === Network ===

async fn fetch_cert_hash() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let stream = tokio::net::TcpStream::connect("localhost:4434").await?;
    let (mut reader, mut writer) = stream.into_split();
    writer.write_all(b"GET /cert-hash HTTP/1.1\r\nHost: localhost\r\n\r\n").await?;
    writer.shutdown().await?;
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf);
    let body_start = response.find('{').ok_or("no json body")?;
    let json: serde_json::Value = serde_json::from_str(&response[body_start..])?;
    let hash_b64 = json["hash"].as_str().ok_or("no hash field")?;
    Ok(base64_decode(hash_b64))
}

async fn send_message(conn: &wtransport::Connection, msg: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let opening = conn.open_bi().await?;
    let (mut send, _recv) = opening.await?;
    send.write_all(msg.as_bytes()).await?;
    let _ = send.finish().await;
    Ok(())
}

async fn read_stream(stream: wtransport::RecvStream) -> Vec<u8> {
    let mut stream = stream;
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf).await;
    buf
}

fn read_u32(data: &[u8], off: &mut usize) -> u32 {
    let v = u32::from_le_bytes(data[*off..*off+4].try_into().unwrap()); *off += 4; v
}
fn read_f32(data: &[u8], off: &mut usize) -> f32 {
    let v = f32::from_le_bytes(data[*off..*off+4].try_into().unwrap()); *off += 4; v
}

fn base64_decode(input: &str) -> Vec<u8> {
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
        if b >= 128 { break; }
        let val = TABLE[b as usize];
        if val == 255 { break; }
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 { bits -= 8; out.push((buf >> bits) as u8); buf &= (1 << bits) - 1; }
    }
    out
}
