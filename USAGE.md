# Usage Guide

This guide provides step-by-step instructions for using the secret-rotator tool.

## Table of Contents

1. [Initial Setup](#initial-setup)
2. [Basic Workflow](#basic-workflow)
3. [Advanced Usage](#advanced-usage)
4. [CI/CD Integration](#cicd-integration)
5. [Common Scenarios](#common-scenarios)

## Initial Setup

### 1. Install the Tool

Build from source:

```bash
git clone https://github.com/kelleyblackmore/Automatic-Secret-Rotation.git
cd Automatic-Secret-Rotation
cargo build --release
sudo cp target/release/secret-rotator /usr/local/bin/
```

### 2. Set Up HashiCorp Vault

If you don't have Vault set up yet:

```bash
# Start Vault in dev mode (for testing only)
vault server -dev

# In another terminal, set environment variables
export VAULT_ADDR='http://127.0.0.1:8200'
export VAULT_TOKEN='your-dev-token'

# Enable KV v2 secrets engine
vault secrets enable -version=2 -path=secret kv
```

### 3. Create Configuration

Generate a sample configuration:

```bash
secret-rotator init
```

Edit `rotator-config.toml` with your Vault details.

### 4. Create Initial Secrets

Create some test secrets in Vault:

```bash
# Using vault CLI
vault kv put secret/app/database password=initial-password-123

# Or using secret-rotator
# (Note: write command would need to be added - for now use vault CLI)
```

## Basic Workflow

### Step 1: Flag Secrets for Rotation

Mark which secrets should be automatically rotated:

```bash
# Flag with default 6-month period
secret-rotator flag app/database

# Flag with custom 3-month period
secret-rotator flag app/api-key --period 3

# Flag multiple secrets
secret-rotator flag app/smtp-password --period 6
secret-rotator flag app/oauth-secret --period 12
```

### Step 2: Check Rotation Status

See which secrets need rotation:

```bash
secret-rotator scan
```

Output:
```
Secrets needing rotation:
  - app/database
  - app/api-key
```

### Step 3: Rotate Secrets

#### Manual Rotation

Rotate a specific secret:

```bash
secret-rotator rotate app/database
```

Output:
```
Successfully rotated secret at: app/database
New secret value: aB3$xY9*...
```

#### Automatic Rotation

Rotate all secrets that are due:

```bash
# Dry run first
secret-rotator auto --dry-run

# Perform rotation
secret-rotator auto
```

## Advanced Usage

### Using Configuration Files

Create different configs for different environments:

```bash
# Development
secret-rotator -c config-dev.toml scan

# Production
secret-rotator -c config-prod.toml auto
```

### Using Environment Variables

Skip the config file entirely:

```bash
export VAULT_ADDR="https://vault.company.com:8200"
export VAULT_TOKEN="hvs.your-token"
export VAULT_MOUNT="secret"

secret-rotator scan
```

### Override Config with CLI

Override specific values:

```bash
secret-rotator \
  --vault-addr https://vault.company.com:8200 \
  --vault-token hvs.token \
  --vault-mount secret \
  scan
```

### Reading Secrets

View secret contents:

```bash
secret-rotator read app/database
```

Output:
```
Secret data:
  password: current-password-value
```

### Listing Secrets

List all secrets in a path:

```bash
# List root
secret-rotator list

# List specific path
secret-rotator list app/
```

## CI/CD Integration

### GitHub Actions

1. Add secrets to your GitHub repository:
   - `VAULT_ADDR`: Your Vault server URL
   - `VAULT_TOKEN`: Your Vault token

2. Create `.github/workflows/rotate-secrets.yml`:

```yaml
name: Rotate Secrets
on:
  schedule:
    - cron: '0 0 * * 0'  # Weekly

jobs:
  rotate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Install tool
        run: cargo install --path .
      - name: Rotate
        env:
          VAULT_ADDR: ${{ secrets.VAULT_ADDR }}
          VAULT_TOKEN: ${{ secrets.VAULT_TOKEN }}
        run: secret-rotator auto
```

### GitLab CI

Add to `.gitlab-ci.yml`:

```yaml
rotate-secrets:
  stage: maintenance
  image: rust:latest
  only:
    - schedules
  before_script:
    - cargo install --path .
  script:
    - secret-rotator auto
  variables:
    VAULT_ADDR: $VAULT_ADDR
    VAULT_TOKEN: $VAULT_TOKEN
```

### Jenkins

Create a Jenkins Pipeline job with this Jenkinsfile:

```groovy
pipeline {
    agent any
    triggers {
        cron('0 0 * * 0')
    }
    environment {
        VAULT_ADDR = credentials('vault-addr')
        VAULT_TOKEN = credentials('vault-token')
    }
    stages {
        stage('Rotate') {
            steps {
                sh 'cargo install --path .'
                sh 'secret-rotator auto'
            }
        }
    }
}
```

## Common Scenarios

### Scenario 1: First-Time Setup

You have existing secrets and want to start rotating them:

```bash
# 1. Flag all your secrets
secret-rotator flag app/db-password --period 6
secret-rotator flag app/api-key --period 3
secret-rotator flag app/jwt-secret --period 6

# 2. Verify they're flagged
secret-rotator scan

# 3. Initial rotation (if needed)
secret-rotator rotate app/db-password

# 4. Set up automation (GitHub Actions, etc.)
```

### Scenario 2: Regular Maintenance

Weekly automated rotation check:

```bash
#!/bin/bash
# Weekly cron job

# Check what needs rotation
echo "Checking for secrets needing rotation..."
secret-rotator scan

# Rotate if needed
echo "Performing automatic rotation..."
secret-rotator auto
```

### Scenario 3: Emergency Rotation

A secret was compromised and needs immediate rotation:

```bash
# Rotate immediately
secret-rotator rotate app/compromised-secret

# Update the secret in your application
# (Application-specific steps)

# Verify rotation
secret-rotator read app/compromised-secret
```

### Scenario 4: Bulk Flagging

Flag all secrets in a namespace:

```bash
# List all secrets
secret-rotator list app/

# Flag them (you'll need to do this for each)
for secret in $(vault kv list -format=json secret/metadata/app/ | jq -r '.[]'); do
  secret-rotator flag "app/${secret}" --period 6
done
```

### Scenario 5: Different Rotation Periods

Different types of secrets need different rotation periods:

```bash
# Critical secrets: 1 month
secret-rotator flag critical/root-password --period 1
secret-rotator flag critical/master-key --period 1

# Normal secrets: 6 months
secret-rotator flag app/db-password --period 6
secret-rotator flag app/api-key --period 6

# Low-priority secrets: 12 months
secret-rotator flag dev/test-token --period 12
```

### Scenario 6: Testing Before Production

Test rotation with dry-run:

```bash
# See what would be rotated
secret-rotator auto --dry-run

# If looks good, proceed
secret-rotator auto
```

## Troubleshooting

### Issue: "Failed to connect to Vault"

**Solution:**
```bash
# Check Vault is running
vault status

# Verify VAULT_ADDR
echo $VAULT_ADDR

# Test connection
curl $VAULT_ADDR/v1/sys/health
```

### Issue: "Permission denied"

**Solution:**
```bash
# Check your token capabilities
vault token capabilities secret/data/app/database

# Should show: ["create", "read", "update"]
```

### Issue: "Secret not found"

**Solution:**
```bash
# List secrets to find the correct path
secret-rotator list

# Or use vault CLI
vault kv list secret/
```

### Issue: "Metadata update failed"

**Solution:**
```bash
# Ensure you're using KV v2 (not v1)
vault secrets list

# Should show version 2 for your mount
```

## Best Practices

1. **Start with Dry Runs**: Always test with `--dry-run` first
2. **Monitor Logs**: Enable verbose logging with `RUST_LOG=debug`
3. **Backup First**: Back up secrets before rotation
4. **Test in Dev**: Test rotation in development environment first
5. **Gradual Rollout**: Start with non-critical secrets
6. **Document Secret Usage**: Know which applications use which secrets
7. **Coordinate Rotations**: Plan rotations during maintenance windows
8. **Use Appropriate Periods**: Balance security with operational overhead

## Next Steps

- Set up monitoring for rotation failures
- Create alerting for secrets approaching rotation
- Integrate with secret distribution to applications
- Implement automated testing of rotated secrets
