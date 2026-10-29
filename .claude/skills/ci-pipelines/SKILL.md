---
name: ci-pipelines
description: Standard layout and security rules for CI/CD pipelines on GitHub Actions and GitLab CI. Use whenever creating or editing .github/workflows/*.yml, .gitlab-ci.yml, or other pipeline config, adding a CI job, or debugging a pipeline. Covers required stages (pre-commit, tests, security scans), pinning actions and images, least-privilege tokens, secrets handling, and linting workflows with actionlint and zizmor.
---

# CI Pipelines

## Hard rules

1. **Every repo runs the same core stages**: pre-commit → tests → security
   scans → build → (deploy). A merge request can't merge while any is red.
2. **Pin everything third-party** to an immutable reference: GitHub Actions
   by full commit SHA with the version in a comment, container images by
   digest. Use the latest release when pinning.
3. **Least privilege.** Default token permissions are read-only; each job
   asks for only what it needs.
4. **Secrets stay secret.** Never echo them, never pass them to untrusted
   code (fork PRs), prefer OIDC over long-lived cloud keys.
5. **Workflows are linted**: `actionlint` and `zizmor` for GitHub Actions,
   `glab ci lint` for GitLab.
6. **CI matches local.** Jobs call the same commands a developer runs
   (`pre-commit run --all-files`, `make test`), not a separate CI-only script.

## 1. Stage layout

| Stage | What runs | Fails on |
|---|---|---|
| lint | `pre-commit run --all-files` (formatters, linters, hook checks) | any hook failure |
| test | unit + integration tests on the latest language version | any failure |
| security | dependency audit (pip-audit / npm audit), SAST (bandit, semgrep), secret scan (gitleaks), IaC scan (checkov / trivy config) | HIGH/CRITICAL or any secret |
| build | build artifact / image, `trivy image` | HIGH/CRITICAL |
| deploy | only from protected branches, after all above pass | — |

Run lint, test and security in parallel; build depends on all three.
Templates: `assets/github-actions.example.yml`, `assets/gitlab-ci.example.yml`.

## 2. GitHub Actions specifics

**Pin by SHA, latest release:**

```yaml
- uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
```

Find the SHA for the latest tag:

```bash
tag=$(git ls-remote --tags https://github.com/actions/checkout \
  | grep -oE 'v[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -1)
git ls-remote https://github.com/actions/checkout "refs/tags/$tag^{}" "refs/tags/$tag"
```

(Use the `^{}` line if present: that's the commit behind an annotated tag.)
Let Dependabot or Renovate keep SHAs current (`assets/dependabot.example.yml`).

Dependabot does not update container images referenced inside `run:` steps
(e.g. `docker run image@sha256:…`) or GitLab `image:` keys. Pin them by
digest anyway, then either let Renovate update them (its `regex` manager can
match `image:tag@sha256:digest`) or re-pin them by hand whenever the
pipeline's tools are upgraded:

```bash
docker buildx imagetools inspect ghcr.io/gitleaks/gitleaks:v8.30.1   # prints the digest
```

Prefer `docker run` of a pinned image over a vendor action that needs a
license key or write permissions just to run a scanner.

**Permissions** at the top of every workflow, widened per job only as needed:

```yaml
permissions:
  contents: read
```

**Untrusted input:**

- Never interpolate `${{ github.event.* }}` (titles, branch names, bodies)
  directly into `run:`. Pass through `env:` and quote: `"$PR_TITLE"`.
- Never use `pull_request_target` with a checkout of the PR's code.
- `actions/checkout` with `persist-credentials: false` unless the job pushes.
- Don't run fork PRs on self-hosted runners.

**Other:**

- `concurrency:` group per ref with `cancel-in-progress: true` for PR runs.
- `timeout-minutes:` on every job.
- Cloud access via OIDC (`id-token: write` + the provider's login action),
  not stored access keys.
- Deploy jobs use a GitHub `environment:` with protection rules.

Lint:

```bash
actionlint
zizmor .github/workflows/
```

## 3. GitLab CI specifics

- `image:` pinned by digest; `include:` templates pinned to a `ref:` tag or SHA.
- `workflow: rules:` so pipelines run for MRs and protected branches, without
  duplicate branch + MR pipelines.
- Secrets as **masked + protected** CI/CD variables; file-type variables for
  keys. Never commit them to `.gitlab-ci.yml`.
- `rules:` instead of `only/except`.
- `interruptible: true` on non-deploy jobs; `timeout:` on long jobs.
- Deploy jobs: `environment:`, `rules: if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH`
  (or protected tags), and `resource_group:` to serialise deploys.
- Use GitLab's ID tokens (`id_tokens:`) for cloud OIDC.
- Validate: `glab ci lint` (or the CI Lint page).

## 4. Secrets

- Scan every push: `gitleaks git --redact` (or GitLab secret detection).
- Mask secrets; never `set -x` in a step that touches them; never upload
  them as artifacts.
- Scope each secret to the environment/job that needs it.
- A leaked secret is rotated immediately. Deleting the commit isn't enough.

## 5. Caching and speed

- Cache dependency downloads keyed on the lock file hash.
- Don't cache anything that holds credentials.
- Prefer `npm ci` / `uv sync --frozen` / `pip install -r` with hashes over
  unlocked installs.

## 6. When a pipeline fails

Read the failing job log, reproduce locally with the same command, fix the
cause, push. Don't retry until green, don't mark jobs `allow_failure` /
`continue-on-error` to get past them, don't skip tests. (See the
`git-workflow` skill for the full rule.)

## Checklist before reporting done

- [ ] lint, test, security, build stages present and blocking
- [ ] All actions pinned by SHA (latest release, version comment); images by digest
- [ ] `permissions: contents: read` at top level; jobs widen only as needed
- [ ] No `${{ github.event.* }}` interpolated in `run:`; no `pull_request_target` checkout of PR code
- [ ] Secrets masked/protected, OIDC for cloud, gitleaks running
- [ ] `actionlint` + `zizmor` (or `glab ci lint`) clean
- [ ] The pipeline ran green on the change
