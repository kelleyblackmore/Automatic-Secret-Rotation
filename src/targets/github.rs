#![cfg(feature = "github")]

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

use crate::config::GitHubTargetConfig;
use crate::targets::target::Target;

const GITHUB_API: &str = "https://api.github.com";
const API_VERSION: &str = "2022-11-28";

pub struct GitHubTarget {
    api_base: String,
    owner: String,
    repo: String,
    secret_name: Option<String>,
    variable_name: Option<String>,
    token: String,
    env_name: Option<String>,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct PublicKeyResponse {
    key_id: String,
    key: String,
}

impl GitHubTarget {
    pub async fn new(config: &GitHubTargetConfig) -> Result<Self> {
        match (&config.secret_name, &config.variable_name) {
            (None, None) => {
                anyhow::bail!("GitHub target requires either secret_name or variable_name")
            }
            (Some(_), Some(_)) => {
                anyhow::bail!("GitHub target: secret_name and variable_name are mutually exclusive")
            }
            _ => {}
        }

        let token = config
            .token
            .clone()
            .or_else(|| std::env::var("GITHUB_TOKEN").ok())
            .context("GitHub token not set — provide token in config or set GITHUB_TOKEN")?;

        Ok(Self {
            api_base: config
                .api_url
                .as_deref()
                .unwrap_or(GITHUB_API)
                .trim_end_matches('/')
                .to_string(),
            owner: config.owner.clone(),
            repo: config.repo.clone(),
            secret_name: config.secret_name.clone(),
            variable_name: config.variable_name.clone(),
            token,
            env_name: config.env_name.clone(),
            client: crate::util::http::build_http_client(30)?,
        })
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    fn gh_request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, url)
            .header("Authorization", self.auth_header())
            .header("User-Agent", "asr-secret-rotator")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
    }

    async fn update_secret(&self, value: &str) -> Result<()> {
        let name = self.secret_name.as_deref().unwrap();

        // Step 1: fetch the public key for NaCl sealed-box encryption
        let pk_url = match &self.env_name {
            Some(env) => format!(
                "{}/repos/{}/{}/environments/{}/secrets/public-key",
                self.api_base, self.owner, self.repo, env
            ),
            None => format!(
                "{}/repos/{}/{}/actions/secrets/public-key",
                self.api_base, self.owner, self.repo
            ),
        };

        let pk_resp = self
            .gh_request(reqwest::Method::GET, &pk_url)
            .send()
            .await
            .context("Failed to fetch GitHub Actions public key")?;

        let pk: PublicKeyResponse =
            crate::util::http::require_success(pk_resp, "GitHub public key fetch")
                .await?
                .json()
                .await
                .context("Failed to parse GitHub public key response")?;

        // Step 2: encrypt the value with NaCl sealed box
        let encrypted =
            encrypt_secret(&pk.key, value.as_bytes()).context("Failed to encrypt secret")?;

        // Step 3: create or update the secret (PUT is idempotent — creates or updates)
        let secret_url = match &self.env_name {
            Some(env) => format!(
                "{}/repos/{}/{}/environments/{}/secrets/{}",
                self.api_base, self.owner, self.repo, env, name
            ),
            None => format!(
                "{}/repos/{}/{}/actions/secrets/{}",
                self.api_base, self.owner, self.repo, name
            ),
        };

        let body = serde_json::json!({
            "encrypted_value": encrypted,
            "key_id": pk.key_id,
        });

        let resp = self
            .gh_request(reqwest::Method::PUT, &secret_url)
            .json(&body)
            .send()
            .await
            .context("Failed to update GitHub Actions secret")?;

        // GitHub returns 201 Created or 204 No Content
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub secret update failed (HTTP {}): {}", status, text);
        }

        Ok(())
    }

    async fn update_variable(&self, value: &str) -> Result<()> {
        let name = self.variable_name.as_deref().unwrap();

        let (list_url, item_url) = match &self.env_name {
            Some(env) => (
                format!(
                    "{}/repos/{}/{}/environments/{}/variables",
                    self.api_base, self.owner, self.repo, env
                ),
                format!(
                    "{}/repos/{}/{}/environments/{}/variables/{}",
                    self.api_base, self.owner, self.repo, env, name
                ),
            ),
            None => (
                format!(
                    "{}/repos/{}/{}/actions/variables",
                    self.api_base, self.owner, self.repo
                ),
                format!(
                    "{}/repos/{}/{}/actions/variables/{}",
                    self.api_base, self.owner, self.repo, name
                ),
            ),
        };

        // Check whether the variable already exists
        let get_resp = self
            .gh_request(reqwest::Method::GET, &item_url)
            .send()
            .await
            .context("Failed to check GitHub Actions variable existence")?;

        let (method, url) = if get_resp.status() == reqwest::StatusCode::NOT_FOUND {
            (reqwest::Method::POST, list_url)
        } else {
            crate::util::http::require_success(get_resp, "GitHub variable check").await?;
            (reqwest::Method::PATCH, item_url)
        };

        let body = serde_json::json!({ "name": name, "value": value });

        let resp = self
            .gh_request(method, &url)
            .json(&body)
            .send()
            .await
            .context("Failed to update GitHub Actions variable")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub variable update failed (HTTP {}): {}", status, text);
        }

        Ok(())
    }
}

#[async_trait]
impl Target for GitHubTarget {
    async fn update_password(&self, _username: &str, new_password: &str) -> Result<()> {
        if self.secret_name.is_some() {
            self.update_secret(new_password).await
        } else {
            self.update_variable(new_password).await
        }
    }

    async fn verify_connection(
        &self,
        _username: &str,
        _password: &str,
        _extra: Option<&str>,
    ) -> Result<()> {
        let url = format!("{}/repos/{}/{}", self.api_base, self.owner, self.repo);
        let resp = self
            .gh_request(reqwest::Method::GET, &url)
            .send()
            .await
            .context("Failed to verify GitHub connection")?;
        crate::util::http::require_success(resp, "GitHub repo access verify").await?;
        Ok(())
    }

    fn target_type(&self) -> &'static str {
        "GitHub"
    }

    fn requires_username(&self) -> bool {
        false
    }
}

/// Implements libsodium's `crypto_box_seal` (anonymous sender sealed box).
///
/// Output: ephemeral_pk (32 bytes) || XSalsa20Poly1305_ciphertext
/// Nonce:  Blake2b(ephemeral_pk || recipient_pk, outlen=24)
///
/// This matches what GitHub's API expects for encrypted Action secrets.
fn encrypt_secret(public_key_b64: &str, plaintext: &[u8]) -> Result<String> {
    use blake2::{digest::VariableOutput, Blake2bVar};
    use crypto_box::{
        aead::{generic_array::GenericArray, Aead, OsRng},
        PublicKey, SalsaBox, SecretKey,
    };

    let pk_bytes = crate::util::base64::decode(public_key_b64)
        .context("Failed to base64-decode GitHub public key")?;
    let pk_array: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("GitHub public key must be exactly 32 bytes"))?;
    let recipient_pk = PublicKey::from(pk_array);

    // Generate an ephemeral keypair — the "anonymous sender" part of sealed box
    let ephemeral_sk = SecretKey::generate(&mut OsRng);
    let ephemeral_pk = ephemeral_sk.public_key();

    // Nonce = BLAKE2b(ephemeral_pk || recipient_pk, outlen=24) — per libsodium spec
    let mut nonce_bytes = [0u8; 24];
    {
        let mut h = Blake2bVar::new(24).expect("24 is a valid Blake2b output size");
        blake2::digest::Update::update(&mut h, ephemeral_pk.as_bytes());
        blake2::digest::Update::update(&mut h, &pk_array);
        h.finalize_variable(&mut nonce_bytes)
            .expect("output buffer matches requested length");
    }
    let nonce = GenericArray::clone_from_slice(&nonce_bytes);

    // X25519 DH + XSalsa20Poly1305 encrypt
    let salsa_box = SalsaBox::new(&recipient_pk, &ephemeral_sk);
    let ciphertext = salsa_box
        .encrypt(&nonce, plaintext)
        .map_err(|_| anyhow::anyhow!("Sealed box encryption failed"))?;

    // Sealed box wire format: ephemeral_pk (32 bytes) || ciphertext
    let mut output = Vec::with_capacity(32 + ciphertext.len());
    output.extend_from_slice(ephemeral_pk.as_bytes());
    output.extend_from_slice(&ciphertext);

    Ok(crate::util::base64::encode(&output))
}
