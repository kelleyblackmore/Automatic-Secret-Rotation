# Automatic Secret Rotation

A Rust-based CLI tool for automatic secret rotation with support for HashiCorp Vault and AWS Secrets Manager. This tool enables automated management of secrets with configurable rotation periods, making it ideal for use in CI/CD pipelines and automation platforms like Jenkins, GitLab CI, and GitHub Actions.

## Features

- **Multiple Backend Support**: Works with HashiCorp Vault KV v2, AWS Secrets Manager, and local file storage
- **HashiCorp Vault Integration**: Seamlessly works with Vault KV v2 secrets engine
- **AWS Secrets Manager Integration**: Full support for AWS Secrets Manager with tag-based metadata and region configuration
- **File Backend**: Local file storage for testing and development (simple key:value format)
- **Automatic Rotation**: Flag secrets for automatic rotation with customizable periods
- **Configurable Schedule**: Default 6-month rotation period, customizable per secret
- **CI/CD Ready**: Designed for automation platforms (Jenkins, GitLab CI, GitHub Actions)
- **Scanning**: Scan vault paths to identify secrets needing rotation
- **Metadata Tracking**: Uses backend metadata to track rotation status and schedules
- **Flexible Configuration**: Configure via file or environment variables
- **Password Generation**: Generate secure random passwords and store them in backends
- **Environment Variable Sync**: Automatically update local shell config files with rotated secrets
- **Auto-Update Workflow**: Rotate secrets and update environment variables in one command
- **Target System**: Update passwords in target systems (databases, APIs) during rotation
- **PostgreSQL Integration**: Automatically update PostgreSQL database passwords when rotating secrets
- **API Target Support**: Update passwords via REST API calls with configurable endpoints and methods
- **Comprehensive Testing**: Full unit test suite with 38+ tests covering all major functionality
- **GitHub Actions CI/CD**: Automated testing and binary releases for multiple platforms

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
1. **Download pre-built binary** from GitHub releases (if available for your platform)
2. **Fall back to building from source** if no pre-built binary is available:
   - Install Rust (if not already installed)
   - Clone the repository
   - Build the binary
3. Install the binary to `~/.local/bin/`:
   - **macOS**: `secret-rotator` (default, to avoid conflict with system `asr`)
   - **Other platforms**: `asr` (default)
4. Verify the installation

**Note**: Pre-built binaries are available for:
- Linux (x86_64, ARM64)
- macOS (x86_64, ARM64/Apple Silicon)

After installation, you may need to add `~/.local/bin` to your PATH:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

**Note for macOS users**: macOS includes a system tool called `asr` (Apple Software Restore) at `/usr/sbin/asr`. The installer automatically handles this by:
1. **Defaulting to `secret-rotator` as the binary name** on macOS to avoid conflicts
2. Detecting macOS and informing you of the default
3. Allowing you to override with `ASR_BINARY_NAME=asr` if you prefer

**Installation options:**
```bash
# Use custom binary name
ASR_BINARY_NAME=my-custom-name ./install.sh

# Force build from source (skip binary download)
ASR_BUILD_FROM_SOURCE=1 ./install.sh

# Combine options
ASR_BINARY_NAME=rotator ASR_BUILD_FROM_SOURCE=1 ./install.sh
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

### Docker

Use the ASR tool in a container:

```bash
# Build from Dockerfile.example (or use the provided example)
docker build -f Dockerfile.example -t asr:latest .

# Run commands
docker run --rm asr:latest --help
docker run --rm -v $(pwd):/workspace asr:latest scan

# With environment variables
docker run --rm \
  -e VAULT_ADDR=http://vault:8200 \
  -e VAULT_TOKEN=your-token \
  -e SECRET_BACKEND=vault \
  asr:latest auto

# Build specific version
docker build -f Dockerfile.example --build-arg ASR_VERSION=v1.0.0 -t asr:v1.0.0 .
```

The Dockerfile will:
1. Try to download pre-built binaries from GitHub releases
2. Fall back to building from source if binaries aren't available
3. Support both x86_64 and ARM64 architectures

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

**For Vault:**
```toml
backend = "vault"

[vault]
address = "http://127.0.0.1:8200"
token = "hvs.your-vault-token"
mount = "secret"

[rotation]
period_months = 6
secret_length = 32
```

**For AWS Secrets Manager:**
```toml
backend = "aws"

[aws]
region = "us-east-1"

[rotation]
period_months = 6
secret_length = 32
```

**For File Backend (Local Storage):**
```toml
backend = "file"

[file]
directory = "~/.asr/secrets"  # Default: ~/.asr/secrets

[rotation]
period_months = 6
secret_length = 32
```

Use it with:

```bash
asr -c rotator-config.toml <command>
```

#### 2. Environment Variables

**For Vault:**
```bash
export SECRET_BACKEND="vault"
export VAULT_ADDR="http://127.0.0.1:8200"
export VAULT_TOKEN="hvs.your-vault-token"
export VAULT_MOUNT="secret"
export ROTATION_PERIOD_MONTHS=6
export SECRET_LENGTH=32

asr <command>
```

**For AWS Secrets Manager:**
```bash
export SECRET_BACKEND="aws"
export AWS_REGION="us-east-1"
export ROTATION_PERIOD_MONTHS=6
export SECRET_LENGTH=32

# AWS credentials are automatically detected from:
# - AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY environment variables
# - AWS credentials file (~/.aws/credentials)
# - IAM role (when running on EC2/ECS/Lambda)

asr <command>
```

**For File Backend:**
```bash
export SECRET_BACKEND="file"
export ASR_FILE_DIR="~/.asr/secrets"  # Optional, defaults to ~/.asr/secrets
export ROTATION_PERIOD_MONTHS=6
export SECRET_LENGTH=32

asr <command>
```

**File Format:**
Secrets are stored in plain text files with `key:value` format, one per line:
```
password:mysecret123
username:admin
```

Metadata is stored in a separate `.meta` file alongside each secret file.

#### Target Configuration (PostgreSQL, API)

Configure target systems where passwords should be updated during rotation:

**PostgreSQL Target:**
```toml
[targets.postgres]
host = "localhost"
port = 5432
database = "postgres"
username = "admin"
password_path = "admin/password"  # Path in backend for admin password
ssl_mode = "prefer"  # Options: disable, allow, prefer, require, verify-ca, verify-full
```

**API Target:**
```toml
[targets.api]
base_url = "https://api.example.com"
endpoint = "/users/{username}/password"  # {username} will be replaced
method = "POST"  # GET, POST, PUT, PATCH, DELETE
password_field = "password"
username_field = "username"  # Optional
timeout_seconds = 30
auth_header = "Bearer token123"  # Optional

[targets.api.headers]  # Optional additional headers
X-Custom-Header = "value"
```

**Legacy Database Config (deprecated, use `[targets.postgres]` instead):**
```toml
[database]
host = "localhost"
port = 5432
database = "postgres"
username = "admin"
password_path = "admin/password"
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
# Basic rotation
asr rotate app/db-password

# Rotate and update target password (PostgreSQL, API, etc.)
asr rotate app/db-password --update-target --target-username myapp_user
```

When using `--update-target`, the tool will:
1. Rotate the secret in the backend (Vault/AWS/File)
2. Update the password in the configured target system (PostgreSQL database or API)
3. Verify the new password works

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

# Rotate and update target passwords (databases, APIs)
asr auto --update-target

# Rotate, update env vars, and update targets
asr auto --update-env --update-target
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

## Use Case Examples

### Use Case 0: Testing with File Backend (Local Storage)

**Scenario**: You want to test secret rotation locally without setting up Vault or AWS.

```bash
# Set backend to file
export SECRET_BACKEND="file"
export ASR_FILE_DIR="./test-secrets"  # Use local directory for testing

# Generate a password
asr gen-password myapp/database --key password

# Flag for rotation
asr flag myapp/database --period 3

# Scan for secrets needing rotation
asr scan

# Rotate secrets
asr auto

# View the secret file
cat test-secrets/myapp/database
# Output:
# password:2ed1md...

# View metadata
cat test-secrets/myapp/database.meta
# Output:
# rotation_enabled:true
# last_rotated:2024-01-15T10:30:00Z
```

**File Structure:**
```
test-secrets/
├── myapp/
│   ├── database          # Secret file (key:value format)
│   └── database.meta    # Metadata file
└── api/
    ├── token
    └── token.meta
```

### Use Case 1: Setting Up a New Application Secret (Vault)

**Scenario**: You're deploying a new application and need to create a database password.

```bash
# 1. Generate and store a new password
asr gen-password myapp/production/database --key password --length 40

# 2. Flag it for automatic rotation every 3 months
asr flag myapp/production/database --period 3

# 3. Verify it was created and flagged
asr read myapp/production/database
asr scan myapp/production/
```

**With Environment Variable Sync:**
```bash
# Generate password and automatically update local environment
asr gen-password --env-var DB_PASSWORD myapp/production/database --key password

# Reload shell to use the new variable
source ~/.bashrc
echo $DB_PASSWORD
```

### Use Case 2: Setting Up a New Application Secret (AWS Secrets Manager)

**Scenario**: You're deploying a new application in AWS and need to create an API key.

```bash
# Set backend to AWS
export SECRET_BACKEND=aws
export AWS_REGION=us-east-1

# 1. Generate and store a new API key
asr gen-password myapp/production/api --key api_key --length 48

# 2. Flag it for automatic rotation every 6 months
asr flag myapp/production/api --period 6

# 3. Verify it was created and flagged
asr read myapp/production/api
asr scan myapp/production/
```

**Using Config File:**
```toml
# config-aws.toml
backend = "aws"

[aws]
region = "us-east-1"

[rotation]
period_months = 6
secret_length = 32
```

```bash
asr -c config-aws.toml gen-password myapp/production/api --key api_key
asr -c config-aws.toml flag myapp/production/api --period 6
```

### Use Case 3: Automated Rotation in CI/CD Pipeline

**Scenario**: Automatically rotate all secrets weekly in your CI/CD pipeline.

**GitHub Actions with Vault:**
```yaml
name: Weekly Secret Rotation
on:
  schedule:
    - cron: '0 0 * * 0'  # Every Sunday at midnight
  workflow_dispatch:

jobs:
  rotate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install asr
        run: cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation
      
      - name: Rotate Vault secrets
        env:
          SECRET_BACKEND: vault
          VAULT_ADDR: ${{ secrets.VAULT_ADDR }}
          VAULT_TOKEN: ${{ secrets.VAULT_TOKEN }}
          VAULT_MOUNT: secret
        run: |
          asr auto --dry-run  # Preview what will be rotated
          asr auto            # Perform rotation
```

**GitHub Actions with AWS Secrets Manager:**
```yaml
name: Weekly Secret Rotation
on:
  schedule:
    - cron: '0 0 * * 0'  # Every Sunday at midnight
  workflow_dispatch:

jobs:
  rotate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Configure AWS credentials
        uses: aws-actions/configure-aws-credentials@v2
        with:
          aws-access-key-id: ${{ secrets.AWS_ACCESS_KEY_ID }}
          aws-secret-access-key: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
          aws-region: us-east-1
      
      - name: Install asr
        run: cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation
      
      - name: Rotate AWS Secrets Manager secrets
        env:
          SECRET_BACKEND: aws
          AWS_REGION: us-east-1
        run: |
          asr auto --dry-run  # Preview what will be rotated
          asr auto            # Perform rotation
```

### Use Case 4: Multi-Environment Secret Management

**Scenario**: Managing secrets across development, staging, and production environments.

**Vault Example:**
```bash
# Development environment
export VAULT_ADDR="http://vault-dev.example.com:8200"
export VAULT_TOKEN="$DEV_VAULT_TOKEN"

# Create secrets for each environment
asr gen-password dev/database --key password
asr gen-password staging/database --key password
asr gen-password production/database --key password

# Flag production secrets for rotation (more frequent)
asr flag production/database --period 3
asr flag staging/database --period 6
asr flag dev/database --period 12  # Less frequent for dev

# Scan specific environment
asr scan production/
```

**AWS Secrets Manager Example:**
```bash
# Using different AWS profiles/regions for each environment
export SECRET_BACKEND=aws

# Development
export AWS_PROFILE=dev
export AWS_REGION=us-west-2
asr gen-password dev/database --key password
asr flag dev/database --period 12

# Staging
export AWS_PROFILE=staging
export AWS_REGION=us-east-1
asr gen-password staging/database --key password
asr flag staging/database --period 6

# Production
export AWS_PROFILE=production
export AWS_REGION=us-east-1
asr gen-password production/database --key password
asr flag production/database --period 3
```

### Use Case 5: Emergency Secret Rotation

**Scenario**: A secret has been compromised and needs immediate rotation.

**Vault:**
```bash
# 1. Immediately rotate the compromised secret
asr rotate production/api-key

# 2. Update the application with the new secret
# (The secret value is displayed - copy it securely)

# 3. Verify rotation timestamp was updated
asr scan production/api-key

# 4. If needed, update rotation period for more frequent rotations
asr flag production/api-key --period 1  # Rotate monthly going forward
```

**AWS Secrets Manager:**
```bash
export SECRET_BACKEND=aws
export AWS_REGION=us-east-1

# 1. Immediately rotate the compromised secret
asr rotate production/api-key

# 2. Update the application with the new secret
# (The secret value is displayed - copy it securely)

# 3. Verify rotation timestamp was updated
asr scan production/api-key

# 4. Update rotation period for more frequent rotations
asr flag production/api-key --period 1  # Rotate monthly going forward
```

### Use Case 6: Bulk Secret Operations

**Scenario**: Migrating multiple secrets or performing bulk operations.

**Vault:**
```bash
# List all secrets in a path
asr list production/

# Scan all secrets that need rotation
asr scan production/

# Rotate all secrets that are due (with dry-run first)
asr auto --dry-run production/
asr auto production/

# Rotate all secrets and update environment variables
asr auto --update-env production/
```

**AWS Secrets Manager:**
```bash
export SECRET_BACKEND=aws
export AWS_REGION=us-east-1

# List all secrets with a prefix
asr list production/

# Scan all secrets that need rotation
asr scan production/

# Rotate all secrets that are due (with dry-run first)
asr auto --dry-run production/
asr auto production/
```

### Use Case 7: Application Integration with Environment Variables

**Scenario**: Your application reads secrets from environment variables, and you want them automatically updated.

**Vault:**
```bash
# 1. Generate password and sync to environment variable
asr gen-password --env-var DB_PASSWORD myapp/database --key password

# 2. Flag for rotation
asr flag myapp/database --period 6

# 3. When rotation happens, automatically update env var
asr auto --update-env myapp/

# 4. Reload shell to pick up new values
source ~/.bashrc

# 5. Your application can now use the updated $DB_PASSWORD
```

**AWS Secrets Manager:**
```bash
export SECRET_BACKEND=aws
export AWS_REGION=us-east-1

# Same workflow works identically
asr gen-password --env-var DB_PASSWORD myapp/database --key password
asr flag myapp/database --period 6
asr auto --update-env myapp/
source ~/.bashrc
```

### Use Case 8: Monitoring and Auditing

**Scenario**: Regular monitoring of secret rotation status.

**Vault:**
```bash
# Check what secrets need rotation
asr scan

# Check specific application
asr scan myapp/

# Dry run to see what would be rotated
asr auto --dry-run

# View secret metadata (rotation status)
asr read myapp/database  # Shows last_rotated in metadata
```

**AWS Secrets Manager:**
```bash
export SECRET_BACKEND=aws
export AWS_REGION=us-east-1

# Check what secrets need rotation
asr scan

# Check specific application
asr scan myapp/

# Dry run to see what would be rotated
asr auto --dry-run

# View secret tags (rotation status)
aws secretsmanager describe-secret --secret-id myapp/database --query 'Tags'
```

### Use Case 9: Secret Migration Between Backends

**Scenario**: Migrating secrets from Vault to AWS Secrets Manager (or vice versa).

```bash
# 1. Read secret from Vault
export SECRET_BACKEND=vault
export VAULT_ADDR="http://vault.example.com:8200"
export VAULT_TOKEN="$VAULT_TOKEN"

SECRET_VALUE=$(asr read myapp/database | grep "password:" | awk '{print $2}')

# 2. Write to AWS Secrets Manager
export SECRET_BACKEND=aws
export AWS_REGION=us-east-1

# Create secret in AWS (as JSON)
echo "{\"password\": \"$SECRET_VALUE\"}" | \
  aws secretsmanager create-secret --name myapp/database --secret-string file:///dev/stdin

# 3. Flag for rotation in AWS
asr flag myapp/database --period 6
```

### Use Case 10: CI/CD Secret Injection

**Scenario**: Using rotated secrets in CI/CD pipelines without exposing them in logs.

**GitLab CI with Vault:**
```yaml
rotate-and-deploy:
  image: rust:latest
  before_script:
    - cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation
    - asr auto  # Rotate secrets if needed
  script:
    # Fetch latest secret (without displaying it)
    - export DB_PASSWORD=$(asr read myapp/database | grep "password:" | awk '{print $2}')
    - echo "Deploying with rotated secret..."
    # Use $DB_PASSWORD in your deployment
  variables:
    SECRET_BACKEND: vault
    VAULT_ADDR: $VAULT_ADDR
    VAULT_TOKEN: $VAULT_TOKEN
```

**GitLab CI with AWS Secrets Manager:**
```yaml
rotate-and-deploy:
  image: rust:latest
  before_script:
    - cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation
    - asr auto  # Rotate secrets if needed
  script:
    # Fetch latest secret (without displaying it)
    - export DB_PASSWORD=$(asr read myapp/database | grep "password:" | awk '{print $2}')
    - echo "Deploying with rotated secret..."
    # Use $DB_PASSWORD in your deployment
  variables:
    SECRET_BACKEND: aws
    AWS_REGION: us-east-1
```

## CI/CD Integration

### GitHub Actions - Vault

```yaml
name: Rotate Secrets (Vault)
on:
  schedule:
    - cron: '0 0 * * 0'  # Weekly on Sunday
  workflow_dispatch:

jobs:
  rotate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install asr
        run: cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation
      
      - name: Rotate secrets
        env:
          SECRET_BACKEND: vault
          VAULT_ADDR: ${{ secrets.VAULT_ADDR }}
          VAULT_TOKEN: ${{ secrets.VAULT_TOKEN }}
          VAULT_MOUNT: secret
        run: |
          asr auto --dry-run  # Preview changes
          asr auto            # Perform rotation
```

### GitHub Actions - AWS Secrets Manager

```yaml
name: Rotate Secrets (AWS)
on:
  schedule:
    - cron: '0 0 * * 0'  # Weekly on Sunday
  workflow_dispatch:

jobs:
  rotate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Configure AWS credentials
        uses: aws-actions/configure-aws-credentials@v2
        with:
          aws-access-key-id: ${{ secrets.AWS_ACCESS_KEY_ID }}
          aws-secret-access-key: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
          aws-region: us-east-1
      
      - name: Install asr
        run: cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation
      
      - name: Rotate secrets
        env:
          SECRET_BACKEND: aws
          AWS_REGION: us-east-1
        run: |
          asr auto --dry-run  # Preview changes
          asr auto            # Perform rotation
```

### GitLab CI - Vault

```yaml
rotate-secrets-vault:
  image: rust:latest
  script:
    - cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation
    - asr auto --dry-run
    - asr auto
  variables:
    SECRET_BACKEND: vault
    VAULT_ADDR: $VAULT_ADDR
    VAULT_TOKEN: $VAULT_TOKEN
    VAULT_MOUNT: secret
  only:
    - schedules
```

### GitLab CI - AWS Secrets Manager

```yaml
rotate-secrets-aws:
  image: rust:latest
  before_script:
    - apt-get update && apt-get install -y awscli
    - aws configure set aws_access_key_id $AWS_ACCESS_KEY_ID
    - aws configure set aws_secret_access_key $AWS_SECRET_ACCESS_KEY
    - aws configure set region us-east-1
  script:
    - cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation
    - asr auto --dry-run
    - asr auto
  variables:
    SECRET_BACKEND: aws
    AWS_REGION: us-east-1
  only:
    - schedules
```

### Jenkins - Vault

```groovy
pipeline {
    agent any
    
    triggers {
        cron('0 0 * * 0')  // Weekly on Sunday
    }
    
    environment {
        SECRET_BACKEND = 'vault'
        VAULT_ADDR = credentials('vault-addr')
        VAULT_TOKEN = credentials('vault-token')
        VAULT_MOUNT = 'secret'
    }
    
    stages {
        stage('Install') {
            steps {
                sh 'cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation'
            }
        }
        
        stage('Rotate Secrets') {
            steps {
                sh 'asr auto --dry-run'
                sh 'asr auto'
            }
        }
    }
}
```

### Jenkins - AWS Secrets Manager

```groovy
pipeline {
    agent any
    
    triggers {
        cron('0 0 * * 0')  // Weekly on Sunday
    }
    
    environment {
        SECRET_BACKEND = 'aws'
        AWS_REGION = 'us-east-1'
        AWS_ACCESS_KEY_ID = credentials('aws-access-key-id')
        AWS_SECRET_ACCESS_KEY = credentials('aws-secret-access-key')
    }
    
    stages {
        stage('Install') {
            steps {
                sh 'cargo install --git https://github.com/kelleyblackmore/Automatic-Secret-Rotation'
            }
        }
        
        stage('Rotate Secrets') {
            steps {
                sh 'asr auto --dry-run'
                sh 'asr auto'
            }
        }
    }
}
```

## How It Works

### Metadata-Based Rotation

The tool uses backend-specific metadata to track rotation status:

**For HashiCorp Vault:**
- Uses Vault's custom metadata feature
- `rotation_enabled`: Set to "true" for secrets that should be rotated
- `last_rotated`: RFC3339 timestamp of last rotation
- `rotation_period_months`: Custom rotation period (optional)

**For AWS Secrets Manager:**
- Uses AWS Secrets Manager tags
- `rotation_enabled`: Tag set to "true" for secrets that should be rotated
- `last_rotated`: Tag with RFC3339 timestamp of last rotation
- `rotation_period_months`: Tag with custom rotation period (optional)

**For File Backend:**
- Uses separate `.meta` files alongside secret files
- `rotation_enabled`: Set to "true" for secrets that should be rotated
- `last_rotated`: RFC3339 timestamp of last rotation
- `rotation_period_months`: Custom rotation period (optional)

### Rotation Process

1. **Flagging**: When you flag a secret, metadata/tags are added to track rotation
2. **Scanning**: The tool reads metadata/tags to identify secrets needing rotation
3. **Rotation**: New random secrets are generated and written to the backend
4. **Tracking**: Metadata/tags are updated with the new rotation timestamp
5. **Environment Sync** (optional): Local shell configs are updated with new values

**Backend Differences:**
- **Vault**: Secrets stored as key-value pairs, metadata stored in Vault's metadata system
- **AWS Secrets Manager**: Secrets stored as JSON strings, metadata stored as tags

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
4. **Path Mapping**: Converts secret paths to environment variable names (e.g., `myapp/database` → `MYAPP_DATABASE`)

This enables seamless integration of backend-managed secrets with applications that read from environment variables. Works identically for both Vault and AWS Secrets Manager.

## Security Considerations

### General Security Best Practices

- **Credentials**: Store backend credentials securely using secret management in your CI/CD platform
- **TLS**: Use HTTPS/TLS for all backend communication in production
- **Permissions**: Ensure credentials have minimal required permissions (principle of least privilege)
- **Audit**: Enable audit logging in your backend to track all secret operations
- **Backup**: Ensure secrets are backed up before rotation
- **Terminal Output**: The `rotate` and `read` commands intentionally display secret values. Always:
  - Use these commands in secure environments only
  - Clear your terminal history after viewing secrets
  - Avoid logging command output that contains secrets
  - Use the `auto` command for automated rotation (doesn't display secrets)
  - Never redirect output containing secrets to files unless properly secured

### Vault-Specific Security

- **Vault Token**: Store Vault tokens securely using secret management in your CI/CD platform
- **TLS**: Use HTTPS for Vault communication in production
- **Permissions**: Ensure the Vault token has appropriate policies for reading, writing, and updating metadata
- **Audit**: Enable Vault audit logging to track all secret operations

### AWS Secrets Manager-Specific Security

- **IAM Roles**: Prefer IAM roles over access keys when running on EC2/ECS/Lambda
- **Access Keys**: If using access keys, rotate them regularly and store them securely
- **IAM Policies**: Use least-privilege IAM policies (see setup section for example)
- **Encryption**: AWS Secrets Manager automatically encrypts secrets at rest using KMS
- **CloudTrail**: Enable CloudTrail to audit all Secrets Manager API calls
- **Resource Policies**: Use resource-based policies to restrict access to specific secrets

## Backend Setup

### HashiCorp Vault Setup

#### Enable KV v2 Secrets Engine

```bash
vault secrets enable -version=2 -path=secret kv
```

#### Create a Policy

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

### AWS Secrets Manager Setup

AWS Secrets Manager requires AWS credentials with appropriate permissions. The tool uses AWS SDK's default credential chain, which checks:

1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
2. AWS credentials file (`~/.aws/credentials`)
3. IAM roles (when running on EC2/ECS/Lambda)

#### Required IAM Permissions

Create an IAM policy with the following permissions:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "secretsmanager:GetSecretValue",
        "secretsmanager:PutSecretValue",
        "secretsmanager:CreateSecret",
        "secretsmanager:UpdateSecret",
        "secretsmanager:DescribeSecret",
        "secretsmanager:ListSecrets",
        "secretsmanager:TagResource"
      ],
      "Resource": "*"
    }
  ]
}
```

#### Notes

- Secrets in AWS Secrets Manager are stored as JSON strings
- Metadata is stored as tags (e.g., `rotation_enabled`, `last_rotated`, `rotation_period_months`)
- Secret names can include forward slashes (e.g., `myapp/database/password`)

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

### macOS-Specific Issues

**"Command not found" or "Wrong command executing"**

On macOS, the installer defaults to installing the binary as `secret-rotator` to avoid conflict with the system `asr` tool. 

1. **Check which binary was installed:**
   ```bash
   ls ~/.local/bin/ | grep -E "(asr|secret-rotator)"
   ```

2. **Use the correct command name:**
   ```bash
   # Default on macOS:
   secret-rotator --help
   
   # If you installed with ASR_BINARY_NAME=asr:
   asr --help
   ```

3. **If you want to use 'asr' on macOS:**
   ```bash
   # Reinstall with custom name:
   ASR_BINARY_NAME=asr ./install.sh
   
   # Then ensure ~/.local/bin comes before /usr/sbin in PATH:
   export PATH="$HOME/.local/bin:$PATH"
   ```

4. **Verify PATH configuration:**
   ```bash
   which secret-rotator  # Should show: ~/.local/bin/secret-rotator
   echo $PATH | grep -o "[^:]*" | grep -n "local"
   # ~/.local/bin should appear before /usr/sbin
   ```

### General Issues

**"Backend configuration not found"**
- Verify `SECRET_BACKEND` is set to either "vault" or "aws"
- Check that the appropriate backend configuration section exists in your config file
- Ensure required environment variables are set

**"Failed to authenticate"**
- Verify credentials are correct and not expired
- Check that credentials have the necessary permissions
- For AWS, verify the credential chain is working (`aws sts get-caller-identity`)

### Vault-Specific Issues

**"Failed to connect to Vault"**
- Verify `VAULT_ADDR` is correct
- Ensure Vault is running and accessible
- Check network connectivity
- Verify TLS certificates if using HTTPS

**"Permission denied"**
- Verify your Vault token has the necessary permissions
- Check the Vault policy allows read/write/metadata operations
- Ensure token hasn't expired

**"Secret not found"**
- Verify the mount path is correct (default: `secret`)
- Check the secret path exists in Vault
- Ensure you're using KV v2 (not v1)

### AWS Secrets Manager-Specific Issues

**"Failed to create AWS Secrets Manager client"**
- Verify AWS credentials are configured correctly
- Check `AWS_REGION` is set or configured in AWS config
- Test credentials: `aws sts get-caller-identity`

**"AccessDeniedException"**
- Verify IAM user/role has required Secrets Manager permissions
- Check resource-based policies if using them
- Ensure you're using the correct AWS account/region

**"ResourceNotFoundException"**
- Verify the secret name is correct (case-sensitive)
- Check that you're querying the correct AWS region
- List secrets: `aws secretsmanager list-secrets`

**"InvalidParameterException"**
- AWS Secrets Manager stores secrets as JSON strings
- Ensure secret data is valid JSON when writing
- Check secret name format (can include forward slashes)

**"Secrets not appearing in list"**
- AWS Secrets Manager paginates results - the tool handles this automatically
- Verify you're querying the correct region
- Check IAM permissions include `secretsmanager:ListSecrets`

## License

Apache License - see LICENSE file for details

## Quick Reference

### Common Commands

**Vault Examples:**
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

**AWS Secrets Manager Examples:**
```bash
# Set backend
export SECRET_BACKEND=aws
export AWS_REGION=us-east-1

# Generate password
asr gen-password --env-var DB_PASS myapp/db

# Sync AWS secret to environment
asr update-env --env-var API_KEY myapp/api

# Rotate all due secrets
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

