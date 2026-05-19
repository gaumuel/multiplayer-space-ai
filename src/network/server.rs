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
}

impl WtServer {
    pub fn new() -> (Self, tokio::sync::watch::Receiver<Option<Snapshot>>) {
        let (snapshot_tx, snapshot_rx) = tokio::sync::watch::channel(None);
        (
            Self {
                clients: Arc::new(RwLock::new(Vec::new())),
                snapshot_tx,
            },
            snapshot_rx,
        )
    }

    pub async fn start(&self, port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (cert_der, key_der) = generate_self_signed_cert()?;

        let identity = Identity::new(
            CertificateChain::single(Certificate::from_der(cert_der)?),
            PrivateKey::from_der_pkcs8(key_der),
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

            if let Some(snapshot) = snapshot_rx_clone.borrow().as_ref() {
                let data = encode_snapshot(snapshot);

                if let Err(e) = connection_clone.send_datagram(data) {
                    warn!("Failed to send datagram to client {}: {}", client_id_clone, e);
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
    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string()])?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params.distinguished_name.push(rcgen::DnType::CommonName, "localhost");

    let key_pair = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    let cert_der = cert.der().to_vec();
    let key_der = key_pair.serialize_der();

    Ok((cert_der, key_der))
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
