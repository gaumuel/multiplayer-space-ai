use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use wtransport::endpoint::Endpoint;
use wtransport::ServerConfig;
use wtransport::Identity;
use wtransport::tls::Certificate;
use wtransport::tls::CertificateChain;
use wtransport::tls::PrivateKey;
use crate::network::protocol::{Snapshot, EntityType};

#[derive(Clone)]
pub struct ClientSession {
    pub id: usize,
}

pub struct WtServer {
    clients: Arc<RwLock<Vec<ClientSession>>>,
    snapshot_tx: tokio::sync::watch::Sender<Option<Snapshot>>,
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
}

impl WtServer {
    pub fn new() -> (Self, tokio::sync::watch::Receiver<Option<Snapshot>>) {
        let (snapshot_tx, snapshot_rx) = tokio::sync::watch::channel(None);
        let (cert_der, key_der) = generate_self_signed_cert().expect("Failed to generate cert");
        (
            Self {
                clients: Arc::new(RwLock::new(Vec::new())),
                snapshot_tx,
                cert_der,
                key_der,
            },
            snapshot_rx,
        )
    }

    pub async fn start(&self, port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cert_hash = sha256_hash(&self.cert_der);
        info!("Certificate SHA-256 (base64): {}", base64_encode(&cert_hash));

        // Serve cert hash over HTTP for the client
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

        let clients = self.clients.clone();
        let snapshot_rx = self.snapshot_tx.subscribe();

        tokio::spawn(async move {
            run_accept_loop(server, clients, snapshot_rx).await;
        });

        Ok(())
    }

    pub async fn push_snapshot(&self, snapshot: Snapshot) {
        let _ = self.snapshot_tx.send(Some(snapshot));
    }
}

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

async fn run_accept_loop(
    server: Endpoint<wtransport::endpoint::endpoint_side::Server>,
    clients: Arc<RwLock<Vec<ClientSession>>>,
    snapshot_rx: tokio::sync::watch::Receiver<Option<Snapshot>>,
) {
    let mut next_client_id: usize = 0;

    loop {
        let incoming_session = server.accept().await;

        let client_id = next_client_id;
        next_client_id += 1;

        info!("Incoming WebTransport connection from client {}", client_id);

        let clients = clients.clone();
        let snapshot_rx = snapshot_rx.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_client(incoming_session, client_id, &clients, snapshot_rx).await {
                warn!("Client {} disconnected: {}", client_id, e);
            }

            let mut clients = clients.write().await;
            clients.retain(|c| c.id != client_id);
            info!("Client {} removed, {} clients remaining", client_id, clients.len());
        });
    }
}

async fn handle_client(
    incoming_session: wtransport::endpoint::IncomingSession,
    client_id: usize,
    clients: &Arc<RwLock<Vec<ClientSession>>>,
    snapshot_rx: tokio::sync::watch::Receiver<Option<Snapshot>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let session_request = incoming_session.await?;
    let connection = session_request.accept().await?;
    info!("Client {} accepted", client_id);

    {
        let mut clients = clients.write().await;
        clients.push(ClientSession { id: client_id });
    }

    let connection_clone = connection.clone();
    let client_id_clone = client_id;
    let mut snapshot_rx_clone = snapshot_rx.clone();

    let send_task = tokio::spawn(async move {
        loop {
            if snapshot_rx_clone.changed().await.is_err() {
                break;
            }

            let data = {
                if let Some(snapshot) = snapshot_rx_clone.borrow().as_ref() {
                    encode_snapshot(snapshot)
                } else {
                    continue;
                }
            };

            let len = (data.len() as u32).to_le_bytes();

            let opening = connection_clone.open_uni().await;
            match opening {
                Ok(opening_stream) => {
                    match opening_stream.await {
                        Ok(mut stream) => {
                            use tokio::io::AsyncWriteExt;
                            if stream.write_all(&len).await.is_err() {
                                break;
                            }
                            if stream.write_all(&data).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Stream open failed for client {}: {}", client_id_clone, e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to open uni stream to client {}: {}", client_id_clone, e);
                    break;
                }
            }
        }
    });

    let connection_clone = connection.clone();
    let client_id_clone = client_id;
    let recv_task = tokio::spawn(async move {
        loop {
            match connection_clone.accept_uni().await {
                Ok(_) => {
                    info!("Client {} sent data", client_id_clone);
                }
                Err(_) => break,
            }
        }
    });

    let _ = tokio::join!(send_task, recv_task);

    Ok(())
}

fn generate_self_signed_cert() -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
    use time::{OffsetDateTime, Duration};

    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string()])?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params.distinguished_name.push(rcgen::DnType::CommonName, "localhost");

    // WebTransport serverCertificateHashes requires max 14 days validity
    let now = OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now + Duration::days(14);

    let key_pair = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    let cert_der = cert.der().to_vec();
    let key_der = key_pair.serialize_der();

    Ok((cert_der, key_der))
}

fn sha256_hash(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
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

fn encode_snapshot(snapshot: &Snapshot) -> Vec<u8> {
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
