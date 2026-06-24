#![cfg(feature = "gcp")]

//! GCP Secret Manager backend via REST API.
//!
//! Authentication: set `GOOGLE_ACCESS_TOKEN` to a valid bearer token, or run on
//! GCP (GCE/GKE/Cloud Run) — the token is fetched from the metadata server.
//!
//! Required env vars:
//!   GCP_PROJECT_ID       – GCP project ID
//!   GOOGLE_ACCESS_TOKEN  (optional) – pre-fetched token; otherwise metadata server is used

use std::collections::HashMap;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

use super::secret_backend::{SecretBackend, SecretData};
use crate::config::GcpConfig;

const API_BASE: &str = "https://secretmanager.googleapis.com/v1";
const LABEL_PREFIX: &str = "asr-";

pub struct GcpSecretManagerBackend {
    project_id: String,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct SecretVersion {
    payload: Option<SecretPayload>,
}

#[derive(Deserialize)]
struct SecretPayload {
    data: String, // base64-encoded
}

#[derive(Deserialize)]
struct SecretResource {
    labels: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct ListSecretsResponse {
    secrets: Option<Vec<SecretResource2>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct SecretResource2 {
    name: String,
}

impl GcpSecretManagerBackend {
    pub async fn new(config: &GcpConfig) -> Result<Self> {
        if let Some(ref path) = config.credentials_file {
            std::env::set_var("GOOGLE_APPLICATION_CREDENTIALS", path);
        }

        Ok(Self {
            project_id: config.project_id.clone(),
            client: crate::util::http::build_http_client(30)?,
        })
    }

    async fn bearer_token(&self) -> Result<String> {
        if let Ok(token) = std::env::var("GOOGLE_ACCESS_TOKEN") {
            return Ok(token);
        }

        // GCP metadata server
        let resp = self.client
            .get("http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token")
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .context("Failed to fetch token from GCP metadata server. Set GOOGLE_ACCESS_TOKEN env var if not running on GCP.")?;

        #[derive(Deserialize)]
        struct Token {
            access_token: String,
        }
        let token: Token = resp.json().await.context("Failed to parse GCP token")?;
        Ok(token.access_token)
    }

    fn secret_resource_name(&self, path: &str) -> String {
        let name = crate::util::path::path_to_k8s_name(path);
        format!("projects/{}/secrets/{}", self.project_id, name)
    }

    fn path_from_resource_name(name: &str) -> String {
        // "projects/proj/secrets/my-secret" → "my/secret"
        let seg = name.rsplit('/').next().unwrap_or(name);
        crate::util::path::k8s_name_to_path(seg)
    }

    async fn ensure_secret_exists(&self, path: &str, token: &str) -> Result<()> {
        let secret_id = crate::util::path::path_to_k8s_name(path);
        let url = format!(
            "{}/projects/{}/secrets?secretId={}",
            API_BASE, self.project_id, secret_id
        );
        let body = serde_json::json!({
            "replication": { "automatic": {} }
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to create GCP secret: {}", path))?;

        let status = resp.status();
        if !status.is_success() && status.as_u16() != 409 {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("GCP Secret Manager create {} → {}: {}", path, status, text);
        }

        Ok(())
    }
}

#[async_trait]
impl SecretBackend for GcpSecretManagerBackend {
    async fn read_secret(&self, path: &str) -> Result<SecretData> {
        let token = self.bearer_token().await?;
        let resource = self.secret_resource_name(path);
        let url = format!("{}/{}/versions/latest:access", API_BASE, resource);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .with_context(|| format!("Failed to read GCP secret: {}", path))?;

        let version: SecretVersion =
            crate::util::http::require_success(resp, &format!("GCP Secret Manager read {}", path))
                .await?
                .json()
                .await
                .context("Failed to parse GCP secret")?;

        let encoded = version.payload.map(|p| p.data).unwrap_or_default();
        let bytes = base64_decode(&encoded).context("Failed to decode GCP secret payload")?;
        let value = String::from_utf8(bytes).context("GCP secret is not valid UTF-8")?;

        let data: HashMap<String, String> =
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&value) {
                map
            } else {
                let mut m = HashMap::new();
                m.insert("value".to_string(), value);
                m
            };

        let metadata = self.read_metadata(path).await.ok();

        Ok(SecretData { data, metadata })
    }

    async fn write_secret(&self, path: &str, data: HashMap<String, String>) -> Result<()> {
        let token = self.bearer_token().await?;
        self.ensure_secret_exists(path, &token).await?;

        let value = serde_json::to_string(&data).context("Failed to serialize secret data")?;
        let encoded = base64_encode(value.as_bytes());

        let resource = self.secret_resource_name(path);
        let url = format!("{}/{}/versions:add", API_BASE, resource);
        let body = serde_json::json!({ "payload": { "data": encoded } });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to write GCP secret: {}", path))?;

        crate::util::http::require_success(resp, &format!("GCP Secret Manager write {}", path))
            .await?;

        Ok(())
    }

    async fn update_metadata(&self, path: &str, metadata: HashMap<String, String>) -> Result<()> {
        let token = self.bearer_token().await?;
        let resource = self.secret_resource_name(path);
        let url = format!("{}/{}?updateMask=labels", API_BASE, resource);

        let labels: HashMap<String, String> = metadata
            .into_iter()
            .map(|(k, v)| {
                let label_key = format!("{}{}", LABEL_PREFIX, k)
                    .replace('_', "-")
                    .to_lowercase();
                (label_key, v)
            })
            .collect();

        let body = serde_json::json!({ "labels": labels });

        let resp = self
            .client
            .patch(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to update GCP secret metadata: {}", path))?;

        crate::util::http::require_success(
            resp,
            &format!("GCP Secret Manager metadata update {}", path),
        )
        .await?;

        Ok(())
    }

    async fn read_metadata(&self, path: &str) -> Result<HashMap<String, String>> {
        let token = self.bearer_token().await?;
        let resource = self.secret_resource_name(path);
        let url = format!("{}/{}", API_BASE, resource);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .with_context(|| format!("Failed to read GCP secret metadata: {}", path))?;

        let secret: SecretResource = crate::util::http::require_success(
            resp,
            &format!("GCP Secret Manager metadata read {}", path),
        )
        .await?
        .json()
        .await
        .context("Failed to parse GCP secret")?;

        let metadata: HashMap<String, String> = secret
            .labels
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, v)| {
                k.strip_prefix(LABEL_PREFIX)
                    .map(|stripped| (stripped.replace('-', "_"), v))
            })
            .collect();

        Ok(metadata)
    }

    async fn list_secrets(&self, path: &str) -> Result<Vec<String>> {
        let token = self.bearer_token().await?;
        let prefix = if path.is_empty() {
            String::new()
        } else {
            crate::util::path::path_to_k8s_name(path)
        };

        let mut all = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = format!(
                "{}/projects/{}/secrets?pageSize=100",
                API_BASE, self.project_id
            );
            if let Some(ref pt) = page_token {
                url.push_str(&format!("&pageToken={}", pt));
            }
            if !prefix.is_empty() {
                // GCP filter by prefix on name
                url.push_str(&format!("&filter=name:{}-", prefix));
            }

            let resp = self
                .client
                .get(&url)
                .bearer_auth(&token)
                .send()
                .await
                .context("Failed to list GCP secrets")?;

            let list: ListSecretsResponse =
                crate::util::http::require_success(resp, "GCP Secret Manager list")
                    .await?
                    .json()
                    .await
                    .context("Failed to parse secret list")?;

            for secret in list.secrets.unwrap_or_default() {
                all.push(Self::path_from_resource_name(&secret.name));
            }

            match list.next_page_token {
                Some(t) if !t.is_empty() => page_token = Some(t),
                _ => break,
            }
        }

        Ok(all)
    }

    fn backend_type(&self) -> &'static str {
        "GCP Secret Manager"
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(CHARS[b0 >> 2] as char);
        out.push(CHARS[((b0 & 3) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((b1 & 0xf) << 2) | (b2 >> 6)] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[b2 & 0x3f] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode(input: &str) -> Result<Vec<u8>> {
    let input = input.trim_end_matches('=');
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut bits = 0u32;
    let mut n = 0u8;
    for c in input.chars() {
        let v: u32 = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            _ => anyhow::bail!("Invalid base64 character: {}", c),
        };
        bits = (bits << 6) | v;
        n += 1;
        if n == 4 {
            out.push((bits >> 16) as u8);
            out.push((bits >> 8) as u8);
            out.push(bits as u8);
            bits = 0;
            n = 0;
        }
    }
    match n {
        2 => out.push((bits >> 4) as u8),
        3 => {
            out.push((bits >> 10) as u8);
            out.push((bits >> 2) as u8);
        }
        _ => {}
    }
    Ok(out)
}
