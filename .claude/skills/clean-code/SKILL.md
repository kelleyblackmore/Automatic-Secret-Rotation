---
name: clean-code
description: How to write and refactor code so the next reader understands it quickly. Use whenever writing new code, refactoring, cleaning up, or reviewing code for readability, in any language. Covers naming, small single-purpose functions, guard clauses, explicit error handling, comments that explain why, removing dead code and duplication, readable tests, and matching the codebase's existing style. Prefers clarity over cleverness and stops before cleanup turns into churn.
---

# Clean Code

Code is read far more often than it is written. Write it so a teammate who has
never seen it can tell what it does, why, and where to change it, without
running it in their head.

Clean is not the same as short, clever or abstract. When a rule here fights
readability, readability wins. For *how much* to change and when to stop, see
`pragmatic-coding`.

## Hard rules

1. **Match the codebase first.** Its naming, layout, error handling and idioms
   beat any rule below. Consistency is part of readability.
2. **Names say what things are.** No `data`, `tmp`, `obj`, `handle`, `doStuff`,
   `mgr`, or single letters outside tiny scopes.
3. **One function, one job, one level of abstraction.** If you need "and" to
   describe it, split it.
4. **Handle errors explicitly.** Never swallow an exception or ignore a return
   code. Fail with a message that says what went wrong and with which input,
   identified by a field name or safe ID. Never put passwords, tokens, keys or
   connection strings in errors or logs.
5. **Comments explain why, not what.** If a comment explains what, rename or
   restructure until it doesn't need to.
6. **No dead code.** No commented-out blocks, unused parameters, imports,
   flags or functions. Git keeps history.
7. **Leave the code you touch a little cleaner**, within the change's scope.
   Don't reformat or rename across files you weren't asked to change.

## 1. Naming

- Intention-revealing: `elapsed_days`, not `d`; `is_expired()`, not `check()`.
- Booleans read as yes/no questions: `is_active`, `has_access`, `should_retry`.
- Functions are verbs (`load_invoice`); classes and values are nouns (`Invoice`).
- Use one word per concept: don't mix `fetch`, `get` and `retrieve` for the same
  thing.
- Use domain words the team uses (`invoice`, `tenant`), not invented ones.
- Put units in names when the type doesn't: `timeout_seconds`, `size_bytes`.
- Name a magic number once: `MAX_LOGIN_ATTEMPTS = 5`, not a bare `5` in three places.
- Name length grows with scope: `i` in a 3-line loop is fine; a module-level
  `i` is not.

## 2. Functions

- Small enough to read at a glance. Size isn't the goal; one clear job is.
- Few parameters, ideally three or fewer. Group related ones into an object, and
  avoid boolean flag arguments (`render(true)`). Two named functions or an enum
  read better.
- No hidden side effects. A function called `get_user` doesn't write to the
  database or send email.
- Separate commands from queries: a function either changes state or returns
  information, not both, unless the language idiom expects it (e.g. `pop`).
- Return early with guard clauses instead of nesting:

```python
# Before
def ship(order):
    if order is not None:
        if order.paid:
            if order.items:
                return dispatch(order)
            else:
                raise ValueError("no items")
        else:
            raise ValueError("unpaid")
    raise ValueError("no order")

# After
def ship(order):
    if order is None:
        raise ValueError("no order")
    if not order.paid:
        raise ValueError("unpaid")
    if not order.items:
        raise ValueError("no items")
    return dispatch(order)
```

Same behaviour, same errors: a refactor changes structure only. Improve the
messages in a separate change.

## 3. Structure

- Keep related code together and in reading order: a caller comes before the
  helpers it calls.
- Keep each function at one level of abstraction. Don't mix `parse_config()`
  with byte-level string slicing in the same body.
- Prefer plain data and plain functions. Add a class, interface or pattern when
  it removes real duplication or isolates real change, not in advance.
- Isolate side effects (I/O, network, clock, randomness) at the edges so the core
  logic is easy to test.
- Prefer immutable values and narrow scopes. Declare variables where they're used.
- Replace long `if`/`switch` chains on a type with a lookup table or polymorphism,
  once there are three or more cases and they keep growing.

## 4. Errors

- Raise or return specific errors, not generic `Exception` or `-1`.
- Validate at the boundary (user input, files, network, config) and trust
  validated data inside.
- Catch only what you can handle. Re-raise with context, keeping the cause:
  `raise ConfigError(f"bad port in {path}") from err`.
- Clean up with the language's tool (`with`, `defer`, `try/finally`, RAII), not
  by hand on every path.
- Don't return `null`/`None` to mean "error". Use an exception, a result type, or
  an explicit empty value.
- Never log and re-raise the same error at every layer. Handle it, or pass it up.

## 5. Comments and docs

Good comments:
- *Why* something non-obvious is done: a workaround, a business rule, a
  performance trade-off, with a link to the issue or spec.
- Warnings: "Not thread-safe; callers hold `lock`."
- Public API docs: what it does, parameters, errors, an example.

Bad comments:
- Restating the code (`i += 1  # increment i`).
- Commented-out code, changelogs, author names, closing-brace labels.
- `TODO` without an owner or issue link. Fix it, file it, or delete it.
- Comments that no longer match the code. Update or delete them in the same change.

## 6. Duplication

- Remove duplicated *knowledge*: the same rule, constant or formula in two places
  will drift. Give it one home.
- Don't merge code that only looks alike but changes for different reasons. A
  wrong shared abstraction is worse than two copies.
- Rule of three: tolerate a second copy; extract on the third.

## 7. Tests

What to test and how is in `testing`. For readability:

- Tests are code too: same naming and clarity standards.
- One behaviour per test, named for it:
  `test_expired_token_is_rejected`, not `test_token_2`.
- Arrange, act, assert, in that order and visibly separated.
- No logic (loops, branches) in tests. Use parametrised cases instead.
- Test behaviour through the public interface, not private internals.

## 8. Refactoring safely

1. Make sure tests cover the behaviour first. If they don't, add them.
2. Change structure *or* behaviour in a commit, never both.
3. Take small steps (rename, extract, inline, move) and run the tests after each one.
4. Use the IDE's or language server's refactoring tools for renames and moves.
5. Run the repo's formatter and linters. They settle style questions, so don't
   debate them.

## Smells to look for

Long function or file · deep nesting · long parameter list · boolean flag
argument · vague names · magic numbers · duplicated logic · comment explaining
*what* · dead or commented-out code · swallowed exception · a class that only
holds data plus a separate class that does everything to it · a change that
needs edits in many unrelated places.

## Checklist

- [ ] Follows the codebase's existing conventions
- [ ] Every name says what the thing is; no magic numbers
- [ ] Each function does one thing; guard clauses instead of deep nesting
- [ ] Errors are specific, handled or propagated with context, never swallowed
- [ ] Comments explain why; no dead or commented-out code; no ownerless TODOs
- [ ] No duplicated knowledge; no premature abstraction
- [ ] Tests are readable, one behaviour each, and pass
- [ ] Formatter and linters are clean
