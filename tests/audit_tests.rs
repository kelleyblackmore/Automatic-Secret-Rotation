use secret_rotator::audit::{AuditEvent, AuditLogger};
use secret_rotator::config::AuditConfig;
use tempfile::TempDir;

fn disabled_config() -> AuditConfig {
    AuditConfig {
        log_file: None,
        stdout: false,
    }
}

fn file_config(path: &str) -> AuditConfig {
    AuditConfig {
        log_file: Some(path.to_string()),
        stdout: false,
    }
}

fn stdout_config() -> AuditConfig {
    AuditConfig {
        log_file: None,
        stdout: true,
    }
}

// ---------------------------------------------------------------------------
// AuditEvent builder
// ---------------------------------------------------------------------------

#[test]
fn test_audit_event_defaults() {
    let ev = AuditEvent::new("rotate", "app/db", "vault");
    assert_eq!(ev.event, "rotate");
    assert_eq!(ev.path, "app/db");
    assert_eq!(ev.backend, "vault");
    assert_eq!(ev.status, "success");
    assert_eq!(ev.duration_ms, 0);
    assert!(ev.error.is_none());
    assert!(ev.target.is_none());
    assert!(ev.username.is_none());
    assert_eq!(ev.schema_version, "1");
}

#[test]
fn test_audit_event_with_error_sets_failed_status() {
    let ev = AuditEvent::new("rotate", "app/db", "vault").with_error("connection refused");
    assert_eq!(ev.status, "failed");
    assert_eq!(ev.error.as_deref(), Some("connection refused"));
}

#[test]
fn test_audit_event_with_duration() {
    let ev = AuditEvent::new("rotate", "app/db", "vault").with_duration(150);
    assert_eq!(ev.duration_ms, 150);
}

#[test]
fn test_audit_event_with_target() {
    let ev = AuditEvent::new("rotate", "app/db", "vault").with_target("PostgresDB");
    assert_eq!(ev.target.as_deref(), Some("PostgresDB"));
}

#[test]
fn test_audit_event_with_username() {
    let ev = AuditEvent::new("rotate", "app/db", "vault").with_username("admin");
    assert_eq!(ev.username.as_deref(), Some("admin"));
}

#[test]
fn test_audit_event_with_status() {
    let ev = AuditEvent::new("rotate", "app/db", "vault").with_status("skipped");
    assert_eq!(ev.status, "skipped");
}

#[test]
fn test_audit_event_serializes_to_valid_json() {
    let ev = AuditEvent::new("rotate", "app/db", "vault")
        .with_duration(42)
        .with_target("MySQL")
        .with_username("root");
    let json = serde_json::to_string(&ev).expect("serialization should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("serialized output must be valid JSON");
    assert_eq!(parsed["event"], "rotate");
    assert_eq!(parsed["path"], "app/db");
    assert_eq!(parsed["backend"], "vault");
    assert_eq!(parsed["status"], "success");
    assert_eq!(parsed["duration_ms"], 42);
    assert_eq!(parsed["target"], "MySQL");
    assert_eq!(parsed["username"], "root");
}

#[test]
fn test_audit_event_error_field_skipped_when_absent() {
    let ev = AuditEvent::new("rotate", "app/db", "vault");
    let json = serde_json::to_string(&ev).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed.get("error").is_none(),
        "error key should be omitted when None"
    );
    assert!(
        parsed.get("target").is_none(),
        "target key should be omitted when None"
    );
    assert!(
        parsed.get("username").is_none(),
        "username key should be omitted when None"
    );
}

// ---------------------------------------------------------------------------
// AuditLogger — enabled / disabled
// ---------------------------------------------------------------------------

#[test]
fn test_audit_logger_disabled_when_no_file_no_stdout() {
    let logger = AuditLogger::new(&disabled_config());
    assert!(!logger.is_enabled());
}

#[test]
fn test_audit_logger_enabled_when_file_configured() {
    let logger = AuditLogger::new(&file_config("/tmp/asr-test-audit.jsonl"));
    assert!(logger.is_enabled());
}

#[test]
fn test_audit_logger_enabled_when_stdout() {
    let logger = AuditLogger::new(&stdout_config());
    assert!(logger.is_enabled());
}

// ---------------------------------------------------------------------------
// AuditLogger — file writes
// ---------------------------------------------------------------------------

#[test]
fn test_audit_logger_writes_jsonl_to_file() {
    let dir = TempDir::new().unwrap();
    let log_path = dir.path().join("audit.jsonl");
    let logger = AuditLogger::new(&file_config(log_path.to_str().unwrap()));

    let ev = AuditEvent::new("rotate", "app/db", "vault");
    logger.log(&ev);

    let contents = std::fs::read_to_string(&log_path).expect("log file should exist");
    let line = contents.trim();
    assert!(!line.is_empty(), "log file should not be empty");
    let parsed: serde_json::Value =
        serde_json::from_str(line).expect("log line must be valid JSON");
    assert_eq!(parsed["event"], "rotate");
    assert_eq!(parsed["path"], "app/db");
}

#[test]
fn test_audit_logger_multiple_events_append_not_overwrite() {
    let dir = TempDir::new().unwrap();
    let log_path = dir.path().join("audit.jsonl");
    let logger = AuditLogger::new(&file_config(log_path.to_str().unwrap()));

    logger.log(&AuditEvent::new("rotate", "app/secret1", "vault"));
    logger.log(&AuditEvent::new("rotate", "app/secret2", "vault"));
    logger.log(&AuditEvent::new("flag", "app/secret3", "vault"));

    let contents = std::fs::read_to_string(&log_path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 3, "should have 3 lines");

    for line in &lines {
        let parsed: serde_json::Value =
            serde_json::from_str(line).expect("each line must be valid JSON");
        assert!(parsed.get("event").is_some());
    }

    let paths: Vec<String> = lines
        .iter()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            v["path"].as_str().unwrap().to_string()
        })
        .collect();
    assert!(paths.iter().any(|p| p == "app/secret1"));
    assert!(paths.iter().any(|p| p == "app/secret2"));
    assert!(paths.iter().any(|p| p == "app/secret3"));
}

#[test]
fn test_audit_logger_creates_parent_directories() {
    let dir = TempDir::new().unwrap();
    let log_path = dir.path().join("deeply").join("nested").join("audit.jsonl");
    let logger = AuditLogger::new(&file_config(log_path.to_str().unwrap()));

    logger.log(&AuditEvent::new("rotate", "app/db", "file"));

    assert!(log_path.exists(), "log file should have been created");
    let contents = std::fs::read_to_string(&log_path).unwrap();
    let _: serde_json::Value =
        serde_json::from_str(contents.trim()).expect("written line must be valid JSON");
}

#[test]
fn test_audit_logger_no_op_when_disabled() {
    let dir = TempDir::new().unwrap();
    let log_path = dir.path().join("audit.jsonl");

    let logger = AuditLogger::new(&disabled_config());
    logger.log(&AuditEvent::new("rotate", "app/db", "vault"));

    assert!(
        !log_path.exists(),
        "disabled logger should not create a file"
    );
}

#[test]
fn test_audit_event_asr_version_is_present() {
    let ev = AuditEvent::new("rotate", "app/db", "vault");
    assert!(
        !ev.asr_version.is_empty(),
        "asr_version should be populated"
    );
}
