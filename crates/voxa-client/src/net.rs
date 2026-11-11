//! Control and bulk TCP connections to the server.

use std::net::SocketAddr;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::sync::{mpsc, Notify};
use tokio::time::timeout;
use voxa_proto::identity::{fingerprint, KeyPair};
use voxa_proto::transport;
use voxa_proto::*;

pub enum NetEvent {
    /// Pinned key did not match; the user must confirm before reconnecting.
    FingerprintMismatch { address: String, expected: String, got: String },
    /// First connection to this server; pin the key.
    NewFingerprint { address: String, fingerprint: String },
    Connected { server_addr: SocketAddr },
    Msg(S2C),
    Bulk(BulkS2C),
    BulkReady,
    Disconnected(String),
}

pub struct ConnectParams {
    pub address: String,
    pub identity: KeyPair,
    pub pinned: Option<String>,
    pub hello: C2S,
}

pub struct Connection {
    pub tx: mpsc::UnboundedSender<C2S>,
    pub bulk_tx: mpsc::Sender<BulkC2S>,
    bulk_rx_slot: Option<mpsc::Receiver<BulkC2S>>,
    shutdown: Arc<Notify>,
}

impl Connection {
    pub fn send(&self, m: C2S) {
        let _ = self.tx.send(m);
    }

    pub fn close(&self, msg: &str) {
        let _ = self.tx.send(C2S::Disconnect(msg.to_string()));
        let s = self.shutdown.clone();
        // Give the writer a moment to flush the goodbye.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            s.notify_waiters();
            s.notify_one();
        });
    }
}

pub fn connect(rt: &tokio::runtime::Handle, p: ConnectParams, events: Sender<NetEvent>, ctx: egui::Context) -> Connection {
    let (tx, rx) = mpsc::unbounded_channel();
    let (bulk_tx, bulk_rx) = mpsc::channel(16);
    let shutdown = Arc::new(Notify::new());
    let mut conn = Connection { tx, bulk_tx, bulk_rx_slot: Some(bulk_rx), shutdown: shutdown.clone() };
    let bulk_rx = conn.bulk_rx_slot.take().unwrap();
    rt.spawn(async move {
        let ev = events.clone();
        let c2 = ctx.clone();
        let reason = match run(p, rx, bulk_rx, events, ctx, shutdown).await {
            Ok(r) => r,
            Err(e) => e.to_string(),
        };
        let _ = ev.send(NetEvent::Disconnected(reason));
        c2.request_repaint();
    });
    conn
}

async fn run(
    p: ConnectParams,
    mut rx: mpsc::UnboundedReceiver<C2S>,
    bulk_rx: mpsc::Receiver<BulkC2S>,
    events: Sender<NetEvent>,
    ctx: egui::Context,
    shutdown: Arc<Notify>,
) -> anyhow::Result<String> {
    let emit = |e: NetEvent| {
        let _ = events.send(e);
        ctx.request_repaint();
    };
    let addr = tokio::net::lookup_host(&p.address)
        .await
        .map_err(|e| anyhow::anyhow!("Could not resolve {}: {e}", p.address))?
        .next()
        .ok_or_else(|| anyhow::anyhow!("Could not resolve {}", p.address))?;
    let stream = timeout(Duration::from_secs(10), TcpStream::connect(addr))
        .await
        .map_err(|_| anyhow::anyhow!("Connection timed out"))?
        .map_err(|e| anyhow::anyhow!("Could not connect: {e}"))?;
    stream.set_nodelay(true)?;
    let (r, w) = stream.into_split();
    let sec = timeout(Duration::from_secs(10), transport::connect(r, w, &p.identity.private))
        .await
        .map_err(|_| anyhow::anyhow!("Handshake timed out"))??;
    let fp = fingerprint(&sec.remote_static);
    match &p.pinned {
        Some(pinned) if *pinned != fp => {
            emit(NetEvent::FingerprintMismatch { address: p.address.clone(), expected: pinned.clone(), got: fp });
            return Ok("Server identity changed".into());
        }
        None => emit(NetEvent::NewFingerprint { address: p.address.clone(), fingerprint: fp }),
        _ => {}
    }
    let mut reader = sec.reader;
    let mut writer = sec.writer;
    writer.send(&p.hello).await?;
    let first: S2C = timeout(Duration::from_secs(15), reader.recv()).await.map_err(|_| anyhow::anyhow!("Server did not respond"))??;
    let token = match &first {
        S2C::Refused(r) => return Ok(r.clone()),
        S2C::Welcome { bulk_token, .. } => *bulk_token,
        _ => return Ok("Unexpected server response".into()),
    };
    emit(NetEvent::Connected { server_addr: addr });
    emit(NetEvent::Msg(first));

    // Bulk connection (screen share & files).
    {
        let events = events.clone();
        let ctx = ctx.clone();
        let identity = p.identity.clone();
        tokio::spawn(async move {
            let _ = bulk(addr, identity, token, bulk_rx, events, ctx).await;
        });
    }

    let writer_task = tokio::spawn(async move {
        let mut ping = tokio::time::interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                m = rx.recv() => match m {
                    Some(m) => { if writer.send(&m).await.is_err() { break; } }
                    None => break,
                },
                _ = ping.tick() => {
                    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
                    if writer.send(&C2S::Ping(t)).await.is_err() { break; }
                }
            }
        }
    });

    let reason = loop {
        tokio::select! {
            m = reader.recv::<S2C>() => match m {
                Ok(S2C::Kicked(r)) => {
                    break match r {
                        LeaveReason::Kicked { by, reason } => format!("Kicked from server by {by}: {reason}"),
                        LeaveReason::Banned { by, reason } => format!("Banned from server by {by}: {reason}"),
                        LeaveReason::Disconnected(m) if !m.is_empty() => m,
                        _ => "Disconnected by server".into(),
                    };
                }
                Ok(m) => emit(NetEvent::Msg(m)),
                Err(e) => break format!("Connection lost ({e})"),
            },
            _ = shutdown.notified() => break "Disconnected".into(),
        }
    };
    writer_task.abort();
    Ok(reason)
}

async fn bulk(
    addr: SocketAddr,
    identity: KeyPair,
    token: [u8; 32],
    mut out: mpsc::Receiver<BulkC2S>,
    events: Sender<NetEvent>,
    ctx: egui::Context,
) -> anyhow::Result<()> {
    let stream = TcpStream::connect(addr).await?;
    stream.set_nodelay(true)?;
    let (r, w) = stream.into_split();
    let sec = transport::connect(r, w, &identity.private).await?;
    let mut reader = sec.reader;
    let mut writer = sec.writer;
    writer.send(&C2S::AttachBulk(token)).await?;
    let first: BulkS2C = reader.recv().await?;
    if !matches!(first, BulkS2C::Attached) {
        anyhow::bail!("bulk attach refused");
    }
    let _ = events.send(NetEvent::BulkReady);
    let w = tokio::spawn(async move {
        while let Some(m) = out.recv().await {
            if writer.send(&m).await.is_err() {
                break;
            }
        }
    });
    while let Ok(m) = reader.recv::<BulkS2C>().await {
        if events.send(NetEvent::Bulk(m)).is_err() {
            break;
        }
        ctx.request_repaint();
    }
    w.abort();
    Ok(())
}
