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
- 🔑 **Password Generation**: Generate secure random passwords and store them in Vault
- 💻 **Environment Variable Sync**: Automatically update local shell config files with rotated secrets
- ⚡ **Auto-Update Workflow**: Rotate secrets and update environment variables in one command

## Installation

### Prerequisites

- Rust 1.70+ (will be auto-installed by the installer script)
- HashiCorp Vault server (or use Docker for local development)

### Quick Install (Recommended)

Install with a single command:

```bash
curl -fsSL https://raw.githubusercontent.com/kelleyblackmore/Automatic-Secret-Rotation/main/install.sh | bash
```

This will:
1. Install Rust (if not already installed)
2. Clone the repository
3. Build and install `asr` to `~/.local/bin/asr`
4. Verify the installation

After installation, you may need to add `~/.local/bin` to your PATH:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### From Source

```bash
git clone https://github.com/kelleyblackmore/Automatic-Secret-Rotation.git
cd Automatic-Secret-Rotation
make install
```

### Build Binary

```bash
make build        # Debug build
make release      # Release build
# Binary will be in target/release/asr
```

### Using Cargo

```bash
cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation
```

### Local Development

The project includes a Makefile with helpful commands:

```bash
make help              # Show all available commands
make vault-docker      # Start Vault in Docker for testing
make vault-full-setup  # Start Vault with test secrets
make test              # Run tests
make all               # Format, lint, test, and build
```

## Quick Start

### 1. Initialize Configuration

Create a sample configuration file:

```bash
asr init -o rotator-config.toml
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
asr flag my-app/database-password --period 6
```

### 3. Scan for Secrets Needing Rotation

Check which secrets are due for rotation:

```bash
asr scan
```

### 4. Rotate Secrets Automatically

Rotate all secrets that are due:

```bash
asr auto
```

Or with automatic environment variable updates:

```bash
asr auto --update-env
```

Perform a dry-run first:

```bash
asr auto --dry-run
```

## Password Management

### Generate New Password

Generate a secure password, store it in Vault, and optionally update your local environment:

```bash
# Generate and store in Vault only
asr gen-password myapp/database

# Generate, store, AND update local environment variable
asr gen-password --env-var DB_PASSWORD myapp/database

# Custom length
asr gen-password --env-var API_KEY --length 48 myapp/api

# Custom key name in Vault (default is "password")
asr gen-password --env-var TOKEN --key token myapp/github
```

### Sync Vault Secret to Environment Variable

Update your local shell config with an existing Vault secret:

```bash
# Sync secret to environment variable
asr update-env --env-var DB_PASSWORD myapp/database

# Sync with custom key
asr update-env --env-var API_TOKEN --key token myapp/github
```

After updating environment variables:
```bash
source ~/.bashrc  # or ~/.zshrc
echo $DB_PASSWORD  # Verify it's set
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
asr -c rotator-config.toml <command>
```

#### 2. Environment Variables

```bash
export VAULT_ADDR="http://127.0.0.1:8200"
export VAULT_TOKEN="hvs.your-vault-token"
export VAULT_MOUNT="secret"
export ROTATION_PERIOD_MONTHS=6
export SECRET_LENGTH=32

asr <command>
```

### Commands

#### `init` - Initialize Configuration

Create a sample configuration file:

```bash
asr init
asr init -o custom-config.toml
```

#### `flag` - Flag Secret for Rotation

Mark a secret for automatic rotation:

```bash
# Use default 6-month period
asr flag app/db-password

# Custom rotation period
asr flag app/api-key --period 3
```

#### `scan` - Scan for Secrets Needing Rotation

List all secrets that need rotation:

```bash
# Scan all secrets
asr scan

# Scan specific path
asr scan app/
```

#### `rotate` - Rotate a Specific Secret

Manually rotate a specific secret:

```bash
asr rotate app/db-password
```

#### `auto` - Automatic Rotation

Rotate all secrets that are due for rotation:

```bash
# Perform rotation
asr auto

# Dry run (show what would be rotated)
asr auto --dry-run

# Scan specific path
asr auto app/

# Rotate and update environment variables
asr auto --update-env
```

When using `--update-env`, environment variables are automatically created based on the secret path:
- `myapp/database` → `MYAPP_DATABASE`
- `api/github` → `API_GITHUB`

#### `gen-password` - Generate New Password

Generate a secure random password and store it in Vault:

```bash
# Generate and store in Vault
asr gen-password myapp/database

# Generate and update local environment variable
asr gen-password --env-var DB_PASSWORD myapp/database

# Custom length (default: 32 characters)
asr gen-password --length 48 myapp/api-key

# Custom key name (default: "password")
asr gen-password --key token --env-var API_TOKEN myapp/github
```

#### `update-env` - Sync Vault Secret to Environment

Update local environment variables with secrets from Vault:

```bash
# Update environment variable from Vault
asr update-env --env-var DB_PASSWORD myapp/database

# Use custom key from secret
asr update-env --env-var API_TOKEN --key token myapp/github
```

This command updates your shell configuration files (`.bashrc`, `.bash_profile`, `.zshrc`, `.profile`) with the secret value. You'll need to reload your shell for changes to take effect:

```bash
source ~/.bashrc
```

#### `read` - Read a Secret

Read and display a secret:

```bash
asr read app/db-password
```

#### `list` - List Secrets

List all secrets at a path:

```bash
asr list
asr list app/
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
      
      - name: Install asr
        run: cargo install --path .
      
      - name: Rotate secrets
        env:
          VAULT_ADDR: ${{ secrets.VAULT_ADDR }}
          VAULT_TOKEN: ${{ secrets.VAULT_TOKEN }}
          VAULT_MOUNT: secret
        run: asr auto
```

### GitLab CI

```yaml
rotate-secrets:
  image: rust:latest
  script:
    - cargo install --path .
    - asr auto
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
                sh 'asr auto'
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
5. **Environment Sync** (optional): Local shell configs are updated with new values

### Secret Generation

Secrets are generated using cryptographically secure random number generation with a character set including:
- Uppercase letters (A-Z)
- Lowercase letters (a-z)
- Numbers (0-9)
- Special characters (!@#$%^&*)

### Environment Variable Management

The tool can automatically update your shell configuration files with rotated secrets:

1. **Shell Config Files**: Updates `.bashrc`, `.bash_profile`, `.zshrc`, and `.profile`
2. **Smart Updates**: If a variable already exists, it's updated in-place; otherwise, it's appended
3. **Comments**: Adds `# Auto-updated by secret rotator` for tracking
4. **Path Mapping**: Converts Vault paths to environment variable names (e.g., `myapp/database` → `MYAPP_DATABASE`)

This enables seamless integration of Vault-managed secrets with applications that read from environment variables.

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

Apache License - see LICENSE file for details

## Quick Reference

### Common Commands

```bash
# Generate password with env var update
asr gen-password --env-var DB_PASS myapp/db

# Sync Vault secret to environment
asr update-env --env-var API_KEY myapp/api

# Rotate all due secrets and update env vars
asr auto --update-env

# Flag secret for rotation
asr flag myapp/password --period 3

# Scan for secrets needing rotation
asr scan

# Dry run auto-rotation
asr auto --dry-run
```

### Makefile Commands

```bash
make install              # Install the binary
make vault-docker         # Start Vault in Docker
make vault-full-setup     # Start Vault with test data
make demo                 # Quick demo with Vault
make test                 # Run tests
make all                  # Format, lint, test, build
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

