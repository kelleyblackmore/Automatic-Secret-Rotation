#!/usr/bin/env bash
# commit-msg hook: rejects AI attribution, planning labels, and overlong subjects.
# Usage: check-commit-msg.sh <path-to-commit-msg-file>
set -euo pipefail

msg_file="${1:?usage: check-commit-msg.sh <commit-msg-file>}"

# Drop comment lines git adds to the editor template.
msg=$(grep -v '^#' "$msg_file" || true)
subject=$(printf '%s\n' "$msg" | sed -n '/./{p;q;}')

# Let git's own merge/revert/fixup subjects through.
case "$subject" in
  "Merge "* | "Revert "* | "fixup! "* | "squash! "* | "amend! "*) exit 0 ;;
esac

fail=0
err() {
  echo "commit-msg: $*" >&2
  fail=1
}

if [ -z "$subject" ]; then
  err "empty subject"
fi

if [ "${#subject}" -gt 72 ]; then
  err "subject is ${#subject} chars; keep it under 50, max 72"
fi

if printf '%s' "$subject" | grep -qE '\.$'; then
  err "subject ends with a period"
fi

ai_pattern='co-authored-by:.*(claude|anthropic|openai|chatgpt|gpt-?[0-9]|copilot|gemini|cursor|codeium|\[bot\]|noreply@anthropic)|generated (with|by) .*(claude|ai|gpt|copilot)|🤖|ai[- ]assisted|ai[- ]generated|claude\.ai/code|claude-session:'
if printf '%s\n' "$msg" | grep -qiE "$ai_pattern"; then
  err "contains AI attribution; remove it"
fi

plan_pattern='(^|[^[:alnum:]])(m[0-9]+|p[0-9]+|phase[ -]?[0-9]+|milestone[ -]?[0-9]*|step [0-9]+( of [0-9]+)?|task [0-9]+(/[0-9]+)?|part [0-9]+ of)([^[:alnum:]]|$)|as planned|per (the )?plan|(of|from|in) the plan|wip([^[:alnum:]]|$)'
if printf '%s\n' "$msg" | grep -qiE "$plan_pattern"; then
  err "contains planning labels (M1, P1, phase, step N, 'as planned', WIP); describe the change instead"
fi

# Author/committer identity. Opt out per repo with:
#   git config workflow.allowAiAuthor true
ai_ident='claude|anthropic|copilot|openai|chatgpt|gpt-?[0-9]|gemini|cursor|codeium|\[bot\]'
if [ "$(git config --bool workflow.allowAiAuthor || echo false)" != "true" ]; then
  for var in GIT_AUTHOR_IDENT GIT_COMMITTER_IDENT; do
    ident=$(git var "$var" 2>/dev/null | sed 's/>.*/>/' || true)
    if printf '%s' "$ident" | grep -qiE "$ai_ident"; then
      err "$var is an AI/bot identity ($ident); set git config user.name/user.email to the user"
    fi
  done
fi

if [ "$fail" -ne 0 ]; then
  echo "commit-msg: subject was: $subject" >&2
  exit 1
fi
