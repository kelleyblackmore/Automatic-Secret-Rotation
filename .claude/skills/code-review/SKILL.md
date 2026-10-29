---
name: code-review
description: How an agent reviews a pull/merge request or a diff. Use whenever asked to review code, a PR/MR, a branch, a commit, or a patch, or before approving or merging. Covers what to check (correctness, security, tests, scope, the repo's own rules), verifying every finding before reporting it, severity labels, and how to write comments that are short, specific and actionable.
---

# Code Review

A review is useful only if every finding in it is real and every real problem
of consequence is in it. Optimise for that, not for volume.

## Hard rules

1. **Verify before you report.** Every finding needs a concrete path: the input
   or call that triggers it and the wrong result. If you can't build one, drop
   it or mark it a question.
2. **Read the code, not just the diff.** Open the callers, the callees and the
   tests the change touches.
3. **Label severity honestly.** Use the scale below; don't inflate nits or bury
   real bugs among them.
4. **Be specific and brief.** File and line, what's wrong, why it matters, a
   suggested fix. No praise padding, no restating the diff.
5. **Stay in scope.** Review what the change does. Pre-existing problems are
   noted once, marked as pre-existing, and never block the change.

## 1. Get context first

- Read the PR/MR description and linked issue: what is this change *for*?
- Read the repo's rules: `CONTRIBUTING.md`, `CLAUDE.md`, `AGENTS.md`,
  `REVIEW.md`, lint configs, and any skills in `.claude/skills/` or
  `.agents/skills/` (e.g. `git-workflow`, `*-secure-coding`).
- Check CI on the head commit. A red pipeline is the first finding.
- Get the full diff against the merge base:
  ```bash
  git fetch origin && git diff origin/develop...HEAD --stat
  git diff origin/develop...HEAD
  ```

## 2. What to check, in priority order

1. **Correctness.** Does it do what it claims? Edge cases: empty, null, zero,
   one, many, max size, unicode, concurrent calls, partial failure, retries,
   timezones. Off-by-one, wrong operator, inverted condition, unhandled error,
   resource leak, race.
2. **Security.** Untrusted input reaching a shell, SQL, path, URL, template,
   deserializer or `eval`. AuthN/AuthZ checks on every new endpoint. Secrets in
   code, logs or errors. New dependencies (real, maintained, latest, needed?).
   Use the matching `*-secure-coding` skill's patterns as a checklist.
3. **Data and compatibility.** Migrations reversible and safe on a live table?
   API/schema changes backward compatible? Config and feature-flag defaults?
4. **Tests.** Is the new behaviour tested, including the failure path? Would the
   test fail if the fix were reverted? Are any tests skipped, weakened or deleted?
   (See `testing`, Reviewing tests.)
5. **Design and maintainability.** Duplicates existing code? Right layer? Names
   say what things are? Comments explain *why*?
6. **Style.** Only what the linters don't catch, and only if the repo cares.

## 3. Verify each finding

For every candidate finding, before writing it down:

- Trace a real path: which caller, with which input, reaches the line, and what
  goes wrong. Quote the lines.
- Where cheap, prove it: run the test suite, write a failing test, run the
  snippet, check the library's actual behaviour or docs.
- Check it isn't already handled elsewhere (validation upstream, a wrapper, a
  framework default).
- If you can't confirm it, either drop it or ask it as a question
  ("Is `user` guaranteed non-null here? `load()` can return None at L42.").

## 4. Severity

| Label | Meaning | Blocks merge |
|---|---|---|
| 🔴 **blocker** | Bug, security hole, data loss, broken build/test, violates a hard repo rule | Yes |
| 🟡 **should fix** | Likely problem or clear maintainability cost; fix now unless there's a reason | Usually |
| 🔵 **nit** | Optional polish | No |
| 🟣 **pre-existing** | Real issue not introduced by this change | No |
| ❓ **question** | You need information to decide | Until answered |

## 5. Writing comments

```
🔴 blocker: path traversal in upload handler (api/files.py:58)

`name` comes from the request and is joined into UPLOAD_DIR unchecked, so
`../../etc/passwd` escapes the folder. Resolve the path and require it to
stay under UPLOAD_DIR (see python-secure-coding, "Files and paths").
```

- One issue per comment, anchored to the line.
- Suggest the fix (a code suggestion block when it's small).
- Explain the impact in one sentence; skip the lecture.
- Don't comment on things the linter/formatter will fix.
- Never include AI attribution or planning labels (see `git-workflow`).

## 6. The summary

End with a short summary, most severe first:

```
2 blockers, 1 should-fix, 2 nits. CI green.

🔴 SQL built with f-string in reports/query.py:31
🔴 Missing auth check on DELETE /projects/{id}
🟡 No test for the empty-cart path
🔵 2 naming nits (inline)
```

Then a verdict: **approve**, **approve with nits**, or **request changes**.
If there are no findings, say so in one line, and say what you checked.

## Reviewing your own change

Run the same process on your own diff before pushing, adversarially: "what
would make CI or a reviewer reject this?" Fix what you find first.

## Checklist

- [ ] Read description, repo rules, and CI status
- [ ] Read surrounding code, not just the diff
- [ ] Each finding has a traced path (or is a question)
- [ ] Severity labels applied honestly; pre-existing issues marked
- [ ] Comments are line-anchored, specific, with a suggested fix
- [ ] Summary with counts and a verdict
