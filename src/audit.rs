use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde::Serialize;

use crate::config::AuditConfig;

const SCHEMA_VERSION: &str = "1";

#[derive(Serialize)]
pub struct AuditEvent {
    pub schema_version: &'static str,
    pub event: String,
    pub path: String,
    pub backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub status: String,
    pub duration_ms: u64,
    pub timestamp: String,
    pub asr_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AuditEvent {
    pub fn new(event: &str, path: &str, backend: &str) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            event: event.to_string(),
            path: path.to_string(),
            backend: backend.to_string(),
            target: None,
            username: None,
            status: "success".to_string(),
            duration_ms: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            asr_version: env!("CARGO_PKG_VERSION").to_string(),
            error: None,
        }
    }

    pub fn with_status(mut self, status: &str) -> Self {
        self.status = status.to_string();
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }

    pub fn with_error(mut self, error: &str) -> Self {
        self.status = "failed".to_string();
        self.error = Some(error.to_string());
        self
    }

    pub fn with_target(mut self, target: &str) -> Self {
        self.target = Some(target.to_string());
        self
    }

    pub fn with_username(mut self, username: &str) -> Self {
        self.username = Some(username.to_string());
        self
    }
}

pub struct AuditLogger {
    log_file: Option<String>,
    stdout: bool,
    enabled: bool,
}

impl AuditLogger {
    pub fn new(config: &AuditConfig) -> Self {
        let enabled = config.log_file.is_some() || config.stdout;
        Self {
            log_file: config.log_file.clone(),
            stdout: config.stdout,
            enabled,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn log(&self, event: &AuditEvent) {
        if !self.enabled {
            return;
        }

        let json = match serde_json::to_string(event) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to serialize audit event: {}", e);
                return;
            }
        };

        if self.stdout {
            println!("{}", json);
        }

        if let Some(ref path) = self.log_file {
            if let Err(e) = self.append_to_file(path, &json) {
                tracing::warn!("Failed to write audit log to {}: {}", path, e);
            }
        }
    }

    fn append_to_file(&self, path: &str, line: &str) -> std::io::Result<()> {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{}", line)
    }
}
