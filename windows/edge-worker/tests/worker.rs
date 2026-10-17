use coucou_edge_worker::{
    advertised_capabilities, backoff_delay, bound_evidence, build_ws_request, credential_service,
    endpoint_url, validate_server_url, CuaAction, CuaBridge, IdempotencyLedger, StoredIdentity,
    TaskLifecycle,
};
use serde_json::json;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_path(name: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("coucou-edge-{name}-{nonce}.json"))
}

#[test]
fn url_security_rejects_plaintext_and_non_web_schemes() {
    assert!(validate_server_url("https://control.example.com", false).is_ok());
    assert!(validate_server_url("wss://control.example.com", false).is_ok());
    assert!(validate_server_url("http://control.example.com", true).is_err());
    assert!(validate_server_url("http://127.0.0.1:8787", false).is_err());
    assert!(validate_server_url("http://localhost:8787", true).is_ok());
    assert!(validate_server_url("ws://[::1]:8787", true).is_ok());
    assert!(validate_server_url("file:///tmp/socket", true).is_err());
}

#[test]
fn endpoint_urls_preserve_the_configured_reverse_proxy_base_path() {
    let server = validate_server_url("https://control.example.com/control", false).unwrap();
    assert_eq!(
        endpoint_url(&server, "/v1/pair/start", false)
            .unwrap()
            .as_str(),
        "https://control.example.com/control/v1/pair/start"
    );
    assert_eq!(
        endpoint_url(&server, "/v1/worker/ws", true)
            .unwrap()
            .as_str(),
        "wss://control.example.com/control/v1/worker/ws"
    );
}

#[test]
fn websocket_handshake_has_signed_auth_headers() {
    let request = build_ws_request(
        "wss://control.example.com/v1/worker/ws",
        "device-7",
        "credential-secret",
        &[7_u8; 32],
        1_700_000_000,
        "nonce-123",
    )
    .unwrap();

    assert_eq!(
        request.headers()["authorization"],
        "Bearer credential-secret"
    );
    assert_eq!(request.headers()["x-coucou-timestamp"], "1700000000");
    assert_eq!(request.headers()["x-coucou-nonce"], "nonce-123");
    let signature = request.headers()["x-coucou-signature"].to_str().unwrap();
    assert!(!signature.is_empty());
    assert!(
        !signature.contains('='),
        "signature must be base64url without padding"
    );
}

#[test]
fn credentials_are_validated_and_namespaced_by_server_origin() {
    let a = credential_service("https://control.example.com/path").unwrap();
    let b = credential_service("https://control.example.com:8443/path").unwrap();
    assert_ne!(a, b);
    assert_eq!(
        a,
        credential_service("https://control.example.com/other").unwrap()
    );

    let invalid = StoredIdentity {
        server_origin: "https://control.example.com".into(),
        device_id: "device-7".into(),
        credential: "secret".into(),
        signing_key: "dG9vLXNob3J0".into(),
    };
    assert!(invalid.validate("https://control.example.com").is_err());
    assert!(invalid.validate("https://other.example.com").is_err());
}

#[test]
fn durable_ledger_deduplicates_after_restart() {
    let path = temp_path("ledger");
    let mut ledger = IdempotencyLedger::open(&path).unwrap();
    assert!(ledger.claim("task-42").unwrap());
    assert!(!ledger.claim("task-42").unwrap());
    drop(ledger);

    let mut reopened = IdempotencyLedger::open(&path).unwrap();
    assert!(!reopened.claim("task-42").unwrap());
    fs::remove_file(path).unwrap();
}

#[test]
fn bridge_allowlist_is_read_only_and_runtime_aware() {
    assert_eq!(CuaAction::parse("capture").unwrap(), CuaAction::Capture);
    assert_eq!(
        CuaAction::parse("accessibility_tree").unwrap(),
        CuaAction::AccessibilityTree
    );
    for forbidden in [
        "click",
        "type",
        "shell",
        "permission",
        "payment",
        "password",
        "2fa",
    ] {
        assert!(
            CuaAction::parse(forbidden).is_err(),
            "{forbidden} must be denied"
        );
    }
    assert_eq!(
        advertised_capabilities(false),
        vec!["health", "system_info"]
    );
    assert_eq!(
        advertised_capabilities(true),
        vec!["health", "system_info", "computer_use"]
    );
}

#[test]
fn evidence_is_bounded_without_embedding_artifact_bytes() {
    let bounded = bound_evidence(
        json!({"text": "x".repeat(20_000), "artifactPath": "C:/tmp/capture.png", "sha256": "a".repeat(64)}),
        1024,
    );
    let encoded = serde_json::to_vec(&bounded).unwrap();
    assert!(encoded.len() <= 1024, "{} bytes", encoded.len());
    assert_eq!(bounded["truncated"], true);
    assert!(bounded.get("base64").is_none());
}

#[test]
fn backoff_is_exponential_and_bounded() {
    assert_eq!(backoff_delay(0), std::time::Duration::from_secs(1));
    assert_eq!(backoff_delay(1), std::time::Duration::from_secs(2));
    assert_eq!(backoff_delay(5), std::time::Duration::from_secs(32));
    assert_eq!(backoff_delay(99), std::time::Duration::from_secs(60));
}

#[test]
fn task_lifecycle_acknowledges_runs_and_cancels() {
    let mut lifecycle = TaskLifecycle::default();
    assert_eq!(lifecycle.accept("task-9").unwrap()["status"], "accepted");
    assert_eq!(lifecycle.running("task-9").unwrap()["status"], "running");
    assert_eq!(lifecycle.cancel("task-9").unwrap()["status"], "cancelled");
    assert!(lifecycle.succeeded("task-9", json!({"ok": true})).is_err());
}

#[test]
fn installed_cua_driver_performs_a_real_read_only_capture() {
    let artifact_dir = temp_path("capture-artifacts");
    let bridge = CuaBridge::discover(&artifact_dir);
    if !bridge.available() {
        return;
    }

    let evidence = bridge
        .execute(CuaAction::Capture, &json!({"maxImageDimension": 640}))
        .unwrap();
    let artifact = evidence["artifactPath"].as_str().unwrap();
    assert!(std::path::Path::new(artifact).is_file());
    assert_eq!(evidence["sha256"].as_str().unwrap().len(), 64);
    assert!(evidence.get("base64").is_none());
    fs::remove_dir_all(artifact_dir).unwrap();
}
