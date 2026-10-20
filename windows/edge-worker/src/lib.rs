use base64::{engine::general_purpose, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::Request;
use url::Url;

pub const MAX_EVIDENCE_BYTES: usize = 64 * 1024;

#[cfg(windows)]
pub struct SingleInstanceGuard(*mut core::ffi::c_void);

#[cfg(windows)]
impl SingleInstanceGuard {
    pub fn acquire(name: &str) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        const ERROR_ALREADY_EXISTS: u32 = 183;
        #[link(name = "kernel32")]
        extern "system" {
            fn CreateMutexW(
                attributes: *mut core::ffi::c_void,
                initial_owner: i32,
                name: *const u16,
            ) -> *mut core::ffi::c_void;
            fn GetLastError() -> u32;
            fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        }
        let wide: Vec<u16> = std::ffi::OsStr::new(name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 0, wide.as_ptr()) };
        if handle.is_null() {
            return Err("failed to create worker instance mutex".into());
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            return Err("another Coucou Edge Worker instance is already running".into());
        }
        Ok(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        #[link(name = "kernel32")]
        extern "system" {
            fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        }
        unsafe { CloseHandle(self.0) };
    }
}

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub server: Url,
    pub device_name: String,
}

impl WorkerConfig {
    pub fn server_from_values(
        server: Option<&str>,
        allow_insecure_localhost: bool,
    ) -> Result<Url, String> {
        let server = server.ok_or_else(|| "COUCOU_VPS_URL is required".to_string())?;
        validate_server_url(server.trim(), allow_insecure_localhost)
    }

    pub fn from_values(
        server: Option<&str>,
        device_name: Option<&str>,
        allow_insecure_localhost: bool,
    ) -> Result<Self, String> {
        let device_name = device_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| "COUCOU_DEVICE_NAME is required".to_string())?;
        if device_name.chars().count() > 80 {
            return Err("COUCOU_DEVICE_NAME must be at most 80 characters".into());
        }
        if device_name.chars().any(char::is_control) {
            return Err("COUCOU_DEVICE_NAME must not contain control characters".into());
        }
        Ok(Self {
            server: Self::server_from_values(server, allow_insecure_localhost)?,
            device_name: device_name.into(),
        })
    }

    pub fn from_environment() -> Result<Self, String> {
        let server = std::env::var("COUCOU_VPS_URL").ok();
        let device_name = std::env::var("COUCOU_DEVICE_NAME").ok();
        let allow_insecure = std::env::var("COUCOU_ALLOW_INSECURE_LOCALHOST")
            .map(|value| value == "1")
            .unwrap_or(false);
        Self::from_values(server.as_deref(), device_name.as_deref(), allow_insecure)
    }
}

pub fn validate_server_url(input: &str, allow_insecure_localhost: bool) -> Result<Url, String> {
    let url = Url::parse(input).map_err(|error| format!("invalid server URL: {error}"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("server URL must not contain credentials".into());
    }
    let secure = matches!(url.scheme(), "https" | "wss");
    let local = match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    let permitted_insecure =
        allow_insecure_localhost && local && matches!(url.scheme(), "http" | "ws");
    if !secure && !permitted_insecure {
        return Err("server URL must use HTTPS/WSS (insecure mode is localhost-only)".into());
    }
    Ok(url)
}

pub fn endpoint_url(server: &Url, endpoint: &str, websocket: bool) -> Result<Url, String> {
    if !endpoint.starts_with('/') {
        return Err("endpoint path must start with /".into());
    }
    let mut url = server.clone();
    let scheme = match (server.scheme(), websocket) {
        ("https" | "wss", false) => "https",
        ("https" | "wss", true) => "wss",
        ("http" | "ws", false) => "http",
        ("http" | "ws", true) => "ws",
        _ => return Err("unsupported server URL scheme".into()),
    };
    url.set_scheme(scheme)
        .map_err(|_| "cannot set endpoint URL scheme")?;
    let base = server.path().trim_end_matches('/');
    url.set_path(&format!("{base}{endpoint}"));
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

pub fn server_origin(input: &str) -> Result<String, String> {
    let url = Url::parse(input).map_err(|error| format!("invalid server URL: {error}"))?;
    Ok(url.origin().ascii_serialization())
}

pub fn credential_service(input: &str) -> Result<String, String> {
    let origin = server_origin(input)?;
    let digest = Sha256::digest(origin.as_bytes());
    Ok(format!(
        "Coucou Edge Worker {}",
        general_purpose::URL_SAFE_NO_PAD.encode(digest)
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredIdentity {
    pub server_origin: String,
    pub device_id: String,
    pub credential: String,
    pub signing_key: String,
}

impl StoredIdentity {
    pub fn validate(&self, expected_server: &str) -> Result<[u8; 32], String> {
        if self.server_origin != server_origin(expected_server)? {
            return Err("stored identity belongs to a different server origin".into());
        }
        if self.device_id.trim().is_empty() || self.credential.trim().is_empty() {
            return Err("stored identity is missing deviceId or credential".into());
        }
        let decoded = general_purpose::STANDARD
            .decode(&self.signing_key)
            .map_err(|_| "stored signing key is not valid base64")?;
        decoded
            .try_into()
            .map_err(|_| "stored signing key must contain exactly 32 bytes".into())
    }

    pub fn public_key_base64(&self) -> Result<String, String> {
        let signing_key = SigningKey::from_bytes(&self.validate(&self.server_origin)?);
        Ok(general_purpose::STANDARD.encode(signing_key.verifying_key().as_bytes()))
    }
}

pub fn build_ws_request(
    url: &str,
    device_id: &str,
    credential: &str,
    signing_key: &[u8; 32],
    timestamp: u64,
    nonce: &str,
) -> Result<Request<()>, String> {
    let mut request = url
        .into_client_request()
        .map_err(|error| format!("invalid websocket request: {error}"))?;
    let payload = format!("{device_id}.{timestamp}.{nonce}");
    let signature = SigningKey::from_bytes(signing_key).sign(payload.as_bytes());
    let headers = request.headers_mut();
    headers.insert(
        "authorization",
        format!("Bearer {credential}")
            .parse()
            .map_err(|_| "credential cannot be represented as a header")?,
    );
    headers.insert(
        "x-coucou-timestamp",
        timestamp
            .to_string()
            .parse()
            .map_err(|_| "invalid timestamp header")?,
    );
    headers.insert(
        "x-coucou-nonce",
        nonce.parse().map_err(|_| "invalid nonce header")?,
    );
    headers.insert(
        "x-coucou-signature",
        general_purpose::URL_SAFE_NO_PAD
            .encode(signature.to_bytes())
            .parse()
            .map_err(|_| "invalid signature header")?,
    );
    Ok(request)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LedgerRecord {
    task_id: String,
    terminal: Option<Value>,
}

pub struct IdempotencyLedger {
    path: PathBuf,
    claimed: HashSet<String>,
    terminal: HashMap<String, Value>,
}

impl IdempotencyLedger {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create ledger directory: {error}"))?;
        }
        let mut ledger = Self {
            path,
            claimed: HashSet::new(),
            terminal: HashMap::new(),
        };
        if ledger.path.exists() {
            let file = fs::File::open(&ledger.path)
                .map_err(|error| format!("open idempotency ledger: {error}"))?;
            for (line_number, line) in BufReader::new(file).lines().enumerate() {
                let line = line.map_err(|error| format!("read ledger: {error}"))?;
                if line.trim().is_empty() {
                    continue;
                }
                let record: LedgerRecord = serde_json::from_str(&line).map_err(|error| {
                    format!("invalid ledger record at line {}: {error}", line_number + 1)
                })?;
                ledger.claimed.insert(record.task_id.clone());
                if let Some(terminal) = record.terminal {
                    ledger.terminal.insert(record.task_id, terminal);
                }
            }
        }
        Ok(ledger)
    }

    pub fn claim(&mut self, task_id: &str) -> Result<bool, String> {
        validate_task_id(task_id)?;
        if self.claimed.contains(task_id) {
            return Ok(false);
        }
        self.append(&LedgerRecord {
            task_id: task_id.into(),
            terminal: None,
        })?;
        self.claimed.insert(task_id.into());
        Ok(true)
    }

    pub fn complete(&mut self, task_id: &str, terminal: Value) -> Result<(), String> {
        if !self.claimed.contains(task_id) {
            return Err("cannot complete an unclaimed task".into());
        }
        self.append(&LedgerRecord {
            task_id: task_id.into(),
            terminal: Some(terminal.clone()),
        })?;
        self.terminal.insert(task_id.into(), terminal);
        Ok(())
    }

    pub fn terminal(&self, task_id: &str) -> Option<&Value> {
        self.terminal.get(task_id)
    }

    fn append(&self, record: &LedgerRecord) -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| format!("open ledger for append: {error}"))?;
        serde_json::to_writer(&mut file, record)
            .map_err(|error| format!("serialize ledger record: {error}"))?;
        file.write_all(b"\n")
            .and_then(|_| file.flush())
            .and_then(|_| file.sync_data())
            .map_err(|error| format!("durably write ledger: {error}"))
    }
}

fn validate_task_id(task_id: &str) -> Result<(), String> {
    if task_id.is_empty() || task_id.len() > 256 || task_id.chars().any(char::is_control) {
        Err("invalid task id".into())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CuaAction {
    Capture,
    AccessibilityTree,
}

impl CuaAction {
    pub fn parse(action: &str) -> Result<Self, String> {
        match action {
            "capture" => Ok(Self::Capture),
            "accessibility_tree" => Ok(Self::AccessibilityTree),
            _ => Err("computer_use action is not in the read-only allowlist".into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CuaBridge {
    executable: PathBuf,
    artifact_dir: PathBuf,
}

impl CuaBridge {
    pub fn new(executable: impl Into<PathBuf>, artifact_dir: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            artifact_dir: artifact_dir.into(),
        }
    }

    pub fn discover(artifact_dir: impl Into<PathBuf>) -> Self {
        Self::new(
            std::env::var_os("CUA_DRIVER_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("cua-driver")),
            artifact_dir,
        )
    }

    pub fn available(&self) -> bool {
        interactive_user_session()
            && Command::new(&self.executable)
                .arg("status")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
    }

    pub fn execute(&self, action: CuaAction, args: &Value) -> Result<Value, String> {
        if !interactive_user_session() {
            return Err("computer_use requires an interactive user session".into());
        }
        fs::create_dir_all(&self.artifact_dir)
            .map_err(|error| format!("create artifact directory: {error}"))?;
        match action {
            CuaAction::Capture => self.capture(args),
            CuaAction::AccessibilityTree => self.accessibility_tree(),
        }
    }

    fn capture(&self, args: &Value) -> Result<Value, String> {
        let maximum = args
            .get("maxImageDimension")
            .and_then(Value::as_u64)
            .unwrap_or(1600)
            .clamp(320, 4096);
        let artifact = self.artifact_dir.join(format!(
            "capture-{}.png",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "system clock is before UNIX epoch")?
                .as_millis()
        ));
        let driver_args = json!({
            "max_image_dimension": maximum,
            "screenshot_out_file": artifact.to_string_lossy()
        });
        let output = Command::new(&self.executable)
            .arg("call")
            .arg("get_desktop_state")
            .arg(driver_args.to_string())
            .arg("--screenshot-out-file")
            .arg(&artifact)
            .output()
            .map_err(|error| format!("start cua-driver: {error}"))?;
        if !output.status.success() {
            return Err(bounded_process_error(&output.stderr));
        }
        let bytes = fs::read(&artifact)
            .map_err(|error| format!("cua-driver did not create capture artifact: {error}"))?;
        let evidence = json!({
            "output": bounded_text(&output.stdout, 8 * 1024),
            "artifactPath": artifact.to_string_lossy(),
            "sha256": format!("{:x}", Sha256::digest(&bytes))
        });
        Ok(bound_evidence(evidence, MAX_EVIDENCE_BYTES))
    }

    fn accessibility_tree(&self) -> Result<Value, String> {
        let output = Command::new(&self.executable)
            .arg("call")
            .arg("get_accessibility_tree")
            .arg("{}")
            .output()
            .map_err(|error| format!("start cua-driver: {error}"))?;
        if !output.status.success() {
            return Err(bounded_process_error(&output.stderr));
        }
        Ok(bound_evidence(
            json!({"output": bounded_text(&output.stdout, MAX_EVIDENCE_BYTES / 2)}),
            MAX_EVIDENCE_BYTES,
        ))
    }
}

pub fn advertised_capabilities(cua_available: bool) -> Vec<&'static str> {
    let mut capabilities = vec!["health", "system_info"];
    if cua_available {
        capabilities.push("computer_use");
    }
    capabilities
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub operation: String,
    pub ok: bool,
    pub data: Value,
}

pub fn execute(capability: &str, args: &Value, bridge: &CuaBridge) -> Evidence {
    let result = match capability {
        "health" => Ok(json!({"status": "ok"})),
        "system_info" => Ok(json!({
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "hostname": std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".into())
        })),
        "computer_use" if bridge.available() => {
            CuaAction::parse(args.get("action").and_then(Value::as_str).unwrap_or(""))
                .and_then(|action| bridge.execute(action, args))
        }
        "computer_use" => Err("computer_use is unavailable at runtime".into()),
        _ => Err("capability is not allowed".into()),
    };
    match result {
        Ok(data) => Evidence {
            operation: capability.into(),
            ok: true,
            data: bound_evidence(data, MAX_EVIDENCE_BYTES),
        },
        Err(error) => Evidence {
            operation: capability.into(),
            ok: false,
            data: json!({"error": error}),
        },
    }
}

fn interactive_user_session() -> bool {
    if !cfg!(windows) {
        return true;
    }
    std::env::var("SESSIONNAME")
        .map(|session| !session.eq_ignore_ascii_case("services") && !session.trim().is_empty())
        .unwrap_or(false)
}

fn bounded_text(bytes: &[u8], maximum: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= maximum {
        text.into_owned()
    } else {
        let mut end = maximum;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…[truncated]", &text[..end])
    }
}

fn bounded_process_error(bytes: &[u8]) -> String {
    format!("cua-driver failed: {}", bounded_text(bytes, 4096))
}

fn remove_embedded_bytes(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.to_ascii_lowercase().as_str(),
                        "base64" | "dataurl" | "imagebytes" | "screenshotbase64"
                    )
                })
                .map(|(key, value)| (key, remove_embedded_bytes(value)))
                .collect(),
        ),
        Value::Array(values) => {
            Value::Array(values.into_iter().map(remove_embedded_bytes).collect())
        }
        other => other,
    }
}

pub fn bound_evidence(value: Value, maximum: usize) -> Value {
    let sanitized = remove_embedded_bytes(value);
    if serde_json::to_vec(&sanitized)
        .map(|encoded| encoded.len() <= maximum)
        .unwrap_or(false)
    {
        return sanitized;
    }
    let original = serde_json::to_string(&sanitized).unwrap_or_else(|_| "unserializable".into());
    let mut low = 0;
    let mut high = original.len();
    let mut best = json!({"truncated": true});
    while low <= high {
        let midpoint = low + (high - low) / 2;
        let mut end = midpoint;
        while end > 0 && !original.is_char_boundary(end) {
            end -= 1;
        }
        let candidate = json!({"truncated": true, "preview": &original[..end]});
        if serde_json::to_vec(&candidate)
            .map(|encoded| encoded.len() <= maximum)
            .unwrap_or(false)
        {
            best = candidate;
            low = midpoint.saturating_add(1);
        } else if midpoint == 0 {
            break;
        } else {
            high = midpoint - 1;
        }
    }
    best
}

pub fn backoff_delay(attempt: u32) -> Duration {
    Duration::from_secs(1_u64.checked_shl(attempt.min(6)).unwrap_or(60).min(60))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleStatus {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Default)]
pub struct TaskLifecycle {
    tasks: HashMap<String, LifecycleStatus>,
}

impl TaskLifecycle {
    pub fn accept(&mut self, task_id: &str) -> Result<Value, String> {
        validate_task_id(task_id)?;
        if self.tasks.contains_key(task_id) {
            return Err("task already accepted".into());
        }
        self.tasks.insert(task_id.into(), LifecycleStatus::Accepted);
        Ok(status_message(task_id, "accepted", None))
    }

    pub fn running(&mut self, task_id: &str) -> Result<Value, String> {
        self.transition(task_id, LifecycleStatus::Accepted, LifecycleStatus::Running)?;
        Ok(status_message(task_id, "running", None))
    }

    pub fn cancel(&mut self, task_id: &str) -> Result<Value, String> {
        match self.tasks.get(task_id) {
            Some(LifecycleStatus::Accepted | LifecycleStatus::Running) => {
                self.tasks
                    .insert(task_id.into(), LifecycleStatus::Cancelled);
                Ok(status_message(task_id, "cancelled", None))
            }
            Some(_) => Err("task is already terminal".into()),
            None => Err("unknown task".into()),
        }
    }

    pub fn succeeded(&mut self, task_id: &str, evidence: Value) -> Result<Value, String> {
        self.transition(
            task_id,
            LifecycleStatus::Running,
            LifecycleStatus::Succeeded,
        )?;
        Ok(status_message(task_id, "succeeded", Some(evidence)))
    }

    pub fn failed(&mut self, task_id: &str, evidence: Value) -> Result<Value, String> {
        self.transition(task_id, LifecycleStatus::Running, LifecycleStatus::Failed)?;
        Ok(status_message(task_id, "failed", Some(evidence)))
    }

    fn transition(
        &mut self,
        task_id: &str,
        expected: LifecycleStatus,
        next: LifecycleStatus,
    ) -> Result<(), String> {
        match self.tasks.get(task_id) {
            Some(current) if *current == expected => {
                self.tasks.insert(task_id.into(), next);
                Ok(())
            }
            Some(_) => Err("invalid task lifecycle transition".into()),
            None => Err("unknown task".into()),
        }
    }
}

fn status_message(task_id: &str, status: &str, evidence: Option<Value>) -> Value {
    let message_type = if matches!(status, "accepted" | "running") {
        "task_status"
    } else {
        "task_result"
    };
    let mut message = json!({
        "type": message_type,
        "taskId": task_id,
        "status": status
    });
    if let Some(evidence) = evidence {
        message["evidence"] = bound_evidence(evidence, MAX_EVIDENCE_BYTES);
    }
    message
}
