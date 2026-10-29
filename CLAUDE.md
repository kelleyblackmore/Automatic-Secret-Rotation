# Agent instructions

This repo carries agent skills in `.claude/skills/`, copied from
https://github.com/kelleyblackmore/Skills. Use them on every task:

- `git-workflow` for branches, commits, pre-commit and merge requests.
- `pragmatic-coding` and `clean-code` when writing or changing code.
- `testing` for every behaviour change and bug fix.
- `code-review` when reviewing a change, including your own before pushing.
- `secrets-management` whenever credentials, tokens or keys are involved.
- `ci-pipelines` when changing `.github/workflows/`.

For Rust code also follow `rust-secure-coding`.

To update the skills, copy the latest folders from the Skills repo over
`.claude/skills/`.
