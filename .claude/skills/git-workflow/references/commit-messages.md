# Commit message examples

## Good

```
fix: handle empty cart on checkout
```

```
feat: add retry to webhook sender
```

```
refactor: extract price rounding helper
```

```
fix: stop double-charging on retry

The payment client retried on timeout without an idempotency key.
```

```
ci: cache pip downloads between jobs
```

```
docs: document local setup for macOS
```

## Bad, and the fix

| Bad | Why | Better |
|---|---|---|
| `M1: implement auth service` | planning label | `feat: add token auth service` |
| `P1 fix for login` | planning label | `fix: reject expired login tokens` |
| `Phase 2 - add caching layer as planned` | phase + plan talk | `perf: cache product lookups` |
| `Step 3 of plan: write tests` | plan talk | `test: cover order cancel paths` |
| `Updated files` | says nothing | `fix: correct tax rate for CA` |
| `This commit adds a new endpoint for users and also fixes the bug in billing and updates the docs` | too long, three changes | split into three commits |
| `WIP` | not a finished change | finish it, or squash before pushing |
| `feat: add search` + `Co-Authored-By: Claude <noreply@anthropic.com>` | AI attribution | `feat: add search` |
| `chore: lint fixes 🤖 Generated with Claude Code` | AI attribution | `chore: fix lint warnings in api` |
| `Fixed the bug.` | past tense, vague, period | `fix: guard null user in profile page` |

## Splitting a change

If you have staged unrelated work together, split it before committing:

```bash
git reset
git add -p src/billing/        # stage only the billing fix
git commit -m "fix: round invoice totals to cents"
git add docs/
git commit -m "docs: describe invoice rounding"
```

## MR titles and descriptions

Title: same format as a commit subject.

Description:

```
Rounds invoice totals to cents before tax is applied, which was causing
off-by-one-cent totals on multi-line invoices.

Tested: unit tests in billing/, manual check on a 12-line invoice.
```

No checklists of "milestones", no "Phase 1 complete", no AI attribution footer.
