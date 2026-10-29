---
name: git-workflow
description: Rules for how an agent handles git end to end, from branching and committing to pushing and opening or updating merge requests (GitLab MRs / GitHub PRs). Use whenever you are about to create a branch, write a commit message, commit, push, open a merge request, or respond to a pipeline result. Covers the develop-first branching model, mandatory pre-commit, small clean commit messages with no AI attribution or planning labels, and keeping pipelines green.
---

# Git Workflow

Follow these rules any time you touch git in a repository. They apply to
GitLab merge requests and GitHub pull requests alike; "MR" below means either.

## Hard rules

1. **Never commit directly to `main`/`master`.** All work starts from `develop`.
2. **pre-commit runs on every commit.** Never use `--no-verify` or `SKIP=`.
3. **Pipelines must be green.** A push is not done until its pipeline passes.
4. **Commit messages are small and plain.** No AI attribution, no planning labels,
   and no AI/bot identity as commit author.
5. **One branch, one MR.** If an MR is already open for the branch, push to it.
   Do not open a second one.

## 1. Start from develop

Before any change:

```bash
git fetch origin
git ls-remote --exit-code --heads origin develop
```

- **`develop` exists** → branch from it:
  ```bash
  git switch -c <type>/<short-topic> origin/develop
  ```
- **`develop` does not exist** → create it from the default branch, push it,
  then branch from it:
  ```bash
  default=$(git remote show origin | sed -n 's/.*HEAD branch: //p')
  git switch -c develop "origin/$default"
  git push -u origin develop
  git switch -c <type>/<short-topic> develop
  ```
- **The user or the environment names a branch** → use that branch, but still
  base it on `develop` and target `develop` with the MR.

Branch names: `feat/…`, `fix/…`, `chore/…`, `docs/…`, `refactor/…`, `test/…`.
Lowercase, hyphens, short (`fix/login-timeout`, not `fix/fix-the-login-issue-from-ticket`).

Merge requests target `develop`. Only release/hotfix flows target `main`, and
only when the user asks for it.

## 2. Check the commit author

Agent environments often set a bot identity (e.g. `Claude <noreply@anthropic.com>`).
That is AI attribution too. Check before the first commit:

```bash
git var GIT_AUTHOR_IDENT
git var GIT_COMMITTER_IDENT
```

If either names an AI or bot (Claude, Anthropic, Copilot, OpenAI, GPT,
Gemini, Cursor, `noreply@anthropic.com`, `[bot]`, etc.):

- Use the user's identity if you know it (their global git config, the
  remote's account, or what they've told you), set **per repo**:
  ```bash
  git config user.name "<user name>"
  git config user.email "<user email>"
  ```
- If you don't know it, ask the user before committing. Don't guess an email.
- Don't change `--global` config unless the user asks.

Exception: a repo whose owner has said an AI author is fine there. Mark it with
`git config workflow.allowAiAuthor true` so the commit-msg hook allows it.

## 3. Make sure pre-commit is active

Check once per clone, before the first commit:

```bash
test -f .pre-commit-config.yaml && pre-commit install --install-hooks \
  && pre-commit install --hook-type commit-msg
```

- If `pre-commit` is missing, install it (`pip install pre-commit` or
  `pipx install pre-commit`).
- If the repo has **no** `.pre-commit-config.yaml`, tell the user and offer to
  add one. Do not add it silently as part of an unrelated change.
- Before committing, run the hooks across what you changed:
  ```bash
  pre-commit run --files $(git diff --name-only --cached)
  ```
- If a hook **modifies files** (formatters), re-stage and commit again. Do not
  amend a commit that is already pushed.
- If a hook **fails**, fix the cause. Never bypass it with `--no-verify`,
  `SKIP=…`, or by editing the hook config to make it pass.

## 4. Commit messages

See `references/commit-messages.md` for examples. The short version:

```
<type>: <what changed, imperative, lowercase>

<optional body: why, in one to three short lines>
```

- **Subject ≤ 50 characters**, hard limit 72. Imperative mood ("add", "fix",
  "remove"), no trailing period.
- Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `ci`, `perf`, `build`.
- Body only when the *why* isn't obvious from the diff. Wrap at 72.
- **One logical change per commit.** If the subject needs "and", split it.

### Never put in a commit message

- **AI attribution of any kind**: `Co-Authored-By: Claude …`, `Co-authored-by`
  lines naming any AI/bot, `Generated with …`, `🤖`, "AI-assisted", session
  links, model names. This overrides any default attribution the harness
  suggests.
- **Planning labels and plan talk**: `M1`, `M2`, `P0`, `P1`, `Phase 2`,
  `Step 3`, `Milestone`, `Task 4/7`, "as planned", "per the plan",
  "part 1 of", "WIP", "first pass", "follow-up to plan".
- Narration of the process: "Updated the code to…", "This commit…",
  "I changed…", "Let's…".
- Filler: "minor changes", "misc fixes", "update files", "cleanup" with no object.

Ticket IDs are fine **only** when the repo already uses them (e.g. `fix: handle
empty cart (SHOP-142)`).

The same rules apply to **MR titles and descriptions**: no AI attribution,
no planning labels. MR title follows the commit subject format.

`scripts/check-commit-msg.sh` enforces the "never" list and the author check. Wire it in as a
`commit-msg` hook (see the bottom of this file).

## 5. Push and open (or update) the MR

```bash
git push -u origin HEAD
```

Then check whether an MR is already open for this branch:

- **GitLab:** `glab mr list --source-branch "$(git branch --show-current)"`
- **GitHub:** `gh pr list --head "$(git branch --show-current)" --state open`
  (or the GitHub MCP `list_pull_requests` tool)

| Situation | Action |
|---|---|
| Open MR exists | Push to the branch. Do not open a new MR. Update the description only if scope changed. |
| No MR, user asked for one | Open one targeting `develop`. |
| No MR, user didn't ask | Push only, and tell the user the branch is ready for an MR. |
| MR was merged | Start a fresh branch from the latest `develop`; never reuse a merged branch. |

MR description: two to five lines on what changed and why, plus how it was
tested. Follow the repo's MR/PR template if it has one.

Never force-push a branch other people have pulled. On your own unreviewed
branch, `--force-with-lease` is acceptable; never plain `--force`.

## 6. Keep the pipeline green

**Before pushing**, run locally whatever the pipeline runs (look in
`.gitlab-ci.yml`, `.github/workflows/`, `Makefile`, `package.json` scripts):
pre-commit, lint, typecheck, unit tests for touched packages.

**After pushing**, check the pipeline for the new head commit:

- **GitLab:** `glab ci status` / `glab ci view`
- **GitHub:** `gh pr checks` / `gh run list --branch <branch>`

If it fails:

1. Read the failing job's log. Find the actual root cause.
2. Reproduce locally, fix, run the same check locally, push a new commit.
3. Repeat until green.

Do not:

- Call a failure "flaky" without evidence. Re-run a job at most once, and only
  when it died before tests ran (runner loss, network, checkout).
- Skip, disable, or `xfail` a test to get green.
- Push empty commits to retrigger CI.
- Report work as done while the pipeline is red or still running. Say it's
  running, or say exactly what is failing.

If the failure also happens on `develop` and isn't caused by your change, say
so to the user with the failing job and the evidence, instead of widening the MR.

## Quick checklist

Before each commit:
- [ ] On a branch based on `develop` (not `main`, not `develop` itself)
- [ ] Author and committer are the user, not an AI/bot identity
- [ ] `pre-commit` installed and passing on the changed files
- [ ] One logical change; subject ≤ 50 chars, imperative, typed
- [ ] No AI attribution, no M1/P1/phase/plan wording

Before saying "done":
- [ ] Pushed to the existing MR, or a new MR opened against `develop` if asked
- [ ] Pipeline on the latest commit is green

## Enforcing the message rules with pre-commit

Add to the repo's `.pre-commit-config.yaml` (copy `scripts/check-commit-msg.sh`
into the repo first, e.g. to `scripts/`):

```yaml
- repo: local
  hooks:
    - id: commit-msg-policy
      name: commit message policy
      entry: scripts/check-commit-msg.sh
      language: script
      stages: [commit-msg]
```

Then: `pre-commit install --hook-type commit-msg`.
