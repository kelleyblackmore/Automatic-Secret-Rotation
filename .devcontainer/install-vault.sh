#!/bin/bash

# Install HashiCorp Vault
set -e

echo "Installing HashiCorp Vault..."

# Add HashiCorp GPG key
curl -fsSL https://apt.releases.hashicorp.com/gpg | gpg --dearmor | sudo tee /usr/share/keyrings/hashicorp-archive-keyring.gpg > /dev/null

# Add HashiCorp repository
echo "deb [signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] https://apt.releases.hashicorp.com $(lsb_release -cs) main" | sudo tee /etc/apt/sources.list.d/hashicorp.list

# Update package index
sudo apt update

# Install Vault
sudo apt install -y vault

# Verify installation
vault version

echo "Vault installation completed successfully!"