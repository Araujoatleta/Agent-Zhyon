use base64::{engine::general_purpose, Engine as _};
use coucou_edge_worker::{
    advertised_capabilities, backoff_delay, bound_evidence, build_ws_request, credential_service,
    execute, server_origin, validate_server_url, CuaBridge, Evidence, IdempotencyLedger,
    StoredIdentity, TaskLifecycle,
};
use ed25519_dalek::SigningKey;
use futures_util::{SinkExt, StreamExt};
use keyring::Entry;
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
const PAIR_POLL_INTERVAL: Duration = Duration::from_secs(2);
const PAIR_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairStart {
    pairing_id: String,
    pin: String,
    claim_token: String,
    expires_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairStatus {
    status: String,
    identity: Option<PairIdentity>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairIdentity {
    device_id: String,
    credential: String,
}

#[derive(Debug, Deserialize)]
struct TaskEnvelope {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    task: Option<RemoteTask>,
    #[serde(rename = "taskId")]
    task_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteTask {
    id: String,
    capability: String,
    #[serde(default)]
    args: Value,
    #[serde(default = "default_max_output_bytes")]
    max_output_bytes: usize,
}

fn default_max_output_bytes() -> usize {
    coucou_edge_worker::MAX_EVIDENCE_BYTES
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let configured =
        std::env::var("COUCOU_VPS_URL").map_err(|_| "COUCOU_VPS_URL is required".to_string())?;
    let allow_insecure = std::env::var("COUCOU_ALLOW_INSECURE_LOCALHOST")
        .map(|value| value == "1")
        .unwrap_or(false);
    let server = validate_server_url(&configured, allow_insecure)?;
    let origin = server_origin(server.as_str())?;
    let storage_root = storage_root(&origin)?;
    let bridge = CuaBridge::discover(storage_root.join("artifacts"));
    let capabilities = advertised_capabilities(bridge.available());
    let identity = load_or_pair(&server, &origin, &capabilities).await?;
    let signing_key = identity.validate(&origin)?;
    let ledger_path = storage_root.join("task-ledger.jsonl");

    let mut failed_attempts = 0_u32;
    loop {
        let websocket_url = websocket_url(&server, &identity.device_id)?;
        let timestamp = unix_timestamp()?;
        let mut nonce = [0_u8; 16];
        rand::thread_rng().fill_bytes(&mut nonce);
        let request = build_ws_request(
            websocket_url.as_str(),
            &identity.device_id,
            &identity.credential,
            &signing_key,
            timestamp,
            &general_purpose::URL_SAFE_NO_PAD.encode(nonce),
        )?;

        let delay = match tokio_tungstenite::connect_async(request).await {
            Ok((socket, _)) => {
                failed_attempts = 0;
                let ledger = IdempotencyLedger::open(&ledger_path)?;
                if let Err(error) =
                    run_connection(socket, &identity.device_id, bridge.clone(), ledger).await
                {
                    eprintln!("worker connection ended: {error}");
                }
                backoff_delay(0)
            }
            Err(error) => {
                eprintln!("websocket connection failed: {error}");
                let delay = backoff_delay(failed_attempts);
                failed_attempts = failed_attempts.saturating_add(1);
                delay
            }
        };
        tokio::time::sleep(delay).await;
    }
}

async fn load_or_pair(
    server: &Url,
    origin: &str,
    capabilities: &[&str],
) -> Result<StoredIdentity, String> {
    let service = credential_service(origin)?;
    let entry = Entry::new(&service, "identity")
        .map_err(|error| format!("open Windows Credential Manager entry: {error}"))?;
    match entry.get_password() {
        Ok(serialized) => {
            let identity: StoredIdentity = serde_json::from_str(&serialized)
                .map_err(|error| format!("stored Coucou identity is corrupt: {error}"))?;
            identity.validate(origin)?;
            Ok(identity)
        }
        Err(keyring::Error::NoEntry) => {
            let identity = pair(server, origin, capabilities).await?;
            let serialized = serde_json::to_string(&identity)
                .map_err(|error| format!("serialize identity: {error}"))?;
            entry.set_password(&serialized).map_err(|error| {
                format!("store identity in Windows Credential Manager: {error}")
            })?;
            Ok(identity)
        }
        Err(error) => Err(format!("read Windows Credential Manager: {error}")),
    }
}

async fn pair(server: &Url, origin: &str, capabilities: &[&str]) -> Result<StoredIdentity, String> {
    let mut secret = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    let signing_key = SigningKey::from_bytes(&secret);
    let public_key = general_purpose::STANDARD.encode(signing_key.verifying_key().as_bytes());
    let device_name = std::env::var("COUCOU_DEVICE_NAME").unwrap_or_else(|_| "Windows PC".into());
    let client = reqwest::Client::new();
    let start_url = http_url(server, "/v1/pair/start")?;
    let start: PairStart = client
        .post(start_url)
        .json(&json!({
            "name": device_name,
            "publicKey": public_key,
            "capabilities": capabilities
        }))
        .send()
        .await
        .map_err(|error| format!("start pairing: {error}"))?
        .error_for_status()
        .map_err(|error| format!("pairing start rejected: {error}"))?
        .json()
        .await
        .map_err(|error| format!("decode pairing start response: {error}"))?;

    println!("Coucou pairing PIN: {}", start.pin);
    println!("Approve this device in Coucou before {}.", start.expires_at);
    let status_url = http_url(server, &format!("/v1/pair/status/{}", start.pairing_id))?;
    let deadline = tokio::time::Instant::now() + PAIR_TIMEOUT;
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err("pairing approval timed out".into());
        }
        let response = client
            .get(status_url.clone())
            .bearer_auth(&start.claim_token)
            .send()
            .await
            .map_err(|error| format!("poll pairing approval: {error}"))?;
        if response.status().is_success() {
            let status: PairStatus = response
                .json()
                .await
                .map_err(|error| format!("decode pairing status: {error}"))?;
            match status.status.as_str() {
                "approved" => {
                    let identity = status
                        .identity
                        .ok_or_else(|| "approved pairing omitted identity".to_string())?;
                    if identity.device_id.is_empty() || identity.credential.is_empty() {
                        return Err("approved pairing returned an invalid identity".into());
                    }
                    return Ok(StoredIdentity {
                        server_origin: origin.into(),
                        device_id: identity.device_id,
                        credential: identity.credential,
                        signing_key: general_purpose::STANDARD.encode(secret),
                    });
                }
                "pending" => {}
                "denied" | "rejected" | "expired" => {
                    return Err(format!("pairing {}", status.status));
                }
                other => return Err(format!("unknown pairing status: {other}")),
            }
        } else if !matches!(response.status().as_u16(), 404 | 409 | 425) {
            return Err(format!(
                "pairing status rejected: HTTP {}",
                response.status()
            ));
        }
        tokio::time::sleep(PAIR_POLL_INTERVAL).await;
    }
}

async fn run_connection(
    mut socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    device_id: &str,
    bridge: CuaBridge,
    mut ledger: IdempotencyLedger,
) -> Result<(), String> {
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let (result_sender, mut result_receiver) =
        mpsc::unbounded_channel::<(String, Evidence, usize)>();
    let mut active: HashMap<String, JoinHandle<()>> = HashMap::new();
    let mut lifecycle = TaskLifecycle::default();

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                send_json(&mut socket, json!({
                    "type": "heartbeat",
                    "deviceId": device_id,
                    "timestamp": unix_timestamp()?
                })).await?;
            }
            result = result_receiver.recv() => {
                let Some((task_id, evidence, max_output_bytes)) = result else { continue; };
                active.remove(&task_id);
                let succeeded = evidence.ok;
                let evidence = bound_evidence(
                    serde_json::to_value(evidence)
                        .map_err(|error| format!("serialize task evidence: {error}"))?,
                    max_output_bytes,
                );
                let terminal = if succeeded {
                    lifecycle.succeeded(&task_id, evidence)
                } else {
                    lifecycle.failed(&task_id, evidence)
                };
                if let Ok(mut message) = terminal {
                    enforce_task_output_limit(&mut message, max_output_bytes);
                    ledger.complete(&task_id, message.clone())?;
                    send_json(&mut socket, message).await?;
                }
            }
            incoming = socket.next() => {
                let message = match incoming {
                    Some(Ok(message)) => message,
                    Some(Err(error)) => return Err(format!("websocket read failed: {error}")),
                    None => return Err("websocket closed by server".into()),
                };
                if message.is_ping() {
                    socket.send(Message::Pong(message.into_data())).await
                        .map_err(|error| format!("send pong: {error}"))?;
                    continue;
                }
                if !message.is_text() {
                    continue;
                }
                let envelope: TaskEnvelope = serde_json::from_str(
                    message.to_text().map_err(|error| format!("decode websocket text: {error}"))?
                ).map_err(|error| format!("decode worker message: {error}"))?;
                match envelope.kind.as_str() {
                    "task" => {
                        let task = envelope.task.ok_or_else(|| "task message omitted task".to_string())?;
                        if !ledger.claim(&task.id)? {
                            if let Some(terminal) = ledger.terminal(&task.id).cloned() {
                                send_json(&mut socket, terminal).await?;
                            } else {
                                send_json(&mut socket, json!({
                                    "type": "task_status", "taskId": task.id, "status": "accepted", "duplicate": true
                                })).await?;
                            }
                            continue;
                        }
                        send_json(&mut socket, lifecycle.accept(&task.id)?).await?;
                        send_json(&mut socket, lifecycle.running(&task.id)?).await?;
                        let sender = result_sender.clone();
                        let task_id = task.id.clone();
                        let task_id_for_work = task.id.clone();
                        let max_output_bytes = task.max_output_bytes;
                        let task_bridge = bridge.clone();
                        let handle = tokio::task::spawn_blocking(move || {
                            let evidence = execute(&task.capability, &task.args, &task_bridge);
                            let _ = sender.send((task_id_for_work, evidence, max_output_bytes));
                        });
                        active.insert(task_id, handle);
                    }
                    "cancel" | "task_cancel" => {
                        let task_id = envelope.task_id.ok_or_else(|| "cancel omitted taskId".to_string())?;
                        if let Some(handle) = active.remove(&task_id) {
                            handle.abort();
                        }
                        if let Ok(message) = lifecycle.cancel(&task_id) {
                            // The control plane has already made cancellation terminal before
                            // broadcasting task_cancel, so persist locally but do not send an
                            // invalid cancelled transition back over the worker socket.
                            ledger.complete(&task_id, message)?;
                        }
                    }
                    "pong" | "heartbeat_ack" => {}
                    _ => {}
                }
            }
        }
    }
}

fn enforce_task_output_limit(message: &mut Value, maximum: usize) {
    let Some(evidence) = message.get("evidence").cloned() else {
        return;
    };
    message["evidence"] = bound_evidence(evidence, maximum);
    let still_too_large = serde_json::to_vec(&message["evidence"])
        .map(|encoded| encoded.len() > maximum)
        .unwrap_or(true);
    if still_too_large {
        message
            .as_object_mut()
            .map(|object| object.remove("evidence"));
    }
}

async fn send_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    value: Value,
) -> Result<(), String> {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .map_err(|error| format!("websocket send failed: {error}"))
}

fn http_url(server: &Url, path: &str) -> Result<Url, String> {
    let mut url = server.clone();
    let scheme = match server.scheme() {
        "https" | "wss" => "https",
        "http" | "ws" => "http",
        _ => return Err("unsupported server URL scheme".into()),
    };
    url.set_scheme(scheme)
        .map_err(|_| "cannot set HTTP URL scheme")?;
    url.set_path(path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn websocket_url(server: &Url, device_id: &str) -> Result<Url, String> {
    let mut url = server.clone();
    let scheme = match server.scheme() {
        "https" | "wss" => "wss",
        "http" | "ws" => "ws",
        _ => return Err("unsupported server URL scheme".into()),
    };
    url.set_scheme(scheme)
        .map_err(|_| "cannot set websocket URL scheme")?;
    url.set_path("/v1/worker/ws");
    url.set_query(None);
    url.query_pairs_mut().append_pair("device_id", device_id);
    Ok(url)
}

fn storage_root(origin: &str) -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "LOCALAPPDATA is unavailable".to_string())?;
    let namespace = credential_service(origin)?
        .split_whitespace()
        .last()
        .ok_or_else(|| "cannot derive server namespace".to_string())?
        .to_string();
    Ok(local.join("Coucou").join("edge-worker").join(namespace))
}

fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| "system clock is before UNIX epoch".into())
}
