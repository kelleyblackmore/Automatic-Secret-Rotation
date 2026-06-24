use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

use super::secret_backend::{SecretBackend, SecretData};

/// HashiCorp Vault client
#[derive(Clone)]
pub struct VaultClient {
    client: Client,
    pub address: String,
    token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub custom_metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VaultSecretData {
    pub data: HashMap<String, String>,
    pub metadata: Option<SecretMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VaultResponse<T> {
    data: T,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VaultWriteRequest {
    pub data: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<HashMap<String, String>>,
}

impl VaultClient {
    /// Create a new Vault client
    pub fn new(address: String, token: String) -> Result<Self> {
        let client = crate::util::http::build_http_client(30)?;

        Ok(Self {
            client,
            address,
            token,
        })
    }

    /// Read a secret from Vault KV v2
    pub async fn read_secret(&self, mount: &str, path: &str) -> Result<VaultSecretData> {
        let url = format!("{}/v1/{}/data/{}", self.address, mount, path);
        debug!("Reading secret from: {}", url);

        let response = self
            .client
            .get(&url)
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .context("Failed to read secret from Vault")?;

        let vault_response: VaultResponse<VaultSecretData> =
            crate::util::http::require_success(response, "Vault read secret")
                .await?
                .json()
                .await
                .context("Failed to parse Vault response")?;

        Ok(vault_response.data)
    }

    /// Write a secret to Vault KV v2
    pub async fn write_secret(
        &self,
        mount: &str,
        path: &str,
        data: HashMap<String, String>,
    ) -> Result<()> {
        let url = format!("{}/v1/{}/data/{}", self.address, mount, path);
        debug!("Writing secret to: {}", url);

        let request_body = VaultWriteRequest {
            data,
            options: None,
        };

        let response = self
            .client
            .post(&url)
            .header("X-Vault-Token", &self.token)
            .json(&request_body)
            .send()
            .await
            .context("Failed to write secret to Vault")?;

        crate::util::http::require_success(response, "Vault write secret").await?;

        info!("Successfully wrote secret to {}/{}", mount, path);
        Ok(())
    }

    /// Update secret metadata
    pub async fn update_metadata(
        &self,
        mount: &str,
        path: &str,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        let url = format!("{}/v1/{}/metadata/{}", self.address, mount, path);
        debug!("Updating metadata at: {}", url);

        let mut body = HashMap::new();
        body.insert("custom_metadata", metadata);

        let response = self
            .client
            .post(&url)
            .header("X-Vault-Token", &self.token)
            .json(&body)
            .send()
            .await
            .context("Failed to update metadata")?;

        crate::util::http::require_success(response, "Vault update metadata").await?;

        info!("Successfully updated metadata for {}/{}", mount, path);
        Ok(())
    }

    /// Read secret metadata
    pub async fn read_metadata(&self, mount: &str, path: &str) -> Result<SecretMetadata> {
        let url = format!("{}/v1/{}/metadata/{}", self.address, mount, path);
        debug!("Reading metadata from: {}", url);

        let response = self
            .client
            .get(&url)
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .context("Failed to read metadata from Vault")?;

        let vault_response: VaultResponse<SecretMetadata> =
            crate::util::http::require_success(response, "Vault read metadata")
                .await?
                .json()
                .await
                .context("Failed to parse Vault metadata response")?;

        Ok(vault_response.data)
    }

    /// List secrets in a path
    pub async fn list_secrets(&self, mount: &str, path: &str) -> Result<Vec<String>> {
        let url = format!("{}/v1/{}/metadata/{}", self.address, mount, path);
        debug!("Listing secrets at: {}", url);

        let response = self
            .client
            .request(reqwest::Method::from_bytes(b"LIST").unwrap(), &url)
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .context("Failed to list secrets from Vault")?;

        // 404 means no secrets exist at this path, which is fine
        if response.status() == 404 {
            info!("No secrets found at {}/{} (empty path)", mount, path);
            return Ok(vec![]);
        }

        #[derive(Deserialize)]
        struct ListData {
            keys: Vec<String>,
        }

        let vault_response: VaultResponse<ListData> =
            crate::util::http::require_success(response, "Vault list secrets")
                .await?
                .json()
                .await
                .context("Failed to parse Vault list response")?;

        Ok(vault_response.data.keys)
    }
}

/// Wrapper for VaultClient that implements SecretBackend trait
pub struct VaultBackend {
    client: VaultClient,
    mount: String,
}

impl VaultBackend {
    pub fn new(client: VaultClient, mount: String) -> Self {
        Self { client, mount }
    }

    /// Create a VaultBackend from config, performing dynamic auth if needed.
    pub async fn from_config(config: &crate::config::VaultConfig) -> Result<Self> {
        let http = crate::util::http::build_http_client(30)?;
        let token = resolve_vault_token(config, &http).await?;
        let vault_client =
            VaultClient::new(config.address.trim_end_matches('/').to_string(), token)?;
        Ok(Self::new(vault_client, config.mount.clone()))
    }
}

// ---------------------------------------------------------------------------
// Vault auth method implementations
// ---------------------------------------------------------------------------

/// Resolve a Vault client token using the configured auth method.
async fn resolve_vault_token(
    config: &crate::config::VaultConfig,
    http: &Client,
) -> Result<String> {
    match config.auth_method.to_lowercase().as_str() {
        "token" | "" => auth_token(config),
        "approle" => auth_approle(config, http).await,
        "kubernetes" | "k8s" => auth_kubernetes(config, http).await,
        "aws" | "aws_iam" => auth_aws_iam(config, http).await,
        "jwt" | "oidc" => auth_jwt(config, http).await,
        other => anyhow::bail!(
            "Unknown vault auth_method '{}'. Supported: token, approle, kubernetes, aws_iam, jwt",
            other
        ),
    }
}

fn auth_token(config: &crate::config::VaultConfig) -> Result<String> {
    config
        .token
        .clone()
        .or_else(|| std::env::var("VAULT_TOKEN").ok())
        .context(
            "Vault token not found. Set vault.token, VAULT_TOKEN, or configure a \
             different auth_method (approle, kubernetes, aws_iam, jwt).",
        )
}

async fn auth_approle(config: &crate::config::VaultConfig, http: &Client) -> Result<String> {
    let ar = config
        .approle
        .as_ref()
        .context("vault.auth_method = \"approle\" requires a [vault.approle] section")?;

    let secret_id = ar
        .secret_id
        .clone()
        .or_else(|| ar.secret_id_env.as_ref().and_then(|e| std::env::var(e).ok()))
        .or_else(|| std::env::var("VAULT_SECRET_ID").ok())
        .context(
            "AppRole secret_id not found. Set vault.approle.secret_id, \
             vault.approle.secret_id_env, or VAULT_SECRET_ID.",
        )?;

    let url = format!(
        "{}/v1/auth/{}/login",
        config.address.trim_end_matches('/'),
        ar.mount
    );
    let resp = http
        .post(&url)
        .json(&serde_json::json!({ "role_id": ar.role_id, "secret_id": secret_id }))
        .send()
        .await
        .context("Vault AppRole login request failed")?;

    vault_extract_token(resp, "AppRole").await
}

async fn auth_kubernetes(config: &crate::config::VaultConfig, http: &Client) -> Result<String> {
    let k8s = config
        .kubernetes
        .as_ref()
        .context("vault.auth_method = \"kubernetes\" requires a [vault.kubernetes] section")?;

    let jwt = std::fs::read_to_string(&k8s.sa_token_path).with_context(|| {
        format!(
            "Failed to read Kubernetes ServiceAccount token from {}",
            k8s.sa_token_path
        )
    })?;

    let url = format!(
        "{}/v1/auth/{}/login",
        config.address.trim_end_matches('/'),
        k8s.mount
    );
    let resp = http
        .post(&url)
        .json(&serde_json::json!({ "role": k8s.role, "jwt": jwt.trim() }))
        .send()
        .await
        .context("Vault Kubernetes login request failed")?;

    vault_extract_token(resp, "Kubernetes").await
}

async fn auth_aws_iam(config: &crate::config::VaultConfig, http: &Client) -> Result<String> {
    let aws = config
        .aws_iam
        .as_ref()
        .context("vault.auth_method = \"aws_iam\" requires a [vault.aws_iam] section")?;

    let (url_b64, headers_b64, body_b64) =
        build_signed_sts_request(aws.header_value.as_deref()).await?;

    let login_url = format!(
        "{}/v1/auth/{}/login",
        config.address.trim_end_matches('/'),
        aws.mount
    );
    let resp = http
        .post(&login_url)
        .json(&serde_json::json!({
            "role": aws.role,
            "iam_http_request_method": "POST",
            "iam_request_url": url_b64,
            "iam_request_headers": headers_b64,
            "iam_request_body": body_b64,
        }))
        .send()
        .await
        .context("Vault AWS IAM login request failed")?;

    vault_extract_token(resp, "AWS IAM").await
}

async fn auth_jwt(config: &crate::config::VaultConfig, http: &Client) -> Result<String> {
    let jwt_cfg = config
        .jwt
        .as_ref()
        .context("vault.auth_method = \"jwt\" requires a [vault.jwt] section")?;

    let jwt = resolve_jwt_token(jwt_cfg, http).await?;

    let url = format!(
        "{}/v1/auth/{}/login",
        config.address.trim_end_matches('/'),
        jwt_cfg.mount
    );
    let resp = http
        .post(&url)
        .json(&serde_json::json!({ "role": jwt_cfg.role, "jwt": jwt }))
        .send()
        .await
        .context("Vault JWT/OIDC login request failed")?;

    vault_extract_token(resp, "JWT/OIDC").await
}

async fn resolve_jwt_token(
    cfg: &crate::config::VaultJwtConfig,
    http: &Client,
) -> Result<String> {
    if let Some(ref env) = cfg.token_env {
        return std::env::var(env)
            .with_context(|| format!("JWT token env var '{}' not set", env));
    }

    // GitLab CI: CI_JOB_JWT_V2 (OIDC) preferred over the older CI_JOB_JWT
    if let Ok(token) = std::env::var("CI_JOB_JWT_V2").or_else(|_| std::env::var("CI_JOB_JWT")) {
        return Ok(token);
    }

    // GitHub Actions OIDC — requires `id-token: write` permission in the workflow
    if let (Ok(req_url), Ok(req_tok)) = (
        std::env::var("ACTIONS_ID_TOKEN_REQUEST_URL"),
        std::env::var("ACTIONS_ID_TOKEN_REQUEST_TOKEN"),
    ) {
        return fetch_github_actions_oidc(&req_url, &req_tok, http).await;
    }

    std::env::var("VAULT_JWT_TOKEN").context(
        "No JWT/OIDC token found. Set vault.jwt.token_env, CI_JOB_JWT, VAULT_JWT_TOKEN, \
         or add `id-token: write` to your GitHub Actions workflow permissions.",
    )
}

async fn fetch_github_actions_oidc(
    request_url: &str,
    request_token: &str,
    http: &Client,
) -> Result<String> {
    let url = if request_url.contains('?') {
        format!("{}&audience=vault", request_url)
    } else {
        format!("{}?audience=vault", request_url)
    };

    #[derive(serde::Deserialize)]
    struct OidcResponse {
        value: String,
    }

    let resp = http
        .get(&url)
        .bearer_auth(request_token)
        .send()
        .await
        .context("Failed to fetch GitHub Actions OIDC token")?;

    let oidc: OidcResponse =
        crate::util::http::require_success(resp, "GitHub Actions OIDC token fetch")
            .await?
            .json()
            .await
            .context("Failed to parse GitHub OIDC token response")?;

    Ok(oidc.value)
}

/// Parse a Vault auth login response and extract `auth.client_token`.
async fn vault_extract_token(resp: reqwest::Response, auth_type: &str) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct VaultAuthResp {
        auth: VaultAuth,
    }
    #[derive(serde::Deserialize)]
    struct VaultAuth {
        client_token: String,
    }

    let body: VaultAuthResp =
        crate::util::http::require_success(resp, &format!("Vault {} auth", auth_type))
            .await?
            .json()
            .await
            .with_context(|| format!("Failed to parse Vault {} auth response", auth_type))?;

    Ok(body.auth.client_token)
}

// ---------------------------------------------------------------------------
// AWS SigV4 helpers for AWS IAM auth
// ---------------------------------------------------------------------------

/// Sign a `GetCallerIdentity` POST request with SigV4 and return the three
/// base64-encoded fields that Vault's `aws` auth method expects:
/// `(iam_request_url, iam_request_headers, iam_request_body)`.
async fn build_signed_sts_request(vault_header: Option<&str>) -> Result<(String, String, String)> {
    use aws_credential_types::provider::ProvideCredentials;
    use sha2::Digest;

    let aws_cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let creds = aws_cfg
        .credentials_provider()
        .context(
            "No AWS credentials provider found. \
             Configure AWS credentials (env vars, instance profile, IRSA, etc.).",
        )?
        .provide_credentials()
        .await
        .context("Failed to load AWS credentials for Vault aws_iam auth")?;

    let region = aws_cfg
        .region()
        .map(|r| r.to_string())
        .unwrap_or_else(|| {
            std::env::var("AWS_REGION")
                .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
                .unwrap_or_else(|_| "us-east-1".to_string())
        });

    let sts_body: &[u8] = b"Action=GetCallerIdentity&Version=2011-06-15";
    let sts_url = "https://sts.amazonaws.com/";

    let now = chrono::Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();

    // Collect and sort headers (canonical form requires lowercase, sorted names)
    let mut headers: Vec<(String, String)> = vec![
        (
            "content-type".to_string(),
            "application/x-www-form-urlencoded; charset=utf-8".to_string(),
        ),
        ("host".to_string(), "sts.amazonaws.com".to_string()),
        ("x-amz-date".to_string(), amz_date.clone()),
    ];

    if let Some(st) = creds.session_token() {
        headers.push(("x-amz-security-token".to_string(), st.to_string()));
    }
    if let Some(h) = vault_header {
        headers.push(("x-vault-aws-iam-server-id".to_string(), h.to_string()));
    }

    headers.sort_by(|a, b| a.0.cmp(&b.0));

    let signed_headers: String = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_headers: String = headers
        .iter()
        .map(|(k, v)| format!("{}:{}\n", k, v.trim()))
        .collect();

    let body_hash = hex_str(&sha2::Sha256::digest(sts_body));

    let canonical_request = format!(
        "POST\n/\n\n{}\n{}\n{}",
        canonical_headers, signed_headers, body_hash
    );

    let credential_scope = format!("{}/{}/sts/aws4_request", date_stamp, region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        credential_scope,
        hex_str(&sha2::Sha256::digest(canonical_request.as_bytes()))
    );

    // HMAC chain: HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date), region), "sts"), "aws4_request")
    let signing_key = {
        let k = sigv4_hmac(
            format!("AWS4{}", creds.secret_access_key()).as_bytes(),
            date_stamp.as_bytes(),
        );
        let k = sigv4_hmac(&k, region.as_bytes());
        let k = sigv4_hmac(&k, b"sts");
        sigv4_hmac(&k, b"aws4_request")
    };

    let signature = hex_str(&sigv4_hmac(&signing_key, string_to_sign.as_bytes()));

    let auth_header = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        creds.access_key_id(),
        credential_scope,
        signed_headers,
        signature
    );

    // Vault expects headers as a JSON map: { "name": ["value"], ... }
    let mut hdr_map: std::collections::BTreeMap<String, serde_json::Value> = headers
        .into_iter()
        .map(|(k, v)| (k, serde_json::json!([v])))
        .collect();
    hdr_map.insert(
        "authorization".to_string(),
        serde_json::json!([auth_header]),
    );

    let headers_json =
        serde_json::to_string(&hdr_map).context("Failed to serialize STS request headers")?;

    Ok((
        crate::util::base64::encode(sts_url.as_bytes()),
        crate::util::base64::encode(headers_json.as_bytes()),
        crate::util::base64::encode(sts_body),
    ))
}

fn sigv4_hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = <Hmac<Sha256>>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hex_str(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[async_trait::async_trait]
impl SecretBackend for VaultBackend {
    async fn read_secret(&self, path: &str) -> Result<SecretData> {
        let vault_data = self.client.read_secret(&self.mount, path).await?;

        let metadata = vault_data.metadata.and_then(|m| m.custom_metadata);

        Ok(SecretData {
            data: vault_data.data,
            metadata: metadata.clone(),
        })
    }

    async fn write_secret(&self, path: &str, data: HashMap<String, String>) -> Result<()> {
        self.client.write_secret(&self.mount, path, data).await
    }

    async fn update_metadata(&self, path: &str, metadata: HashMap<String, String>) -> Result<()> {
        self.client
            .update_metadata(&self.mount, path, metadata)
            .await
    }

    async fn read_metadata(&self, path: &str) -> Result<HashMap<String, String>> {
        let metadata = self.client.read_metadata(&self.mount, path).await?;
        Ok(metadata.custom_metadata.unwrap_or_default())
    }

    async fn list_secrets(&self, path: &str) -> Result<Vec<String>> {
        self.client.list_secrets(&self.mount, path).await
    }

    fn backend_type(&self) -> &'static str {
        "HashiCorp Vault"
    }
}
