.PHONY: help build install test clean fmt lint run check all

# Default target
help:
	@echo "Available targets:"
	@echo ""
	@echo "Build & Test:"
	@echo "  make build            - Build the project in debug mode"
	@echo "  make release          - Build the project in release mode"
	@echo "  make install          - Install the binary locally"
	@echo "  make test             - Run all tests"
	@echo "  make check            - Run cargo check"
	@echo "  make fmt              - Format code with rustfmt"
	@echo "  make lint             - Run clippy linter"
	@echo "  make clean            - Clean build artifacts"
	@echo "  make all              - Format, lint, test, and build"
	@echo ""
	@echo "Running:"
	@echo "  make run              - Run the application"
	@echo "  make init-config      - Initialize a sample config file"
	@echo "  make run-example      - Run with example config (needs Vault)"
	@echo "  make demo             - Quick demo with Vault env vars"
	@echo ""
	@echo "Vault Development:"
	@echo "  make vault-docker           - Start Vault in Docker (token: root)"
	@echo "  make vault-docker-stop      - Stop Vault Docker container"
	@echo "  make vault-create-test-secrets - Create test secrets in Vault"
	@echo "  make vault-flag-test-secrets   - Flag test secrets for rotation"
	@echo "  make vault-full-setup       - Complete Vault setup with test data"
	@echo "  make dev-with-vault         - Run scan with temporary Vault"
	@echo "  make install-vault          - Install Vault CLI"
	@echo "  make vault-dev              - Start Vault dev server (CLI)"
	@echo "  make vault-setup            - Show Vault environment setup commands"

# Build in debug mode
build:
	cargo build

# Build in release mode
release:
	cargo build --release

# Install the binary locally
install:
	cargo install --path .

# Run all tests
test:
	cargo test

# Run cargo check
check:
	cargo check

# Format code
fmt:
	cargo fmt

# Run clippy
lint:
	cargo clippy -- -D warnings

# Clean build artifacts
clean:
	cargo clean

# Run the application
run:
	cargo run

# Run in development mode with arguments
run-scan:
	cargo run -- scan

run-auto:
	cargo run -- auto

# Complete workflow: format, lint, test, and build
all: fmt lint test build

# Watch mode for development (requires cargo-watch)
watch:
	@command -v cargo-watch >/dev/null 2>&1 || { echo "cargo-watch not installed. Run: cargo install cargo-watch"; exit 1; }
	cargo watch -x check -x test -x run

# Install development dependencies
dev-deps:
	cargo install cargo-watch
	rustup component add rustfmt clippy

# Generate documentation
docs:
	cargo doc --no-deps --open

# Run with example config
run-example:
	@echo "Note: This requires Vault to be running. Use 'make vault-docker' in another terminal first."
	cargo run -- --config examples/config.toml scan

# Quick demo: run example with Vault environment variables (if Vault is running)
demo:
	@echo "Running demo (ensure Vault is running with 'make vault-docker')..."
	VAULT_ADDR='http://127.0.0.1:8200' VAULT_TOKEN='root' cargo run -- scan

# Initialize a config file
init-config:
	cargo run -- init

# Install Vault CLI if not present
install-vault:
	@if command -v vault >/dev/null 2>&1; then \
		echo "Vault CLI already installed: $$(vault version)"; \
	else \
		echo "Installing Vault CLI..."; \
		wget -O- https://apt.releases.hashicorp.com/gpg | gpg --dearmor | sudo tee /usr/share/keyrings/hashicorp-archive-keyring.gpg >/dev/null; \
		echo "deb [signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] https://apt.releases.hashicorp.com $$(lsb_release -cs) main" | sudo tee /etc/apt/sources.list.d/hashicorp.list; \
		sudo apt update && sudo apt install -y vault; \
		echo "Vault CLI installed: $$(vault version)"; \
	fi

# Development: start a local Vault server using Docker
vault-docker:
	@if ! command -v docker >/dev/null 2>&1; then \
		echo "Docker not installed. Please install Docker first."; \
		exit 1; \
	fi
	@echo "Starting Vault in Docker..."
	@echo "Root token: root"
	@echo "Vault address: http://127.0.0.1:8200"
	docker run --rm --name vault-dev \
		-p 8200:8200 \
		-e 'VAULT_DEV_ROOT_TOKEN_ID=root' \
		-e 'VAULT_DEV_LISTEN_ADDRESS=0.0.0.0:8200' \
		hashicorp/vault:latest

# Stop the Docker Vault container
vault-docker-stop:
	@docker stop vault-dev 2>/dev/null || echo "Vault container not running"

# Development: start a local Vault server using Vault CLI
vault-dev: install-vault
	vault server -dev -dev-root-token-id=root

# Development: setup Vault dev environment in another terminal
vault-setup:
	@echo "Run these commands in your terminal:"
	@echo "export VAULT_ADDR='http://127.0.0.1:8200'"
	@echo "export VAULT_TOKEN='root'"

# Create test secrets in Vault (requires Vault to be running)
vault-create-test-secrets:
	@echo "Creating test secrets in Vault..."
	@docker exec vault-dev vault kv put secret/database/postgres password=old_postgres_pass username=dbuser || \
		VAULT_ADDR='https://127.0.0.1:8200' VAULT_TOKEN='root' vault kv put secret/database/postgres password=old_postgres_pass username=dbuser
	@docker exec vault-dev vault kv put secret/api/github token=ghp_old_token_12345 || \
		VAULT_ADDR='https://127.0.0.1:8200' VAULT_TOKEN='root' vault kv put secret/api/github token=ghp_old_token_12345
	@docker exec vault-dev vault kv put secret/app/secret_key key=old_secret_key_value || \
		VAULT_ADDR='https://127.0.0.1:8200' VAULT_TOKEN='root' vault kv put secret/app/secret_key key=old_secret_key_value
	@echo "Test secrets created!"

# Flag test secrets for rotation
vault-flag-test-secrets:
	@echo "Flagging test secrets for rotation..."
	VAULT_ADDR='http://127.0.0.1:8200' VAULT_TOKEN='root' cargo run -- flag secret/database/postgres
	VAULT_ADDR='http://127.0.0.1:8200' VAULT_TOKEN='root' cargo run -- flag secret/api/github
	@echo "Test secrets flagged!"

# Complete setup: start Vault, create secrets, flag them
vault-full-setup:
	@echo "Starting Vault in background..."
	@docker run -d --rm --name vault-dev \
		-p 8200:8200 \
		-e 'VAULT_DEV_ROOT_TOKEN_ID=root' \
		-e 'VAULT_DEV_LISTEN_ADDRESS=0.0.0.0:8200' \
		hashicorp/vault:latest >/dev/null 2>&1 || echo "Vault already running"
	@echo "Waiting for Vault to be ready..."
	@sleep 3
	@$(MAKE) vault-create-test-secrets
	@$(MAKE) vault-flag-test-secrets
	@echo ""
	@echo "✓ Vault is ready with test secrets!"
	@echo "  Run: make demo"

# Development: run with Vault Docker container (starts vault, runs command, stops vault)
dev-with-vault:
	@echo "Starting Vault in background..."
	@docker run -d --rm --name vault-dev-tmp \
		-p 8200:8200 \
		-e 'VAULT_DEV_ROOT_TOKEN_ID=root' \
		-e 'VAULT_DEV_LISTEN_ADDRESS=0.0.0.0:8200' \
		hashicorp/vault:latest >/dev/null
	@echo "Waiting for Vault to be ready..."
	@sleep 3
	@echo "Running command with Vault..."
	@VAULT_ADDR='http://127.0.0.1:8200' VAULT_TOKEN='root' cargo run -- scan || true
	@echo "Stopping Vault..."
	@docker stop vault-dev-tmp >/dev/null
