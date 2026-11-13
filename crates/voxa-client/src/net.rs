//! Control and bulk TCP connections to the server.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::sync::{mpsc, Notify};
use tokio::time::timeout;
use voxa_proto::identity::{fingerprint, KeyPair};
use voxa_proto::transport;
use voxa_proto::traffic::{self, Cat, TRAFFIC};
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
    /// The bulk (files / screen share) connection dropped; it reconnects by itself.
    BulkLost(String),
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
    /// Bounded queue for bulk data (screen share frames, file chunks).
    pub bulk_tx: mpsc::Sender<BulkC2S>,
    /// Unbounded, prioritised queue for small bulk control messages (start / cancel
    /// transfers) so they can never be dropped behind a full data queue.
    pub bulk_ctl: mpsc::UnboundedSender<BulkC2S>,
    shutdown: Arc<Notify>,
}

impl Connection {
    pub fn send(&self, m: C2S) {
        let _ = self.tx.send(m);
    }

    pub fn bulk_control(&self, m: BulkC2S) {
        let _ = self.bulk_ctl.send(m);
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
    let (bulk_ctl, bulk_ctl_rx) = mpsc::unbounded_channel();
    let shutdown = Arc::new(Notify::new());
    let conn = Connection { tx, bulk_tx, bulk_ctl, shutdown: shutdown.clone() };
    rt.spawn(async move {
        let ev = events.clone();
        let c2 = ctx.clone();
        let reason = match run(p, rx, bulk_rx, bulk_ctl_rx, events, ctx, shutdown).await {
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
    bulk_ctl_rx: mpsc::UnboundedReceiver<BulkC2S>,
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
    TRAFFIC.sent(Cat::Chat, writer.last_size);
    let first: S2C = timeout(Duration::from_secs(15), reader.recv()).await.map_err(|_| anyhow::anyhow!("Server did not respond"))??;
    TRAFFIC.received(Cat::Chat, reader.last_size);
    let token = match &first {
        S2C::Refused(r) => return Ok(r.clone()),
        S2C::Welcome { bulk_token, .. } => *bulk_token,
        _ => return Ok("Unexpected server response".into()),
    };
    emit(NetEvent::Connected { server_addr: addr });
    emit(NetEvent::Msg(first));

    // Bulk connection (screen share & files), reconnected automatically while the
    // control connection is alive.
    let alive = Arc::new(AtomicBool::new(true));
    {
        let events = events.clone();
        let ctx = ctx.clone();
        let identity = p.identity.clone();
        let alive = alive.clone();
        let server_key = sec.remote_static.clone();
        tokio::spawn(async move {
            bulk_loop(addr, identity, server_key, token, bulk_rx, bulk_ctl_rx, events, ctx, alive).await;
        });
    }

    let writer_task = tokio::spawn(async move {
        let mut ping = tokio::time::interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                m = rx.recv() => match m {
                    Some(m) => {
                        if writer.send(&m).await.is_err() { break; }
                        TRAFFIC.sent(Cat::Chat, writer.last_size);
                    }
                    None => break,
                },
                _ = ping.tick() => {
                    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
                    if writer.send(&C2S::Ping(t)).await.is_err() { break; }
                    TRAFFIC.sent(Cat::Chat, writer.last_size);
                }
            }
        }
    });

    let reason = loop {
        tokio::select! {
            m = reader.recv::<S2C>() => match m.inspect(|_| TRAFFIC.received(Cat::Chat, reader.last_size)) {
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
    alive.store(false, Ordering::Relaxed);
    writer_task.abort();
    Ok(reason)
}

/// Keeps the bulk connection up for the lifetime of the session. The queues are owned
/// here (not by a per-connection task) so they survive reconnects.
#[allow(clippy::too_many_arguments)]
async fn bulk_loop(
    addr: SocketAddr,
    identity: KeyPair,
    server_key: Vec<u8>,
    token: [u8; 32],
    mut out: mpsc::Receiver<BulkC2S>,
    mut ctl: mpsc::UnboundedReceiver<BulkC2S>,
    events: Sender<NetEvent>,
    ctx: egui::Context,
    alive: Arc<AtomicBool>,
) {
    let mut first = true;
    while alive.load(Ordering::Relaxed) {
        if !first {
            // Anything still queued belongs to transfers that just failed.
            while out.try_recv().is_ok() {}
            while ctl.try_recv().is_ok() {}
            tokio::time::sleep(Duration::from_secs(2)).await;
            if !alive.load(Ordering::Relaxed) {
                break;
            }
        }
        first = false;
        let res = bulk_session(addr, &identity, &server_key, token, &mut out, &mut ctl, &events, &ctx, &alive).await;
        if !alive.load(Ordering::Relaxed) {
            break;
        }
        let _ = events.send(NetEvent::BulkLost(match res {
            Ok(()) => "connection closed".into(),
            Err(e) => e.to_string(),
        }));
        ctx.request_repaint();
    }
}

#[allow(clippy::too_many_arguments)]
async fn bulk_session(
    addr: SocketAddr,
    identity: &KeyPair,
    server_key: &[u8],
    token: [u8; 32],
    out: &mut mpsc::Receiver<BulkC2S>,
    ctl: &mut mpsc::UnboundedReceiver<BulkC2S>,
    events: &Sender<NetEvent>,
    ctx: &egui::Context,
    alive: &AtomicBool,
) -> anyhow::Result<()> {
    let stream = timeout(Duration::from_secs(10), TcpStream::connect(addr)).await??;
    stream.set_nodelay(true)?;
    let (r, w) = stream.into_split();
    let sec = timeout(Duration::from_secs(10), transport::connect(r, w, &identity.private)).await??;
    // Must be the same server as the (pinned) control connection; otherwise someone in
    // between could take the token and read or change files and screen shares.
    if sec.remote_static != server_key {
        anyhow::bail!("server identity changed on the second connection");
    }
    let mut reader = sec.reader;
    let mut writer = sec.writer;
    writer.send(&C2S::AttachBulk(token)).await?;
    TRAFFIC.sent(Cat::Chat, writer.last_size);
    let first: BulkS2C = timeout(Duration::from_secs(10), reader.recv()).await??;
    TRAFFIC.received(Cat::Chat, reader.last_size);
    if !matches!(first, BulkS2C::Attached) {
        anyhow::bail!("bulk attach refused");
    }
    let _ = events.send(NetEvent::BulkReady);
    ctx.request_repaint();
    let mut check = tokio::time::interval(Duration::from_secs(2));
    loop {
        // `recv` on the secure reader is cancel-safe, so racing it here is fine.
        tokio::select! {
            biased;
            m = ctl.recv() => match m {
                Some(m) => {
                    writer.send(&m).await?;
                    TRAFFIC.sent(traffic::bulk_c2s(&m), writer.last_size);
                }
                None => return Ok(()),
            },
            m = out.recv() => match m {
                Some(m) => {
                    writer.send(&m).await?;
                    TRAFFIC.sent(traffic::bulk_c2s(&m), writer.last_size);
                }
                None => return Ok(()),
            },
            m = reader.recv::<BulkS2C>() => {
                let m = m?;
                TRAFFIC.received(traffic::bulk_s2c(&m), reader.last_size);
                if events.send(NetEvent::Bulk(m)).is_err() {
                    return Ok(());
                }
                ctx.request_repaint();
            }
            _ = check.tick() => {
                if !alive.load(Ordering::Relaxed) {
                    return Ok(());
                }
            }
        }
    }
}
