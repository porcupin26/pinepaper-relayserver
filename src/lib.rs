//! Document-blind relay. Rooms contain live socket handles only; no snapshots,
//! operation history, file cache, database, replay queue, or payload logging.
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{any, get},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, watch, Semaphore};

pub const PROTOCOL: u8 = 1;
pub const MAX_FRAME: usize = 128 * 1024;
const MAX_PEERS: usize = 32;
const MAX_ROOMS: usize = 256;

#[derive(Clone)]
struct Peer {
    tx: mpsc::Sender<Message>,
    cancel: watch::Sender<bool>,
}
#[derive(Default)]
struct Hub {
    closing: bool,
    rooms: HashMap<String, BTreeMap<String, Peer>>,
    attempts: HashMap<IpAddr, (Instant, usize)>,
}
#[derive(Clone)]
pub struct Relay {
    hub: Arc<Mutex<Hub>>,
    slots: Arc<Semaphore>,
}
impl Default for Relay {
    fn default() -> Self {
        Self {
            hub: Default::default(),
            slots: Arc::new(Semaphore::new(256)),
        }
    }
}
impl Relay {
    pub fn shutdown(&self) {
        let mut hub = self.hub.lock().unwrap();
        hub.closing = true;
        for peer in hub.rooms.values().flat_map(|room| room.values()) {
            let _ = peer.cancel.send(true);
        }
    }
    pub fn room_count(&self) -> usize {
        self.hub.lock().unwrap().rooms.len()
    }
    fn admit(&self, ip: IpAddr) -> bool {
        let mut hub = self.hub.lock().unwrap();
        hub.attempts
            .retain(|_, (time, _)| time.elapsed() < Duration::from_secs(60));
        if hub.attempts.len() >= 4096 && !hub.attempts.contains_key(&ip) {
            return false;
        }
        let entry = hub.attempts.entry(ip).or_insert((Instant::now(), 0));
        entry.1 += 1;
        entry.1 <= 60
    }
    fn enter(
        &self,
        wanted: Option<String>,
        id: &str,
        peer: Peer,
    ) -> Result<(String, Vec<String>), &'static str> {
        let mut hub = self.hub.lock().unwrap();
        if hub.closing {
            return Err("server_full");
        }
        let code = if let Some(code) = wanted {
            if code.len() != 5
                || !code
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            {
                return Err("invalid_code");
            }
            if !hub.rooms.contains_key(&code) {
                return Err("room_not_found");
            }
            code
        } else {
            if hub.rooms.len() >= MAX_ROOMS {
                return Err("server_full");
            }
            loop {
                let mut n = uuid::Uuid::new_v4().as_u128();
                let code: String = (0..5)
                    .map(|_| {
                        let c = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"[(n % 36) as usize] as char;
                        n /= 36;
                        c
                    })
                    .collect();
                if !hub.rooms.contains_key(&code) {
                    break code;
                }
            }
        };
        let room = hub.rooms.entry(code.clone()).or_default();
        if room.len() >= MAX_PEERS {
            return Err("room_full");
        }
        let peers = room.keys().cloned().collect();
        room.insert(id.to_owned(), peer);
        Ok((code, peers))
    }
    fn forward(&self, room: &str, from: &str, to: Option<&str>, message: Value) {
        let peers: Vec<Peer> = self
            .hub
            .lock()
            .unwrap()
            .rooms
            .get(room)
            .map(|peers| {
                peers
                    .iter()
                    .filter(|(id, _)| {
                        id.as_str() != from && to.is_none_or(|target| target == id.as_str())
                    })
                    .map(|(_, peer)| peer.clone())
                    .collect()
            })
            .unwrap_or_default();
        let data = Message::Text(message.to_string().into());
        for peer in peers {
            // A slow peer reconnects and obtains a fresh snapshot from a client.
            // Never silently drop an edit while leaving the recipient connected.
            if peer.tx.try_send(data.clone()).is_err() {
                let _ = peer.cancel.send(true);
            }
        }
    }
    fn leave(&self, code: &str, id: &str) {
        {
            let mut hub = self.hub.lock().unwrap();
            if let Some(room) = hub.rooms.get_mut(code) {
                room.remove(id);
                if room.is_empty() {
                    hub.rooms.remove(code);
                }
            }
        }
        self.forward(code, id, None, json!({"type":"peer_left", "peer":id}));
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Hello {
    Create { version: u8 },
    Join { version: u8, code: String },
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Client {
    Relay { to: Option<String>, payload: Value },
}
pub fn router(relay: Relay) -> Router {
    Router::new()
        .route(
            "/healthz",
            get(|| async { Json(json!({"status":"ok","protocol":PROTOCOL,"storage":"none"})) }),
        )
        .route("/v1/exchange", any(upgrade))
        .with_state(relay)
}
async fn upgrade(
    State(relay): State<Relay>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    ws: WebSocketUpgrade,
) -> Response {
    if !relay.admit(address.ip()) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let Ok(permit) = relay.slots.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    ws.max_message_size(MAX_FRAME)
        .max_frame_size(MAX_FRAME)
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_FRAME * 2)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            session(relay, socket).await;
        })
}
async fn error(socket: &mut WebSocket, code: &str) {
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        socket.send(Message::Text(
            json!({"type":"error","code":code}).to_string().into(),
        )),
    )
    .await;
    let _ = socket.close().await;
}
async fn session(relay: Relay, mut socket: WebSocket) {
    let hello = match tokio::time::timeout(Duration::from_secs(8), socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<Hello>(&text).ok(),
        _ => None,
    };
    let wanted = match hello {
        Some(Hello::Create { version: PROTOCOL }) => None,
        Some(Hello::Join {
            version: PROTOCOL,
            code,
        }) => Some(code),
        _ => {
            error(&mut socket, "invalid_hello").await;
            return;
        }
    };
    let id = uuid::Uuid::new_v4().simple().to_string();
    let (tx, mut rx) = mpsc::channel(8);
    let (cancel, mut cancelled) = watch::channel(false);
    let (code, peers) = match relay.enter(wanted, &id, Peer { tx, cancel }) {
        Ok(room) => room,
        Err(e) => {
            error(&mut socket, e).await;
            return;
        }
    };
    if socket
        .send(Message::Text(
            json!({"type":"welcome","version":PROTOCOL,"code":code,"peer":id,"peers":peers})
                .to_string()
                .into(),
        ))
        .await
        .is_err()
    {
        relay.leave(&code, &id);
        return;
    }
    relay.forward(&code, &id, None, json!({"type":"peer_joined","peer":id}));
    let (mut sink, mut stream) = socket.split();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    let mut last_seen = Instant::now();
    let mut budget = 4 * 1024 * 1024;
    let mut budget_at = Instant::now();
    loop {
        tokio::select! {
            _ = cancelled.changed() => break,
            incoming = stream.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        last_seen = Instant::now();
                        if budget_at.elapsed() >= Duration::from_secs(1) {
                            budget = 4 * 1024 * 1024; budget_at = Instant::now();
                        }
                        if text.len() > budget { break; }
                        budget -= text.len();
                        let Ok(Client::Relay { to, payload }) = serde_json::from_str(&text) else { break; };
                        if to.as_ref().is_some_and(|id| id.len() != 32) { break; }
                        relay.forward(&code, &id, to.as_deref(), json!({"type":"relay","from":id,"payload":payload}));
                    }
                    Some(Ok(Message::Pong(_))) => last_seen = Instant::now(),
                    Some(Ok(Message::Ping(data))) => {
                        last_seen = Instant::now();
                        if sink.send(Message::Pong(data)).await.is_err() { break; }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => break,
                }
            }
            outgoing = rx.recv() => {
                let Some(message) = outgoing else { break; };
                if !matches!(tokio::time::timeout(Duration::from_secs(5), sink.send(message)).await, Ok(Ok(()))) { break; }
            }
            _ = heartbeat.tick() => {
                if last_seen.elapsed() > Duration::from_secs(65) { break; }
                if sink.send(Message::Ping(vec![].into())).await.is_err() { break; }
            }
        }
    }
    relay.leave(&code, &id);
    let _ = tokio::time::timeout(Duration::from_secs(1), sink.close()).await;
}
