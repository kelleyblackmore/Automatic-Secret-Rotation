---
name: testing
description: How to write, fix and run automated tests that catch real bugs and stay maintainable. Use whenever adding or changing behaviour, fixing a bug, writing or reviewing tests, dealing with a failing or flaky test, or setting up test tooling and coverage, in any language. Covers what to test, choosing unit vs integration vs end-to-end, test structure and naming, fakes vs mocks, failing-test-first bug fixes, proving a test can fail, flaky-test root causes, coverage as a guide not a target, and the commands for pytest, Go, Rust and Vitest/Jest.
---

# Testing

A test is worth having when it fails for a real bug, passes otherwise, and tells
the reader what broke. Write tests that check behaviour, not implementation, so
a refactor doesn't break them and a regression does.

How test code should read is also covered in `clean-code`. When tests run in CI,
see `ci-pipelines`.

## Hard rules

1. **Every behaviour change ships with a test**, including the failure path.
2. **A bug fix starts with a failing test** that reproduces the bug.
3. **Prove the test can fail.** Break or revert the code under test and watch it
   go red. A test that never fails proves nothing.
4. **Never skip, delete, weaken or retry a test to get green.** Fix the code or
   fix the test's real flaw, and say which.
5. **Test through the public interface.** Don't test private functions, internal
   state or call order unless that *is* the contract.
6. **Tests are deterministic and isolated.** No real network, wall-clock time,
   randomness or shared state without control. Any order, any parallelism, same
   result.
7. **Use the project's existing test framework, layout and helpers.** Don't add
   a second runner or assertion library.

## 1. What to test

Start from the behaviour the change adds or alters, then cover:

- **The main case**: the reason the code exists.
- **Boundaries**: empty, one, many; zero, negative, max; first/last; exactly at
  a limit and one past it.
- **Bad input**: missing, malformed, wrong type, too large, unicode, injection
  strings at trust boundaries.
- **Failure paths**: the dependency times out, returns an error, returns
  nothing, or returns garbage. Check the error type and message, and that
  nothing was half-written.
- **State and concurrency**, where relevant: repeated calls (idempotency),
  concurrent calls, retries, ordering.

Skip what adds nothing: trivial getters, the framework's own behaviour, and
code that is already covered by a test at a better level.

## 2. Pick the right level

| Level | Use for | Keep it |
|---|---|---|
| **Unit** | Logic, rules, parsing, calculations, edge cases | Fast (ms), no I/O, most of your tests |
| **Integration** | Your code against a real DB, queue, filesystem or HTTP layer | Real dependency in a container or temp dir, a few per boundary |
| **End-to-end** | A critical user journey through the deployed stack | Few, stable, covering paths that must never break |

- Test each rule at the lowest level that can see it. Don't use an end-to-end
  test to check a date-formatting edge case.
- Test what mocks can't prove at the integration level: SQL, migrations,
  serialization, HTTP status codes and headers, file permissions.
- If code is hard to unit test, push the I/O to the edges (see `clean-code`,
  Structure) rather than mocking deeper.

## 3. Structure and naming

- **One behaviour per test**, named for it:
  `test_expired_token_is_rejected`, `TestParse/empty_input_returns_error`.
  The name should read as the spec.
- **Arrange, act, assert**, visibly separated. One act per test.
- **Assert on outcomes**: return values, raised errors, stored state, emitted
  messages. Assert the specific value, not just "truthy" or "no error".
- **No logic in tests**: no loops, branches or computed expected values that
  repeat the code under test. Use table-driven or parametrised cases instead.
- **Small, explicit fixtures.** Build only the data the test needs, close to the
  test. Use builders or factories with sensible defaults, overriding the field
  under test.
- **Failure messages say what differed.** Use the framework's rich asserts
  (`assert a == b` in pytest, `cmp.Diff` in Go, `assert_eq!` in Rust) so a
  failure shows expected vs actual.

## 4. Test doubles: fake at the edges

Replace only what is slow, non-deterministic or external: network, clock,
randomness, filesystem, other services. Use the real thing for your own code.

| Double | Use |
|---|---|
| **Fake**: a small working implementation (in-memory repo, fake clock) | Preferred. Tests behaviour, survives refactors |
| **Stub**: returns canned answers | Driving a code path (error, timeout, empty) |
| **Mock**: asserts it was called a certain way | Only when the call *is* the behaviour (sent the email, charged the card) |

- Never mock the unit under test, and never mock types you don't own deeply.
  Wrap them in a thin adapter you own and fake that.
- Too many mocks in one test means the design needs a seam, not more mocks.
- Inject dependencies (parameters, constructor, interface) rather than patching
  globals.

Example (Python, pytest): a fake clock and parametrised boundaries.

```python
# auth_token.py
from datetime import datetime, timedelta

class Token:
    def __init__(self, issued_at: datetime, ttl: timedelta):
        self.expires_at = issued_at + ttl

    def is_valid(self, now: datetime) -> bool:
        return now < self.expires_at


# test_token.py
from datetime import datetime, timedelta

import pytest

from auth_token import Token

ISSUED = datetime(2026, 1, 1, 12, 0, 0)
TTL = timedelta(minutes=15)


@pytest.mark.parametrize(
    ("elapsed", "valid"),
    [
        (timedelta(0), True),
        (TTL - timedelta(microseconds=1), True),
        (TTL, False),  # expires exactly at the TTL
        (TTL + timedelta(days=1), False),
    ],
)
def test_token_validity_at_boundaries(elapsed, valid):
    token = Token(issued_at=ISSUED, ttl=TTL)

    assert token.is_valid(now=ISSUED + elapsed) is valid
```

Time is passed in, not read from the system, so there is nothing to patch and
the boundary is exact.

## 5. Fixing a bug

1. Reproduce it by hand and note the exact input.
2. Write the smallest test that reproduces it, at the lowest level that can.
3. Run it and **watch it fail for the right reason** (the bug, not a typo or
   import error).
4. Fix the code. Run the test: green. Run the whole suite: green.
5. Look for the same bug nearby (sibling functions, other callers) and cover
   those too.

## 6. Prove the test can fail

Before you call a test done, check it would catch a regression. For new
behaviour, break the code under test (flip a condition, return early), run the
test, expect red, and undo the change.

For a bug fix, commit the fix and its test first, then run the new test against
the base branch's code (assuming tests live outside `src/`):

```bash
git checkout origin/develop -- src/   # old code, new test
pytest tests/test_token.py            # must fail, for the bug's reason
git checkout HEAD -- src/             # back to the committed fix
pytest tests/test_token.py            # must pass
git status                            # clean again
```

For critical logic, mutation testing automates this: `mutmut` (Python),
`cargo-mutants` (Rust), `Stryker` (JS/TS). Run it on the changed module, not
the whole codebase.

## 7. Flaky tests

A test that sometimes fails is a bug in the test or the code. Find it:

| Symptom | Usual cause | Fix |
|---|---|---|
| Fails near midnight, month end, DST, in CI's timezone | Real clock or local timezone | Inject the clock; use UTC; fixed timestamps |
| Fails when run with others or in another order | Shared state: globals, singletons, DB rows, temp files, env vars | Fresh fixture per test; clean up in teardown; unique names |
| Fails under load or in CI only | Sleeps and timeouts racing real work | Wait for the condition (poll with a deadline, events, channels), never `sleep` |
| Fails sometimes with random data | Unseeded randomness | Seed it and print the seed on failure |
| Fails when the network is slow or down | Real external call | Fake it at the edge; real calls only in marked integration tests |
| Fails only in parallel | Data race, shared port or file | Run with the race detector; allocate free ports and temp dirs |

Reproduce it by repeating the test and shuffling the order (see commands below).
Quarantining or auto-retrying is not a fix. If you can't fix it now, say so and
open an issue with the failure output.

## 8. Coverage

- Use coverage to **find untested code**, especially branches, not as a target.
  100% coverage with weak asserts catches nothing.
- Look at the changed lines: every new branch should be exercised, or there
  should be a reason it isn't.
- If the project enforces a threshold, don't lower it. Add the missing tests.
- Measure branch coverage, not just line coverage.

## 9. Property-based and fuzz tests

For parsers, encoders, serializers and anything with a large input space, add a
property test alongside the examples: "decode(encode(x)) == x", "output is
always sorted", "never raises on any string". Tools: `hypothesis` (Python),
`go test -fuzz` (Go), `proptest` (Rust), `fast-check` (JS/TS). When one finds a
failure, keep that input as a regular example test.

## 10. Commands

Use the project's own task runner (`make test`, `just test`, `npm test`) if it
has one. Otherwise:

| | Run all | Run one | Random order / repeat | Coverage |
|---|---|---|---|---|
| **Python (pytest)** | `pytest` | `pytest tests/test_x.py::test_name` | `pytest -p randomly` (pytest-randomly), `pytest --count=50` (pytest-repeat) | `pytest --cov --cov-branch --cov-report=term-missing` |
| **Go** | `go test -race ./...` | `go test -run 'TestParse/empty' ./pkg/x` | `go test -shuffle=on -count=50 ./pkg/x` | `go test -coverprofile=cover.out ./... && go tool cover -func=cover.out` |
| **Rust** | `cargo test` | `cargo test tests::parse_empty -- --exact` (full path) | `cargo test -- --test-threads=1` to rule out parallelism | `cargo llvm-cov` (cargo-llvm-cov) |
| **Vitest** | `vitest run` | `vitest run src/x.test.ts -t "name"` | `vitest run --sequence.shuffle` | `vitest run --coverage` (@vitest/coverage-v8) |
| **Jest** | `jest` | `jest src/x.test.ts -t "name"` | `jest --randomize` | `jest --coverage` |

Run the narrowest test while working, and the full suite before you push.

Go, table-driven with subtests:

```go
func TestParsePort(t *testing.T) {
	tests := []struct {
		name    string
		in      string
		want    int
		wantErr bool
	}{
		{name: "valid", in: "8080", want: 8080},
		{name: "lowest", in: "1", want: 1},
		{name: "highest", in: "65535", want: 65535},
		{name: "zero", in: "0", wantErr: true},
		{name: "too high", in: "65536", wantErr: true},
		{name: "not a number", in: "http", wantErr: true},
		{name: "empty", in: "", wantErr: true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got, err := ParsePort(tt.in)
			if (err != nil) != tt.wantErr {
				t.Fatalf("ParsePort(%q) error = %v, wantErr %v", tt.in, err, tt.wantErr)
			}
			if got != tt.want {
				t.Errorf("ParsePort(%q) = %d, want %d", tt.in, got, tt.want)
			}
		})
	}
}
```

## Reviewing tests

When reviewing a change (see `code-review`), check:
- Would the test fail if the change were reverted?
- Is the failure path tested, not just the main case?
- Were any tests skipped, deleted, loosened, or given retries? Why?
- Are there sleeps, real network calls, real time or unseeded randomness?
- Do mocks check behaviour, or just mirror the implementation line by line?

## Checklist

- [ ] New or changed behaviour has tests, including failure paths and boundaries
- [ ] Bug fix has a test that failed before the fix and passes after
- [ ] Each test checks one behaviour, is named for it, and asserts specific outcomes
- [ ] Only external, slow or non-deterministic things are faked
- [ ] No sleeps, real clock, real network, unseeded randomness or shared state
- [ ] Tests pass in random order and in parallel
- [ ] Nothing skipped, deleted, weakened or retried to get green
- [ ] Changed lines and branches are covered; any project threshold still met
- [ ] Full suite passes locally and in CI
