# Automatic Secret Rotation (`asr`)

A Rust CLI for automatic secret rotation across multiple backends and target systems.

## Supported backends

| Backend | Config value | Notes |
|---------|-------------|-------|
| HashiCorp Vault KV v2 | `vault` | Always included |
| AWS Secrets Manager | `aws` | Always included |
| Local file | `file` | Dev/testing; flat key:value files |
| Azure Key Vault | `azure` | Requires `--features azure` |
| GCP Secret Manager | `gcp` | Requires `--features gcp` |
| OpenShift / Kubernetes | `ocp` | Requires `--features ocp` |

## Installation

**Linux / macOS:**
```bash
curl -fsSL https://raw.githubusercontent.com/kelleyblackmore/Automatic-Secret-Rotation/main/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/kelleyblackmore/Automatic-Secret-Rotation/main/install.ps1 | iex
```

**From source:**
```bash
cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation
# With optional backends/features:
cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation --features azure,gcp,ocp,mysql
```

> **macOS note:** The system tool `/usr/sbin/asr` (Apple Software Restore) conflicts with this binary name. The installer defaults to `secret-rotator` on macOS. Override: `ASR_BINARY_NAME=asr ./install.sh`.

## Quick start

```bash
asr init                          # Create a config file
asr gen-password myapp/database   # Generate and store a password
asr flag myapp/database --period 3 # Rotate every 3 months
asr scan                          # Check which secrets are due
asr auto --dry-run                # Preview what would be rotated
asr auto                          # Rotate all due secrets
```

## Configuration

`asr` loads config from a TOML file (`-c config.toml`) or environment variables.

```toml
backend = "vault"   # vault | aws | file | azure | gcp | ocp

[vault]
address = "http://127.0.0.1:8200"
token   = "hvs.your-token"
mount   = "secret"

[aws]
region = "us-east-1"

[file]
directory = "~/.asr/secrets"

[azure]
vault_url = "https://my-vault.vault.azure.net"

[gcp]
project_id = "my-gcp-project"

[ocp]
namespace = "my-app"

[rotation]
period_months = 6
secret_length = 32

# Append-only JSONL audit log
[audit]
log_file = "/var/log/asr/audit.jsonl"
stdout   = false

# Webhook / Slack notifications
[notification]
webhook_url = "https://hooks.slack.com/services/..."
events      = ["rotate", "flag", "scan"]
```

### Target systems

**Single target (backward-compatible):**
```toml
[targets.postgres]
host          = "localhost"
database      = "mydb"
username      = "admin"
password_path = "myapp/db-admin"    # secret path for admin creds
ssl_mode      = "prefer"

[targets.api]
base_url       = "https://api.example.com"
endpoint       = "/users/{username}/password"
method         = "POST"
auth_header    = "Bearer token"
```

**Multiple targets (new array form):**
```toml
[[targets]]
type          = "postgres"
host          = "primary.db.internal"
database      = "app"
username      = "admin"
password_path = "myapp/db-admin"

[[targets]]
type          = "postgres"
host          = "replica.db.internal"
database      = "app"
username      = "admin"
password_path = "myapp/db-admin"

[[targets]]
type          = "mysql"    # requires --features mysql
host          = "mysql.internal"
database      = "app"
username      = "admin"
password_path = "myapp/mysql-admin"
```

### Key environment variables

| Variable | Description |
|----------|-------------|
| `SECRET_BACKEND` | Backend name |
| `VAULT_ADDR` / `VAULT_TOKEN` / `VAULT_MOUNT` | Vault |
| `AWS_REGION` | AWS |
| `AZURE_VAULT_URL` | Azure Key Vault |
| `GCP_PROJECT_ID` | GCP |
| `OCP_NAMESPACE` | Kubernetes namespace |
| `ROTATION_PERIOD_MONTHS` | Default rotation period |
| `SECRET_LENGTH` | Password length (default: 32) |
| `ASR_AUDIT_LOG` | JSONL audit log path |
| `ASR_AUDIT_STDOUT` | Write audit events to stdout (`1`/`true`) |
| `ASR_WEBHOOK_URL` | Webhook URL for notifications |

## Commands

```
asr init                        Create a sample config file
asr flag <path> [--period N]    Flag a secret for rotation
asr scan [path]                 List secrets due for rotation
asr rotate <path>               Rotate one secret
  --update-target               Also update configured target systems
  --target-username <USER>      Username to update in target
asr auto [path]                 Rotate all due secrets
  --dry-run                     Preview without making changes
  --update-env                  Sync new values to shell config files
  --update-target               Update target systems
asr read <path>                 Display a secret (WARNING: shows values)
asr list [path]                 List secrets at a path
asr gen-password <path>         Generate and store a random password
asr update-env <path>           Sync a secret to a shell env var
asr update-keychain <path>      Store in macOS Keychain (macOS + --features keychain)
```

## CI/CD (GitHub Actions)

```yaml
name: Weekly Secret Rotation
on:
  schedule:
    - cron: '0 0 * * 0'
  workflow_dispatch:

jobs:
  rotate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install asr
        run: curl -fsSL https://raw.githubusercontent.com/kelleyblackmore/Automatic-Secret-Rotation/main/install.sh | bash
      - name: Rotate secrets
        env:
          SECRET_BACKEND: vault
          VAULT_ADDR:     ${{ secrets.VAULT_ADDR }}
          VAULT_TOKEN:    ${{ secrets.VAULT_TOKEN }}
        run: |
          asr auto --dry-run
          asr auto
```

## Vault setup

```bash
vault secrets enable -version=2 -path=secret kv

cat > rotator-policy.hcl <<EOF
path "secret/data/*"     { capabilities = ["create","read","update"] }
path "secret/metadata/*" { capabilities = ["create","read","update","list"] }
EOF
vault policy write rotator rotator-policy.hcl
vault token create -policy=rotator
```

## AWS IAM permissions

```json
{
  "Effect": "Allow",
  "Action": [
    "secretsmanager:GetSecretValue", "secretsmanager:PutSecretValue",
    "secretsmanager:CreateSecret",   "secretsmanager:UpdateSecret",
    "secretsmanager:DescribeSecret", "secretsmanager:ListSecrets",
    "secretsmanager:TagResource"
  ],
  "Resource": "*"
}
```

## OCP / Kubernetes RBAC

See [`examples/ocp-rbac.yaml`](examples/ocp-rbac.yaml) for a complete `Role`, `RoleBinding`, and weekly CronJob manifest.

## Security notes

- Store backend credentials in CI secrets or Vault — never commit them
- Use HTTPS/TLS for backend connections in production
- `rotate` and `read` commands display secret values — use only in secure environments and clear terminal history afterwards
- `auto` never displays secret values

## Development

```bash
cargo build            # Debug build
cargo test             # Unit tests

# Integration tests (requires Docker Compose)
docker compose -f tests/integration/docker-compose.yml up -d
cargo test --features integration -- --ignored
docker compose -f tests/integration/docker-compose.yml down

cargo build --release  # Release build
```

## License

Apache License — see [LICENSE](LICENSE).
