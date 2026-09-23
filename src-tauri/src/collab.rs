// Real-time collaboration transport (Stage 2: connect/auth/reconnect; Stage 3 added outbound
// data so the transport now carries Yjs sync/update frames both ways).
//
// This module owns a single persistent WebSocket connection from the Rust core to the
// collaboration server, matching the pattern `ai.rs` already established for long-lived
// streaming network I/O: a detached OS thread running its own tokio runtime, pushing results
// back to the frontend rather than blocking the invoking command (see `ai::ai_request`).
//
// The one deliberate difference from `ai.rs` is the delivery mechanism: instead of a global
// `app.emit()` event, updates are pushed over a `tauri::ipc::Channel`, which is Tauri's
// recommended mechanism for high-frequency streams (an ordered, private pipe to one listener,
// with a raw-bytes fast path). That matters now that this transport carries per-keystroke Yjs
// update frames (Stage 3); it cost nothing extra for Stage 2's auth/echo traffic either.
//
// Stage 2 scope: connect, authenticate with a shared workspace password, exchange text/binary
// frames with the server, track connection status, and reconnect with backoff on unexpected
// drops.
//
// Stage 3 adds: `send_data`/`CollabHandle::outbound_tx`, an unbounded queue drained by a new
// branch in `run_once`'s select loop, so the frontend (src/lib/collab/connection.ts) can push
// outbound Yjs frames without the transport itself changing shape. `CollabEvent::Data` (inbound)
// was already a generic byte-carrying frame from Stage 2, so it needed no change at all - it now
// simply carries real Yjs sync/update messages instead of Stage 2's echo bytes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

use crate::state::AppState;

const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Caps the exponent in `MIN_BACKOFF * 2^n` so the shift never overflows; 2^5 * 1s = 32s, already
/// above `MAX_BACKOFF`, so the final `.min(MAX_BACKOFF)` is what actually bites.
const MAX_BACKOFF_EXPONENT: u32 = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Events pushed from the Rust WebSocket client to the frontend over a `Channel`. Serialized as
/// tagged JSON today; `Data`'s bytes now carry real Yjs sync/update frames (Stage 3) and are
/// JSON-array encoded like any other field. If per-keystroke traffic volume ever justifies the
/// Channel's raw-bytes fast path instead, that changes only how `Data` is produced and sent -
/// not this transport, the reconnect logic, or the frontend's status handling.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CollabEvent {
    /// Connection lifecycle. `status` is one of: "connecting" | "connected" | "reconnecting" |
    /// "disconnected" | "error".
    Status {
        status: String,
        detail: Option<String>,
        attempt: Option<u32>,
    },
    /// A text frame from the server (Stage 2: the echo payloads used to prove the transport).
    Message { text: String },
    /// A binary frame from the server, carried opaquely - a Yjs sync-step or update message
    /// (Stage 3+; see src/lib/collab/syncProtocol.ts, which is the only place that decodes it).
    Data { bytes: Vec<u8> },
}

/// Snapshot of the last known status, held in `AppState` so a freshly (re)opened Settings panel
/// can show the current state immediately via `get_collab_status`, without waiting for the next
/// event on a new `Channel`.
#[derive(Clone, Serialize)]
pub struct CollabStatusSnapshot {
    pub status: String,
    pub detail: Option<String>,
}

impl Default for CollabStatusSnapshot {
    fn default() -> Self {
        Self {
            status: "disconnected".to_string(),
            detail: None,
        }
    }
}

/// Live handle to the current connection attempt. Held in `AppState` so `disconnect_collab` and a
/// fresh `connect_collab` call can stop a previous run, and so a background task can tell (via
/// `generation`) whether it has since been superseded or explicitly stopped.
pub struct CollabHandle {
    generation: u64,
    stop_tx: watch::Sender<bool>,
    /// Queues outbound binary frames (Yjs sync/update messages, Stage 3+) to the write half of
    /// the WebSocket owned by this connection's background task. Unbounded because these frames
    /// are small, latency-sensitive edit events - we'd rather buffer a burst in memory than block
    /// or drop a keystroke; a real backlog only builds up while disconnected, and Yjs updates are
    /// idempotent/mergeable so nothing is lost by the resync that follows reconnecting anyway.
    outbound_tx: mpsc::UnboundedSender<Vec<u8>>,
}

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthMessage<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    workspace: &'a str,
    password: &'a str,
}

/// The small set of server->client control messages Stage 2's transport understands by shape;
/// anything else is forwarded to the frontend unparsed as a `CollabEvent::Message`.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ServerMessage {
    Connected,
    #[serde(other)]
    Other,
}

/// Start (or restart) the collaboration connection. Any previous connection/reconnect loop owned
/// by this vault session is stopped first. Runs on a detached thread; this function itself
/// returns immediately once the thread is spawned, matching `ai::ai_request`.
pub fn connect(
    app: AppHandle,
    state: &State<'_, AppState>,
    url: String,
    workspace: String,
    password: String,
    on_event: Channel<CollabEvent>,
) -> Result<(), String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("Server URL is required".to_string());
    }
    if !url.starts_with("ws://") && !url.starts_with("wss://") {
        return Err("Server URL must start with ws:// or wss://".to_string());
    }

    let generation = NEXT_GENERATION.fetch_add(1, Ordering::SeqCst);
    let (stop_tx, stop_rx) = watch::channel(false);
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    {
        let mut guard = state
            .collab
            .lock()
            .map_err(|_| "Collaboration state is unavailable".to_string())?;
        if let Some(previous) = guard.take() {
            let _ = previous.stop_tx.send(true);
        }
        *guard = Some(CollabHandle {
            generation,
            stop_tx,
            outbound_tx,
        });
    }
    if let Ok(mut status) = state.collab_status.lock() {
        *status = CollabStatusSnapshot {
            status: "connecting".to_string(),
            detail: None,
        };
    }

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = on_event.send(CollabEvent::Status {
                    status: "error".to_string(),
                    detail: Some(format!("Failed to start collaboration runtime: {e}")),
                    attempt: None,
                });
                return;
            }
        };
        rt.block_on(run_with_reconnect(
            app, generation, url, workspace, password, on_event, stop_rx, outbound_rx,
        ));
    });

    Ok(())
}

/// Stop the current connection (or reconnect loop), if any. Idempotent.
pub fn disconnect(state: &State<'_, AppState>) -> Result<(), String> {
    let mut guard = state
        .collab
        .lock()
        .map_err(|_| "Collaboration state is unavailable".to_string())?;
    if let Some(handle) = guard.take() {
        let _ = handle.stop_tx.send(true);
    }
    drop(guard);
    if let Ok(mut status) = state.collab_status.lock() {
        *status = CollabStatusSnapshot::default();
    }
    Ok(())
}

pub fn status(state: &State<'_, AppState>) -> Result<CollabStatusSnapshot, String> {
    state
        .collab_status
        .lock()
        .map(|snapshot| snapshot.clone())
        .map_err(|_| "Collaboration state is unavailable".to_string())
}

/// Queue a binary frame (a Yjs sync/update message, see src/lib/collab/syncProtocol.ts) for
/// sending over the active collaboration WebSocket. Fails if there is no active connection; does
/// not wait for the frame to actually reach the wire (see `outbound_tx` on `CollabHandle`).
pub fn send_data(state: &State<'_, AppState>, bytes: Vec<u8>) -> Result<(), String> {
    let guard = state
        .collab
        .lock()
        .map_err(|_| "Collaboration state is unavailable".to_string())?;
    match guard.as_ref() {
        Some(handle) => handle
            .outbound_tx
            .send(bytes)
            .map_err(|_| "Collaboration connection is not active".to_string()),
        None => Err("Not connected to a collaboration server".to_string()),
    }
}

/// Uploads one file/image (from a live note's paste/drop handler in the frontend) to
/// collab-server's `POST /upload/<workspace>` HTTP endpoint - see that project's README - and
/// returns the URL the frontend should embed directly as the note's `<img src>`/`<a href>`
/// (password already included as a query parameter, since that's what a plain `<img>`/`<a>`
/// element needs - no custom header to attach). Stateless with respect to `AppState`:
/// `server_url`/`workspace`/`password` come straight from the frontend's already-loaded vault
/// config, the same way `connect`'s arguments do, rather than being looked up here.
///
/// This is a blocking HTTP call (matching `image_proxy.rs`'s use of `reqwest::blocking` for the
/// same reason: it's invoked from a `#[tauri::command]`, which already runs off the main/UI
/// thread on its own blocking-safe worker, so there's no async runtime to plug an async client
/// into here without adding one just for this).
pub fn upload_live_file(
    server_url: String,
    workspace: String,
    password: String,
    name: String,
    mime_type: String,
    data: Vec<u8>,
) -> Result<String, String> {
    // Mirrors collab-server's own UPLOAD_MAX_BYTES default (95 MB) so an oversized file fails
    // fast locally instead of uploading tens of megabytes only to be rejected at the end - the
    // server enforces its own limit independently and remains the source of truth if the two
    // ever drift (e.g. an operator raising UPLOAD_MAX_BYTES there).
    const MAX_UPLOAD_BYTES: usize = 95 * 1024 * 1024;
    if data.len() > MAX_UPLOAD_BYTES {
        return Err(format!(
            "\"{name}\" is {:.1} MB, over the {} MB live-notebook upload limit",
            data.len() as f64 / (1024.0 * 1024.0),
            MAX_UPLOAD_BYTES / (1024 * 1024),
        ));
    }

    let http_base = to_http_base(&server_url)?;
    let mut upload_url =
        reqwest::Url::parse(&http_base).map_err(|e| format!("Invalid server URL: {e}"))?;
    upload_url
        .path_segments_mut()
        .map_err(|_| "Invalid server URL".to_string())?
        .push("upload")
        .push(&workspace);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let content_type = if mime_type.trim().is_empty() {
        "application/octet-stream".to_string()
    } else {
        mime_type
    };
    let response = client
        .post(upload_url)
        .header("X-Collab-Password", password.clone())
        .header("X-File-Name", encode_uri_component(&name))
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .body(data)
        .send()
        .map_err(|e| format!("Upload failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let detail = response.text().unwrap_or_default();
        let detail = if detail.is_empty() { "no further details".to_string() } else { detail };
        return Err(format!("Upload failed ({status}): {detail}"));
    }

    let body: UploadApiResponse = response
        .json()
        .map_err(|e| format!("Upload succeeded but the server's response couldn't be read: {e}"))?;

    let mut display_url = reqwest::Url::parse(&format!("{http_base}{}", body.url))
        .map_err(|e| format!("Invalid upload URL returned by server: {e}"))?;
    display_url.query_pairs_mut().append_pair("password", &password);
    Ok(display_url.to_string())
}

#[derive(Deserialize)]
struct UploadApiResponse {
    url: String,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    size: u64,
    #[allow(dead_code)]
    #[serde(rename = "githubBackedUp")]
    github_backed_up: bool,
}

/// Rewrites a `ws://`/`wss://` collaboration server URL (as stored in vault config for the
/// WebSocket connection) to the matching `http://`/`https://` base for the plain HTTP upload
/// endpoints - same host/port, different scheme. Passes an already-`http(s)://` URL through
/// unchanged (in case a server is ever configured with one directly), and strips any trailing
/// slash so `path_segments_mut().push(...)` above doesn't produce a double slash.
fn to_http_base(server_url: &str) -> Result<String, String> {
    let trimmed = server_url.trim().trim_end_matches('/');
    if let Some(rest) = trimmed.strip_prefix("wss://") {
        return Ok(format!("https://{rest}"));
    }
    if let Some(rest) = trimmed.strip_prefix("ws://") {
        return Ok(format!("http://{rest}"));
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Ok(trimmed.to_string());
    }
    Err(format!("\"{server_url}\" doesn't look like a collaboration server URL"))
}

/// Percent-encodes exactly the characters JavaScript's `encodeURIComponent` would leave alone vs.
/// escape, byte-by-byte over the UTF-8 encoding - so collab-server's `decodeURIComponent` call on
/// the `X-File-Name` header reconstructs the original filename exactly, including any non-ASCII
/// characters. Used instead of a raw header value because HTTP header values can't safely carry
/// arbitrary Unicode.
fn encode_uri_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
        }
    }
    out
}

/// True while `generation` is still the connection recorded in `AppState` - false once the user
/// disconnected or started a newer connection, telling an in-flight retry/backoff to give up
/// quietly instead of reporting stale status over a `Channel` nobody is listening to anymore.
fn is_current(app: &AppHandle, generation: u64) -> bool {
    let state = app.state::<AppState>();
    let result = match state.collab.lock() {
        Ok(guard) => guard.as_ref().is_some_and(|h| h.generation == generation),
        Err(_) => false,
    };
    result
}

fn set_status(app: &AppHandle, status: &str, detail: Option<String>) {
    if let Ok(mut guard) = app.state::<AppState>().collab_status.lock() {
        *guard = CollabStatusSnapshot {
            status: status.to_string(),
            detail,
        };
    }
}

fn emit_status(
    app: &AppHandle,
    on_event: &Channel<CollabEvent>,
    status: &str,
    detail: Option<String>,
    attempt: Option<u32>,
) {
    set_status(app, status, detail.clone());
    let _ = on_event.send(CollabEvent::Status {
        status: status.to_string(),
        detail,
        attempt,
    });
}

async fn run_with_reconnect(
    app: AppHandle,
    generation: u64,
    url: String,
    workspace: String,
    password: String,
    on_event: Channel<CollabEvent>,
    mut stop_rx: watch::Receiver<bool>,
    mut outbound_rx: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let mut attempt: u32 = 0;
    loop {
        if *stop_rx.borrow() || !is_current(&app, generation) {
            return;
        }
        attempt += 1;
        emit_status(
            &app,
            &on_event,
            if attempt == 1 { "connecting" } else { "reconnecting" },
            None,
            Some(attempt),
        );

        match run_once(
            &app,
            &url,
            &workspace,
            &password,
            &on_event,
            &mut stop_rx,
            &mut outbound_rx,
        )
        .await
        {
            Ok(()) => {
                // run_once only returns Ok(()) after observing the stop signal.
                emit_status(&app, &on_event, "disconnected", None, None);
                return;
            }
            Err(e) => {
                if *stop_rx.borrow() || !is_current(&app, generation) {
                    return;
                }
                emit_status(&app, &on_event, "error", Some(e), Some(attempt));
            }
        }

        let exponent = attempt.saturating_sub(1).min(MAX_BACKOFF_EXPONENT);
        let backoff = std::cmp::min(MIN_BACKOFF * (1u32 << exponent), MAX_BACKOFF);
        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = stop_rx.changed() => {}
        }
    }
}

/// Runs one connection attempt to completion. Returns `Ok(())` only when the stop signal fired
/// (an explicit `disconnect_collab`, or a newer `connect_collab` superseding this one); any other
/// way the connection ends - handshake failure, the server closing it, a read/write error - comes
/// back as `Err` so the caller retries with backoff.
async fn run_once(
    app: &AppHandle,
    url: &str,
    workspace: &str,
    password: &str,
    on_event: &Channel<CollabEvent>,
    stop_rx: &mut watch::Receiver<bool>,
    outbound_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
) -> Result<(), String> {
    let (ws_stream, _response) = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url))
        .await
        .map_err(|_| "Connection timed out".to_string())?
        .map_err(|e| format!("Connection failed: {e}"))?;

    let (mut write, mut read) = ws_stream.split();

    let auth = serde_json::to_string(&AuthMessage {
        kind: "auth",
        workspace,
        password,
    })
    .map_err(|e| e.to_string())?;
    write
        .send(Message::from(auth))
        .await
        .map_err(|e| format!("Failed to send authentication: {e}"))?;

    loop {
        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() {
                    return Err("Collaboration state is unavailable".to_string());
                }
                if *stop_rx.borrow() {
                    let _ = write.send(Message::Close(None)).await;
                    return Ok(());
                }
            }
            outbound = outbound_rx.recv() => {
                match outbound {
                    Some(bytes) => {
                        write
                            .send(Message::Binary(bytes.into()))
                            .await
                            .map_err(|e| format!("Failed to send data: {e}"))?;
                    }
                    None => {
                        // The sending half (CollabHandle::outbound_tx) is gone, which only
                        // happens once this connection has been superseded or disconnected -
                        // the stop signal has already fired or is about to. Park this branch so
                        // selecting on a permanently-closed channel doesn't spin the loop; the
                        // stop_rx/read branches keep making progress and will end the loop.
                        std::future::pending::<()>().await;
                    }
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let text = text.to_string();
                        if let Ok(ServerMessage::Connected) = serde_json::from_str::<ServerMessage>(&text) {
                            emit_status(app, on_event, "connected", None, None);
                        } else {
                            let _ = on_event.send(CollabEvent::Message { text });
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        let _ = on_event.send(CollabEvent::Data { bytes: bytes.to_vec() });
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        // tungstenite queues an automatic Pong reply internally, but that queued
                        // frame is only flushed on the next write through the *same* stream half
                        // that read the Ping - and read/write are split here onto independent
                        // tasks/branches, so an otherwise-idle connection's auto-Pong can sit
                        // unflushed indefinitely. Reply explicitly so idle windows don't silently
                        // fail the relay's heartbeat and get disconnected.
                        write
                            .send(Message::Pong(payload))
                            .await
                            .map_err(|e| format!("Failed to respond to ping: {e}"))?;
                    }
                    Some(Ok(Message::Pong(_))) | Some(Ok(Message::Frame(_))) => {}
                    Some(Ok(Message::Close(frame))) => {
                        let reason = frame.map(|f| f.reason.to_string()).filter(|r| !r.is_empty());
                        return Err(reason.unwrap_or_else(|| "The server closed the connection".to_string()));
                    }
                    Some(Err(e)) => return Err(format!("Connection error: {e}")),
                    None => return Err("Connection closed unexpectedly".to_string()),
                }
            }
        }
    }
}
