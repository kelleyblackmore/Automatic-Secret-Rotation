---
name: pragmatic-coding
description: How to decide what to build and how much, so changes are small, correct and easy to keep. Use whenever implementing a feature, fixing a bug, choosing a design or dependency, estimating, or deciding whether to refactor, generalise or stop. Covers understanding the real problem first, the smallest change that solves it, YAGNI, keeping knowledge in one place, decoupling, reversible decisions, failing fast, automating checks, debugging by evidence, and finishing (tests, docs, cleanup) before calling it done.
---

# Pragmatic Coding

Solve the problem in front of you, well, with the least code and the fewest
decisions you'll regret. Care about the result, not about the tools, patterns or
the cleverness of the solution.

For how the code itself should read, see `clean-code`. For commits and MRs, see
`git-workflow`.

## Hard rules

1. **Understand before you change.** Know the actual problem, the expected
   behaviour and how you'll check it before writing code.
2. **Make the smallest change that fully solves it.** No speculative features,
   options or layers (YAGNI). No unrelated edits.
3. **Prove it works.** Run it, test it, check the output. "Should work" is not
   done.
4. **Fit in.** Use the project's existing language, libraries, patterns and
   tooling before adding new ones.
5. **Fail fast and loudly.** Detect impossible states early with a clear error.
   Never hide a failure to keep going.
6. **Own the outcome.** If something is wrong, unclear or out of scope, say so
   plainly with options. Don't guess silently and don't paper over it.

## 1. Understand the problem

- Restate the task in one sentence. If you can't, ask, or read more.
- Find the real requirement behind the request: "add a retry" might really mean
  "this call times out under load".
- Reproduce a bug before fixing it, ideally as a failing test.
- Read the code you're about to change, its callers and its tests. Look for an
  existing helper before writing one.
- Ask when the answer changes what you build. Otherwise pick the conventional
  default, state it, and continue.

## 2. Keep the change small

- Prefer changing existing code over adding parallel code paths.
- Prefer configuration of something that exists over a new mechanism.
- Hard-code until a second real case appears. Generalise on real needs, not
  imagined ones.
- Every line, dependency and option is a cost someone pays later. Delete freely.
- Note out-of-scope problems ("broken windows") in your summary or an issue.
  Fix them in the current change only when they block it.

## 3. Build it incrementally

- **Tracer bullet first:** get a thin, working path end to end (input to output,
  UI to database), then fill it in. You find integration problems early.
- Take small steps, each ending in a state that builds and passes tests.
- Spike to learn when the approach is unclear, then throw the spike away and
  write the real thing. Don't ship prototype code by accident.
- Keep feedback loops short: run the narrowest test that covers the change
  often, and the full suite before you finish.

## 4. Design for change

- **One source of truth (DRY):** every rule, constant, schema or setting lives in
  exactly one place. Generate or derive the rest (docs, clients, types) rather
  than copying.
- **Orthogonality:** changing one thing shouldn't require changing unrelated
  things. Keep modules focused, and talk through narrow interfaces.
- **Reversibility:** isolate vendor, framework and infrastructure choices behind
  a seam you own when switching them later is plausible. Don't build a seam for
  every library.
- **Configuration, not code, for values that vary by environment.** Keep secrets
  out of both (see `secrets-management`).
- **Boring technology:** prefer the well-known, maintained option the team
  already uses. A new dependency must earn its place: needed, maintained, latest
  stable, and small enough to understand.

## 5. Program defensively, not paranoidly

- State and check assumptions: validate inputs at boundaries, and assert
  invariants that "can't happen".
- Crash early with a clear message rather than corrupt data or limp on.
- Handle the errors you expect (missing file, timeout, bad input) and let truly
  unexpected ones surface.
- Make illegal states unrepresentable where the type system allows it (enums,
  non-null types, value objects).
- Clean up what you open, on every path.

## 6. Debug by evidence

1. Reproduce it reliably. Write down the exact steps and inputs.
2. Read the actual error and stack trace, all of it.
3. Check your own code first. The compiler, the OS or the library is rarely the
   problem.
4. Form one hypothesis, test it, and change one thing at a time. Bisect
   (`git bisect`, halving the input) when the cause is unclear.
5. Explain the problem out loud or in writing, step by step. The gap usually
   shows up.
6. Fix the cause, not the symptom. Add a test that would have caught it. Look
   for the same bug elsewhere.

## 7. Automate the boring parts

- If you do it twice by hand, script it. If the team does it, put it in the repo
  (`Makefile`, `justfile`, `scripts/`).
- Let tools enforce style and catch mistakes: formatter, linters, type checker,
  pre-commit, CI. Don't spend review time on what a tool can check.
- Make the build, test and release reproducible from a clean checkout with one
  command each.

## 8. Estimate and communicate

- Break work into pieces small enough to estimate. Give a range, and say what
  drives the uncertainty.
- Surface risks and blockers as soon as you find them, not at the end.
- When trade-offs exist, give a recommendation with the reason, not just a list
  of options.
- Report results faithfully: what you changed, what you verified, what you
  didn't verify, and what is left.

## 9. Finish the job

"Done" means:
- The problem is solved and verified (tests pass, behaviour checked).
- New behaviour has tests, including the failure path.
- Docs, config examples and changelogs affected by the change are updated.
- No debug output, dead code, stray files or TODOs without an owner remain.
- Linters, type checks and CI are green.

## Warning signs

Writing code before you can state the problem · adding an option "in case" ·
a new abstraction with one implementation · a new dependency for a few lines ·
editing many unrelated files · "it works on my machine" · retrying a flaky test
until it passes · catching every exception to make an error go away · a fix you
can't explain.

## Checklist

- [ ] Problem understood and restated; bug reproduced if it's a bug
- [ ] Smallest change that solves it; nothing speculative, nothing unrelated
- [ ] Reused existing code, patterns and dependencies where possible
- [ ] Each piece of knowledge has one home; modules stay decoupled
- [ ] Assumptions checked; failures are loud and clear
- [ ] Verified by running it and by tests, including the failure path
- [ ] Docs and config updated; no leftovers; CI green
- [ ] Summary says what changed, what was verified, and what remains
