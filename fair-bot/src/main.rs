//! STRIKE 2.5D — Fair Bot (RestrictedPlayer)
//!
//! Controls one ship at a time, just like a human.
//! Uses SelectNextShip to cycle, Move to steer, Shoot to fire.
//!
//! Usage:
//!   cargo run                    # Creates a fair-mode room and starts
//!   cargo run -- <room_id>       # Joins existing room as RestrictedPlayer

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wtransport::{ClientConfig, Endpoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let room_id = std::env::args().nth(1);

    let hash = fetch_cert_hash().await?;
    println!("[fair-bot] Got cert hash, connecting...");

    let config = ClientConfig::builder()
        .with_bind_default()
        .with_server_certificate_hashes([wtransport::tls::Sha256Digest::new(
            hash.try_into().map_err(|_| "bad hash length")?,
        )])
        .build();

    let connection = Endpoint::client(config)?
        .connect("https://localhost:4433")
        .await?;

    println!("[fair-bot] Connected!");

    // Create or join room
    let msg = if let Some(id) = &room_id {
        format!(r#"{{"type":"JoinRoom","room_id":"{}","role":"RestrictedPlayer"}}"#, id)
    } else {
        r#"{"type":"CreateRoom","mode":"HumanVsHuman","obstacles":true,"fair_mode":true}"#.to_string()
    };
    send(&connection, &msg).await?;
    println!("[fair-bot] Sent: {}", msg);

    if room_id.is_none() {
        tokio::time::sleep(Duration::from_millis(500)).await;
        send(&connection, r#"{"type":"StartGame"}"#).await?;
        println!("[fair-bot] Sent: StartGame");
    } else {
        tokio::time::sleep(Duration::from_millis(300)).await;
        send(&connection, r#"{"type":"StartGame"}"#).await?;
    }

    // Select first ship
    tokio::time::sleep(Duration::from_millis(500)).await;
    send(&connection, r#"{"type":"Command","command":{"type":"SelectNextShip"}}"#).await?;
    println!("[fair-bot] Selected first ship");

    let conn = connection.clone();
    let listen_handle = tokio::spawn(async move {
        let mut my_team: Option<u8> = None;
        let mut playing = false;
        let mut selected_ship_id: Option<u32> = None;
        let mut tick_count: u32 = 0;
        let mut switch_cooldown: u32 = 0;

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
                    match msg_type {
                        "RoomCreated" | "RoomJoined" => {
                            my_team = msg["team"].as_u64().map(|t| t as u8);
                            println!("[fair-bot] Team: {}", my_team.unwrap_or(255));
                        }
                        "GameStarted" => { playing = true; println!("[fair-bot] Game started!"); }
                        "ShipSelected" => {
                            selected_ship_id = msg["ship_id"].as_u64().map(|id| id as u32);
                            println!("[fair-bot] Selected ship: {:?}", selected_ship_id);
                        }
                        "NoShipAvailable" => { selected_ship_id = None; }
                        "GameOver" => {
                            println!("[fair-bot] Game over! Winner: team {}", msg["winner_team"].as_u64().unwrap_or(255));
                            break;
                        }
                        _ => {}
                    }
                }
            } else if prefix == b'S' && playing {
                if let Some(team) = my_team {
                    tick_count += 1;
                    if switch_cooldown > 0 { switch_cooldown -= 1; }

                    let commands = think(body, team, selected_ship_id, tick_count, &mut switch_cooldown);
                    for cmd in commands {
                        let _ = send(&conn, &cmd).await;
                    }
                }
            }
        }
    });

    listen_handle.await?;
    Ok(())
}

/// Fair bot AI: controls one ship at a time
fn think(data: &[u8], my_team: u8, selected_id: Option<u32>, tick: u32, switch_cooldown: &mut u32) -> Vec<String> {
    if data.len() < 8 { return vec![]; }

    let mut offset = 0;
    let _tick = read_u32(data, &mut offset);
    let entity_count = read_u32(data, &mut offset) as usize;

    let mut my_selected: Option<(f32, f32)> = None;
    let mut enemies: Vec<(f32, f32)> = Vec::new();
    let mut enemy_base: Option<(f32, f32)> = None;
    let mut my_ship_alive = false;

    for _ in 0..entity_count {
        if offset + 18 > data.len() { break; }

        let id = read_u32(data, &mut offset);
        let x = read_f32(data, &mut offset);
        let y = read_f32(data, &mut offset);
        let _z = read_f32(data, &mut offset);
        let entity_type = data[offset]; offset += 1;
        let team = data[offset]; offset += 1;

        let has_health = data[offset]; offset += 1;
        if has_health == 1 { offset += 4; }
        let has_max = data[offset]; offset += 1;
        if has_max == 1 { offset += 4; }

        match entity_type {
            0 => { // Ship
                if team == my_team {
                    if selected_id == Some(id) {
                        my_selected = Some((x, y));
                        my_ship_alive = true;
                    }
                } else {
                    enemies.push((x, y));
                }
            }
            2 => { // Base
                if team != my_team { enemy_base = Some((x, y)); }
            }
            _ => {}
        }
    }

    let mut commands = Vec::new();

    // If selected ship is dead, switch to next
    if selected_id.is_some() && !my_ship_alive && *switch_cooldown == 0 {
        commands.push(r#"{"type":"Command","command":{"type":"SelectNextShip"}}"#.to_string());
        *switch_cooldown = 15; // Wait half a second before switching again
        return commands;
    }

    // Periodically switch ships (every 3 seconds) to control different ones
    if tick % 90 == 0 && *switch_cooldown == 0 {
        commands.push(r#"{"type":"Command","command":{"type":"SelectNextShip"}}"#.to_string());
        *switch_cooldown = 15;
        return commands;
    }

    if let Some((sx, sy)) = my_selected {
        // Find target: nearest enemy or base
        let target = enemy_base.unwrap_or((1500.0, 0.0));
        let (tx, ty) = enemies.iter()
            .filter(|(ex, ey)| dist(*ex, *ey, sx, sy) < 600.0)
            .min_by(|a, b| {
                dist(a.0, a.1, sx, sy).partial_cmp(&dist(b.0, b.1, sx, sy)).unwrap()
            })
            .copied()
            .unwrap_or(target);

        // Move toward target
        let dx = tx - sx;
        let dy = ty - sy;
        let d = (dx * dx + dy * dy).sqrt();

        if d > 80.0 {
            let ndx = dx / d;
            let ndy = dy / d;
            commands.push(format!(
                r#"{{"type":"Command","command":{{"type":"Move","dx":{},"dy":{}}}}}"#, ndx, ndy
            ));
        } else {
            commands.push(r#"{"type":"Command","command":{"type":"StopMove"}}"#.to_string());
        }

        // Aim at target
        if d > 0.001 {
            commands.push(format!(
                r#"{{"type":"Command","command":{{"type":"Aim","dx":{},"dy":{}}}}}"#, dx / d, dy / d
            ));
        }

        // Shoot every few ticks
        if tick % 3 == 0 {
            commands.push(r#"{"type":"Command","command":{"type":"Shoot"}}"#.to_string());
        }
    }

    commands
}

fn dist(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1; let dy = y2 - y1; (dx * dx + dy * dy).sqrt()
}

// === Network ===

async fn send(conn: &wtransport::Connection, msg: &str) -> Result<(), Box<dyn std::error::Error>> {
    let opening = conn.open_bi().await?;
    let (mut s, _) = opening.await?;
    s.write_all(msg.as_bytes()).await?;
    let _ = s.finish().await;
    Ok(())
}

async fn read_stream(stream: wtransport::RecvStream) -> Vec<u8> {
    let mut stream = stream;
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf).await;
    buf
}

async fn fetch_cert_hash() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let stream = tokio::net::TcpStream::connect("localhost:4434").await?;
    let (mut reader, mut writer) = stream.into_split();
    writer.write_all(b"GET /cert-hash HTTP/1.1\r\nHost: localhost\r\n\r\n").await?;
    writer.shutdown().await?;
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf);
    let body_start = response.find('{').ok_or("no json")?;
    let json: serde_json::Value = serde_json::from_str(&response[body_start..])?;
    let hash_b64 = json["hash"].as_str().ok_or("no hash")?;
    Ok(base64_decode(hash_b64))
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

fn read_u32(data: &[u8], off: &mut usize) -> u32 {
    let v = u32::from_le_bytes(data[*off..*off+4].try_into().unwrap()); *off += 4; v
}
fn read_f32(data: &[u8], off: &mut usize) -> f32 {
    let v = f32::from_le_bytes(data[*off..*off+4].try_into().unwrap()); *off += 4; v
}
