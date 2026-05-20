//! STRIKE 2.5D — Example Bot Client
//!
//! Connects via WebTransport, joins a room, and controls ships using AI logic.
//!
//! Usage: cargo run -- [room_id]
//!   If room_id is provided, joins that room. Otherwise creates a new HumanVsHuman room.

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wtransport::{ClientConfig, Endpoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let room_id = std::env::args().nth(1);

    // Fetch cert hash from server
    let hash = fetch_cert_hash().await?;
    println!("[bot] Got cert hash, connecting...");

    // Connect via WebTransport
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

    // Join or create room
    let msg = if let Some(id) = &room_id {
        format!(r#"{{"type":"JoinRoom","room_id":"{}","role":"Player"}}"#, id)
    } else {
        r#"{"type":"CreateRoom","mode":"HumanVsHuman"}"#.to_string()
    };
    send_message(&connection, &msg).await.map_err(|e| -> Box<dyn std::error::Error> { e })?;
    println!("[bot] Sent: {}", msg);

    // Listen for server messages and snapshots
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

            // Skip 4-byte length prefix
            let payload = &data[4..];
            let prefix = payload[0];
            let body = &payload[1..];

            if prefix == b'C' {
                // Control message (JSON)
                if let Ok(msg) = serde_json::from_slice::<serde_json::Value>(body) {
                    let msg_type = msg["type"].as_str().unwrap_or("");
                    println!("[bot] Control: {}", msg_type);

                    match msg_type {
                        "RoomCreated" | "RoomJoined" => {
                            my_team = msg["team"].as_u64().map(|t| t as u8);
                            println!("[bot] Joined as team {}", my_team.unwrap_or(255));
                        }
                        "GameStarted" => {
                            playing = true;
                            println!("[bot] Game started!");
                        }
                        "GameOver" => {
                            let winner = msg["winner_team"].as_u64().unwrap_or(255);
                            println!("[bot] Game over! Winner: team {}", winner);
                            playing = false;
                        }
                        _ => {}
                    }
                }
            } else if prefix == b'S' && playing {
                // Snapshot — parse and send commands
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

/// Parse a snapshot and return AI commands
fn process_snapshot(data: &[u8], my_team: u8) -> Vec<String> {
    if data.len() < 8 { return vec![]; }

    let mut offset = 0;
    let _tick = read_u32(data, &mut offset);
    let entity_count = read_u32(data, &mut offset) as usize;

    let mut my_ships: Vec<(u32, f32, f32)> = Vec::new();
    let mut enemy_ships: Vec<(f32, f32)> = Vec::new();
    let mut enemy_base: Option<(f32, f32)> = None;

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
        let has_max_health = data[offset]; offset += 1;
        if has_max_health == 1 { offset += 4; }

        match entity_type {
            0 => { // Ship
                if team == my_team {
                    my_ships.push((id, x, y));
                } else {
                    enemy_ships.push((x, y));
                }
            }
            2 => { // Base
                if team != my_team {
                    enemy_base = Some((x, y));
                }
            }
            _ => {}
        }
    }

    // AI: move toward enemy base, shoot at nearest enemy
    let mut commands = Vec::new();
    let target_base = enemy_base.unwrap_or((1500.0, 0.0));

    for (ship_id, sx, sy) in &my_ships {
        // Move toward enemy base
        let dx = target_base.0 - sx;
        let dy = target_base.1 - sy;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > 100.0 {
            let ndx = dx / dist;
            let ndy = dy / dist;
            commands.push(format!(
                r#"{{"type":"Command","command":{{"type":"MoveShip","ship_id":{},"dx":{},"dy":{}}}}}"#,
                ship_id, ndx, ndy
            ));
        }

        // Shoot at nearest enemy ship
        if let Some((ex, ey)) = find_nearest(*sx, *sy, &enemy_ships) {
            let dx = ex - sx;
            let dy = ey - sy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > 0.0 {
                commands.push(format!(
                    r#"{{"type":"Command","command":{{"type":"ShootFrom","ship_id":{},"dx":{},"dy":{}}}}}"#,
                    ship_id, dx / dist, dy / dist
                ));
            }
        }
    }

    commands
}

fn find_nearest(x: f32, y: f32, targets: &[(f32, f32)]) -> Option<(f32, f32)> {
    targets.iter()
        .min_by(|a, b| {
            let da = (a.0 - x).powi(2) + (a.1 - y).powi(2);
            let db = (b.0 - x).powi(2) + (b.1 - y).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
}

// === Network helpers ===

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

    // Decode base64
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

// === Binary helpers ===

fn read_u32(data: &[u8], off: &mut usize) -> u32 {
    let v = u32::from_le_bytes(data[*off..*off+4].try_into().unwrap());
    *off += 4; v
}

fn read_f32(data: &[u8], off: &mut usize) -> f32 {
    let v = f32::from_le_bytes(data[*off..*off+4].try_into().unwrap());
    *off += 4; v
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
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    out
}
