---
name: secrets-management
description: Rules for handling secrets and credentials (API keys, tokens, passwords, private keys, connection strings, cloud credentials). Use whenever code, config, CI, containers or infrastructure needs a credential, when adding .env files, when setting up gitleaks or secret scanning, or when a secret may have been committed or leaked. Covers never committing secrets, scanning in pre-commit and CI, loading secrets at runtime, OIDC instead of long-lived keys, and the leaked-secret response.
---

# Secrets Management

## Hard rules

1. **No secrets in git.** Not in code, config, tests, docs, examples, commit
   messages, PR descriptions or issue comments, in any branch, ever.
2. **Scan on every commit and in CI.** gitleaks in pre-commit and in the
   pipeline. Never bypass it.
3. **Load at runtime** from the environment, a mounted file, or a secrets
   manager. Never bake into images or build artifacts.
4. **Prefer short-lived credentials.** OIDC / workload identity / IAM roles over
   long-lived keys. Least privilege, one credential per service and environment.
5. **A leaked secret is rotated immediately.** Deleting the commit is not enough.
6. **Never print or log a secret,** including in errors, debug output or CI logs.
   Don't paste real secrets into chats or AI prompts.

## 1. Keep them out of the repo

`.gitignore` (add whatever applies):

```gitignore
.env
.env.*
!.env.example
*.pem
*.key
*.p12
*.pfx
id_rsa*
*.tfvars
!*.example.tfvars
.npmrc
.pypirc
credentials.json
service-account*.json
```

- Commit a `.env.example` with every variable name and a fake value
  (`API_KEY=changeme`) so people know what to set.
- Test fixtures use obviously fake values (`sk_test_000…`, `AKIAIOSFODNN7EXAMPLE`);
  if a scanner flags them, allow-list that exact value, not the file.

## 2. Scanning

pre-commit (see `assets/pre-commit-snippet.yaml`):

```yaml
- repo: https://github.com/gitleaks/gitleaks
  rev: v8.30.1          # pre-commit autoupdate for the latest
  hooks:
    - id: gitleaks
```

CI scans the full history on every push and PR:

```bash
gitleaks git . --redact --exit-code 1        # whole history
gitleaks dir . --redact                      # working tree only
```

Turn on the platform's own protection too: GitHub secret scanning + push
protection, or GitLab secret detection + push rules.

**False positives:** add a `.gitleaksignore` entry with the finding's
fingerprint (from `gitleaks … --report-format json`) and a comment saying why.
Never disable a rule globally or skip the hook.

## 3. Loading secrets at runtime

| Where it runs | Use |
|---|---|
| Local dev | `.env` (git-ignored) loaded by the app or `direnv`; or `op run` / `doppler run` / `aws-vault exec` |
| CI | The platform's masked + protected secrets, scoped to the job/environment; OIDC for cloud access |
| Containers | Orchestrator secrets mounted as files (`/run/secrets/…`) or injected env; never `ENV`/`ARG` in the Dockerfile |
| Kubernetes | External Secrets Operator / CSI secrets store from a real secrets manager; not plain `Secret` YAML in git |
| Cloud | AWS Secrets Manager / SSM, GCP Secret Manager, Azure Key Vault, HashiCorp Vault |

In code: read once at startup, validate it's present, fail fast with a message
naming the variable (not the value).

```python
api_key = os.environ.get("PAYMENTS_API_KEY")
if not api_key:
    raise SystemExit("PAYMENTS_API_KEY is not set")
```

If a secret must live in git (GitOps), encrypt it: SOPS with age/KMS, or
Sealed Secrets. The decryption key is never in the repo.

## 4. Credentials for CI and cloud

- GitHub Actions → cloud: OIDC (`id-token: write` + the provider's login action
  with a role ARN / workload identity provider). No `AWS_SECRET_ACCESS_KEY`.
- GitLab → cloud: `id_tokens:` + role assumption.
- Scope roles to one repo/branch/environment via the OIDC subject claim.
- Personal access tokens: fine-grained, minimum scopes, expiry set; never a
  personal token for automation (use an app/bot identity).

## 5. If a secret leaks

Do these in order, and tell the user immediately:

1. **Revoke/rotate** the credential at the provider. This is the fix; everything
   else is cleanup.
2. **Check for use**: provider audit logs (CloudTrail, GitHub audit log, the
   API's access log) from the time of the leak.
3. **Replace** it everywhere it's used with the new value from the secrets manager.
4. **Remove from history** only if the repo owner agrees: `git filter-repo` or
   BFG, then force-push and have everyone re-clone. Public forks/caches may
   still have it, which is why step 1 comes first.
5. **Prevent recurrence**: add the scanner/hook that would have caught it.

Never "fix" a leak by just committing a deletion.

## Checklist

- [ ] No secrets in the diff (run `gitleaks dir . --redact`), including tests and docs
- [ ] `.gitignore` covers `.env`, keys and credential files; `.env.example` committed
- [ ] gitleaks in pre-commit and CI; platform push protection on
- [ ] Secrets loaded at runtime from env/files/secrets manager, validated at startup
- [ ] Cloud access from CI uses OIDC; credentials are least-privilege and scoped
- [ ] Nothing logs or echoes a secret
