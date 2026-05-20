use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Mutex};
use tracing::{info, warn};
use wtransport::endpoint::Endpoint;
use wtransport::ServerConfig;
use wtransport::Identity;
use wtransport::tls::Certificate;
use wtransport::tls::CertificateChain;
use wtransport::tls::PrivateKey;
use crate::network::protocol::{Snapshot, EntityType};
use crate::network::messages::*;

/// A message from a client task to the main game loop
#[derive(Debug)]
pub enum InboundEvent {
    ClientConnected { client_id: usize },
    ClientDisconnected { client_id: usize },
    Message { client_id: usize, msg: ClientMessage },
}

/// A message from the main game loop to a specific client
#[derive(Debug, Clone)]
pub enum OutboundEvent {
    Control(ServerMessage),
    Snapshot(Vec<u8>), // pre-encoded binary snapshot
}

/// Per-client sender handle
type ClientSender = mpsc::UnboundedSender<OutboundEvent>;

/// Shared state for client senders
pub type ClientSenders = Arc<RwLock<HashMap<usize, ClientSender>>>;

pub struct WtServer {
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
}

impl WtServer {
    pub fn new() -> Self {
        let (cert_der, key_der) = generate_self_signed_cert().expect("Failed to generate cert");
        Self { cert_der, key_der }
    }

    pub async fn start(
        &self,
        port: u16,
        inbound_tx: mpsc::UnboundedSender<InboundEvent>,
        client_senders: ClientSenders,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cert_hash = sha256_hash(&self.cert_der);
        info!("Certificate SHA-256 (base64): {}", base64_encode(&cert_hash));

        let hash_for_http = cert_hash.clone();
        tokio::spawn(async move {
            serve_cert_hash(hash_for_http).await;
        });

        let identity = Identity::new(
            CertificateChain::single(Certificate::from_der(self.cert_der.clone())?),
            PrivateKey::from_der_pkcs8(self.key_der.clone()),
        );

        let config = ServerConfig::builder()
            .with_bind_default(port)
            .with_identity(identity)
            .build();

        let server = Endpoint::server(config)?;
        info!("WebTransport server listening on 0.0.0.0:{}", port);

        tokio::spawn(async move {
            run_accept_loop(server, inbound_tx, client_senders).await;
        });

        Ok(())
    }
}

async fn run_accept_loop(
    server: Endpoint<wtransport::endpoint::endpoint_side::Server>,
    inbound_tx: mpsc::UnboundedSender<InboundEvent>,
    client_senders: ClientSenders,
) {
    let next_id = Arc::new(Mutex::new(0usize));

    loop {
        let incoming_session = server.accept().await;
        let inbound_tx = inbound_tx.clone();
        let client_senders = client_senders.clone();
        let next_id = next_id.clone();

        tokio::spawn(async move {
            let client_id = {
                let mut id = next_id.lock().await;
                let cid = *id;
                *id += 1;
                cid
            };

            info!("Incoming WebTransport connection from client {}", client_id);

            if let Err(e) = handle_client(incoming_session, client_id, inbound_tx.clone(), client_senders.clone()).await {
                warn!("Client {} disconnected: {}", client_id, e);
            }

            // Cleanup
            client_senders.write().await.remove(&client_id);
            let _ = inbound_tx.send(InboundEvent::ClientDisconnected { client_id });
            info!("Client {} removed", client_id);
        });
    }
}

async fn handle_client(
    incoming_session: wtransport::endpoint::IncomingSession,
    client_id: usize,
    inbound_tx: mpsc::UnboundedSender<InboundEvent>,
    client_senders: ClientSenders,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let session_request = incoming_session.await?;
    let connection = session_request.accept().await?;
    info!("Client {} accepted", client_id);

    // Create outbound channel for this client
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<OutboundEvent>();
    client_senders.write().await.insert(client_id, out_tx);

    let _ = inbound_tx.send(InboundEvent::ClientConnected { client_id });

    // Task: send outbound messages to client
    let conn_send = connection.clone();
    let send_task = tokio::spawn(async move {
        while let Some(event) = out_rx.recv().await {
            let data = match &event {
                OutboundEvent::Control(msg) => {
                    let json = serde_json::to_vec(msg).unwrap_or_default();
                    // Prefix with 'C' byte to distinguish from snapshot
                    let mut buf = Vec::with_capacity(1 + json.len());
                    buf.push(b'C');
                    buf.extend_from_slice(&json);
                    buf
                }
                OutboundEvent::Snapshot(encoded) => {
                    // Prefix with 'S' byte
                    let mut buf = Vec::with_capacity(1 + encoded.len());
                    buf.push(b'S');
                    buf.extend_from_slice(encoded);
                    buf
                }
            };

            let len = (data.len() as u32).to_le_bytes();
            let opening = conn_send.open_uni().await;
            match opening {
                Ok(opening_stream) => {
                    match opening_stream.await {
                        Ok(mut stream) => {
                            if stream.write_all(&len).await.is_err() { break; }
                            if stream.write_all(&data).await.is_err() { break; }
                        }
                        Err(_) => break,
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Task: receive inbound messages from client
    let conn_recv = connection.clone();
    let inbound_tx_clone = inbound_tx.clone();
    let recv_task = tokio::spawn(async move {
        loop {
            match conn_recv.accept_bi().await {
                Ok((_, mut recv_stream)) => {
                    use tokio::io::AsyncReadExt;
                    let mut buf = Vec::new();
                    if recv_stream.read_to_end(&mut buf).await.is_ok() {
                        if let Ok(msg) = serde_json::from_slice::<ClientMessage>(&buf) {
                            let _ = inbound_tx_clone.send(InboundEvent::Message { client_id, msg });
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    let _ = tokio::join!(send_task, recv_task);
    Ok(())
}

// === Utility functions (unchanged) ===

async fn serve_cert_hash(hash: Vec<u8>) {
    use tokio::net::TcpListener;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = match TcpListener::bind("0.0.0.0:4434").await {
        Ok(l) => l,
        Err(e) => {
            warn!("Failed to start cert hash HTTP server: {}", e);
            return;
        }
    };
    info!("Cert hash available at http://localhost:4434/cert-hash");

    loop {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;

            let hash_b64 = base64_encode(&hash);
            let body = format!("{{\"hash\":\"{}\"}}", hash_b64);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    }
}

fn generate_self_signed_cert() -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
    use time::{OffsetDateTime, Duration};

    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string()])?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params.distinguished_name.push(rcgen::DnType::CommonName, "localhost");

    let now = OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now + Duration::days(14);

    let key_pair = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    Ok((cert.der().to_vec(), key_pair.serialize_der()))
}

fn sha256_hash(data: &[u8]) -> Vec<u8> {
    let digest = ring::digest::digest(&ring::digest::SHA256, data);
    digest.as_ref().to_vec()
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

pub fn encode_snapshot(snapshot: &Snapshot) -> Vec<u8> {
    let entity_size = 4 + 4 * 3 + 1 + 1 + 1 + 4 + 1 + 4;
    let mut buf = Vec::with_capacity(8 + snapshot.entities.len() * entity_size);

    buf.extend_from_slice(&snapshot.tick.to_le_bytes());
    buf.extend_from_slice(&(snapshot.entities.len() as u32).to_le_bytes());

    for entity in &snapshot.entities {
        buf.extend_from_slice(&entity.id.to_le_bytes());
        buf.extend_from_slice(&entity.x.to_le_bytes());
        buf.extend_from_slice(&entity.y.to_le_bytes());
        buf.extend_from_slice(&entity.z.to_le_bytes());

        let entity_type = match entity.entity_type {
            EntityType::Ship => 0u8,
            EntityType::Bullet => 1,
            EntityType::Base => 2,
            EntityType::Pickup => 3,
            EntityType::Obstacle => 4,
        };
        buf.push(entity_type);
        buf.push(entity.team.unwrap_or(0xFF));

        if let Some(health) = entity.health {
            buf.push(1);
            buf.extend_from_slice(&health.to_le_bytes());
        } else {
            buf.push(0);
        }

        if let Some(max_health) = entity.max_health {
            buf.push(1);
            buf.extend_from_slice(&max_health.to_le_bytes());
        } else {
            buf.push(0);
        }
    }

    buf
}
