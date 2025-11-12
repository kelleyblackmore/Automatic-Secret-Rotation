# Automatic Secret Rotation

A Rust-based CLI tool for automatic secret rotation with HashiCorp Vault integration. This tool enables automated management of secrets with configurable rotation periods, making it ideal for use in CI/CD pipelines and automation platforms like Jenkins, GitLab CI, and GitHub Actions.

## Features

- 🔐 **HashiCorp Vault Integration**: Seamlessly works with Vault KV v2 secrets engine
- 🔄 **Automatic Rotation**: Flag secrets for automatic rotation with customizable periods
- 📅 **Configurable Schedule**: Default 6-month rotation period, customizable per secret
- 🤖 **CI/CD Ready**: Designed for automation platforms (Jenkins, GitLab CI, GitHub Actions)
- 🔍 **Scanning**: Scan vault paths to identify secrets needing rotation
- 📝 **Metadata Tracking**: Uses Vault metadata to track rotation status and schedules
- 🎯 **Flexible Configuration**: Configure via file or environment variables

## Installation

### From Source

```bash
cargo install --path .
```

### Build Binary

```bash
cargo build --release
# Binary will be in target/release/secret-rotator
```

## Quick Start

### 1. Initialize Configuration

Create a sample configuration file:

```bash
secret-rotator init -o rotator-config.toml
```

Edit the configuration with your Vault details:

```toml
[vault]
address = "http://127.0.0.1:8200"
token = "your-vault-token-here"
mount = "secret"

[rotation]
period_months = 6
secret_length = 32
```

### 2. Flag a Secret for Rotation

Flag a secret to be rotated every 6 months:

```bash
secret-rotator flag my-app/database-password --period 6
```

### 3. Scan for Secrets Needing Rotation

Check which secrets are due for rotation:

```bash
secret-rotator scan
```

### 4. Rotate Secrets Automatically

Rotate all secrets that are due:

```bash
secret-rotator auto
```

Or perform a dry-run first:

```bash
secret-rotator auto --dry-run
```

## Usage

### Configuration

The tool can be configured in two ways:

#### 1. Configuration File

Create a TOML configuration file:

```toml
[vault]
address = "http://127.0.0.1:8200"
token = "hvs.your-vault-token"
mount = "secret"

[rotation]
period_months = 6
secret_length = 32
```

Use it with:

```bash
secret-rotator -c rotator-config.toml <command>
```

#### 2. Environment Variables

```bash
export VAULT_ADDR="http://127.0.0.1:8200"
export VAULT_TOKEN="hvs.your-vault-token"
export VAULT_MOUNT="secret"
export ROTATION_PERIOD_MONTHS=6
export SECRET_LENGTH=32

secret-rotator <command>
```

### Commands

#### `init` - Initialize Configuration

Create a sample configuration file:

```bash
secret-rotator init
secret-rotator init -o custom-config.toml
```

#### `flag` - Flag Secret for Rotation

Mark a secret for automatic rotation:

```bash
# Use default 6-month period
secret-rotator flag app/db-password

# Custom rotation period
secret-rotator flag app/api-key --period 3
```

#### `scan` - Scan for Secrets Needing Rotation

List all secrets that need rotation:

```bash
# Scan all secrets
secret-rotator scan

# Scan specific path
secret-rotator scan app/
```

#### `rotate` - Rotate a Specific Secret

Manually rotate a specific secret:

```bash
secret-rotator rotate app/db-password
```

#### `auto` - Automatic Rotation

Rotate all secrets that are due for rotation:

```bash
# Perform rotation
secret-rotator auto

# Dry run (show what would be rotated)
secret-rotator auto --dry-run

# Scan specific path
secret-rotator auto app/
```

#### `read` - Read a Secret

Read and display a secret:

```bash
secret-rotator read app/db-password
```

#### `list` - List Secrets

List all secrets at a path:

```bash
secret-rotator list
secret-rotator list app/
```

## CI/CD Integration

### GitHub Actions

```yaml
name: Rotate Secrets
on:
  schedule:
    - cron: '0 0 * * 0'  # Weekly on Sunday
  workflow_dispatch:

jobs:
  rotate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      
      - name: Install secret-rotator
        run: cargo install --path .
      
      - name: Rotate secrets
        env:
          VAULT_ADDR: ${{ secrets.VAULT_ADDR }}
          VAULT_TOKEN: ${{ secrets.VAULT_TOKEN }}
          VAULT_MOUNT: secret
        run: secret-rotator auto
```

### GitLab CI

```yaml
rotate-secrets:
  image: rust:latest
  script:
    - cargo install --path .
    - secret-rotator auto
  variables:
    VAULT_ADDR: $VAULT_ADDR
    VAULT_TOKEN: $VAULT_TOKEN
    VAULT_MOUNT: secret
  only:
    - schedules
```

### Jenkins

```groovy
pipeline {
    agent any
    
    triggers {
        cron('0 0 * * 0')  // Weekly on Sunday
    }
    
    environment {
        VAULT_ADDR = credentials('vault-addr')
        VAULT_TOKEN = credentials('vault-token')
        VAULT_MOUNT = 'secret'
    }
    
    stages {
        stage('Install') {
            steps {
                sh 'cargo install --path .'
            }
        }
        
        stage('Rotate Secrets') {
            steps {
                sh 'secret-rotator auto'
            }
        }
    }
}
```

## How It Works

### Metadata-Based Rotation

The tool uses Vault's custom metadata feature to track rotation status:

- `rotation_enabled`: Set to "true" for secrets that should be rotated
- `last_rotated`: RFC3339 timestamp of last rotation
- `rotation_period_months`: Custom rotation period (optional)

### Rotation Process

1. **Flagging**: When you flag a secret, metadata is added to track rotation
2. **Scanning**: The tool reads metadata to identify secrets needing rotation
3. **Rotation**: New random secrets are generated and written to Vault
4. **Tracking**: Metadata is updated with the new rotation timestamp

### Secret Generation

Secrets are generated using cryptographically secure random number generation with a character set including:
- Uppercase letters (A-Z)
- Lowercase letters (a-z)
- Numbers (0-9)
- Special characters (!@#$%^&*)

## Security Considerations

- **Vault Token**: Store Vault tokens securely using secret management in your CI/CD platform
- **TLS**: Use HTTPS for Vault communication in production
- **Permissions**: Ensure the Vault token has appropriate policies for reading, writing, and updating metadata
- **Audit**: Enable Vault audit logging to track all secret operations
- **Backup**: Ensure secrets are backed up before rotation
- **Terminal Output**: The `rotate` and `read` commands intentionally display secret values. Always:
  - Use these commands in secure environments only
  - Clear your terminal history after viewing secrets
  - Avoid logging command output that contains secrets
  - Use the `auto` command for automated rotation (doesn't display secrets)
  - Never redirect output containing secrets to files unless properly secured

## Vault Setup

### Enable KV v2 Secrets Engine

```bash
vault secrets enable -version=2 -path=secret kv
```

### Create a Policy

Create a policy file `rotator-policy.hcl`:

```hcl
path "secret/data/*" {
  capabilities = ["create", "read", "update"]
}

path "secret/metadata/*" {
  capabilities = ["create", "read", "update", "list"]
}
```

Apply the policy:

```bash
vault policy write rotator rotator-policy.hcl
vault token create -policy=rotator
```

## Development

### Build

```bash
cargo build
```

### Test

```bash
cargo test
```

### Run

```bash
cargo run -- --help
```

## Troubleshooting

### "Failed to connect to Vault"

- Verify `VAULT_ADDR` is correct
- Ensure Vault is running and accessible
- Check network connectivity

### "Permission denied"

- Verify your Vault token has the necessary permissions
- Check the Vault policy allows read/write/metadata operations

### "Secret not found"

- Verify the mount path is correct (default: `secret`)
- Check the secret path exists in Vault
- Ensure you're using KV v2 (not v1)

## License

MIT License - see LICENSE file for details

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

