# Rust Style

This document defines the Rust engineering bar for Gum.

Gum is infrastructure software. The Rust code should feel:
- safe
- idiomatic
- explicit
- maintainable
- boring in the good way

The goal is not cleverness.
The goal is code that is easy to trust, debug, and extend.

## Core Rule

Write idiomatic, safe Rust by default.

Avoid:
- `unsafe`
- panic-driven control flow
- clever abstractions
- hidden behavior
- premature optimization

Prefer:
- explicit errors
- readable ownership
- straightforward control flow
- small focused functions
- stable, maintainable patterns

## Safety

- Safe Rust is the default.
- `unsafe` is forbidden unless there is no practical safe alternative.
- Any `unsafe` usage must be:
  - documented
  - justified
  - narrowly scoped
  - reviewed with extra scrutiny

If a problem can be solved with safe Rust, use safe Rust.

## Error Handling

- Use `Result` for fallible paths.
- Do not hide errors behind silent fallback behavior unless the fallback is explicitly intended.
- Prefer returning precise errors over crashing.
- Avoid `unwrap()` and `expect()` in production code.

Allowed uses of `unwrap()` / `expect()`:
- tests
- tiny development-only setup paths where failure is unrecoverable and obvious

Not allowed in:
- request handling
- scheduler logic
- execution paths
- store/query code
- runner logic

## Idiomatic Rust

- Prefer ownership and borrowing patterns that are easy to read.
- Avoid complicated lifetime tricks unless they are clearly necessary.
- Prefer standard library types and well-understood crate APIs.
- Prefer enums and structs that model the domain clearly.
- Match exhaustively on important states.
- Keep trait surfaces small and concrete.
- Use generics where they clearly reduce duplication, not to look elegant.

## Control Flow

- Prefer obvious code over dense code.
- Use iterators when they make the code clearer.
- Use loops when they make the code clearer.
- Avoid deeply nested branching where a small helper function would clarify intent.
- Keep async flows bounded and understandable.

The reader should be able to scan a function and understand:
- what it does
- what can fail
- what state changes happen

## Data and State

- Model important state transitions explicitly.
- Prefer strong types over ad hoc strings where practical.
- Avoid passing loosely structured data through many layers when a real struct would be clearer.
- Keep state mutations narrow and easy to reason about.

For Gum specifically:
- runs
- attempts
- leases
- deploys
- scheduler state

should always be represented with explicit, readable data flow.

## Abstractions

- Do not build abstractions ahead of demonstrated need.
- Avoid “framework-y” internal code.
- Prefer one more plain function over one more clever trait.
- Prefer concrete types unless polymorphism is clearly buying simplicity.

If an abstraction makes the code harder to trace, it is the wrong abstraction.

## Concurrency and Async

- Keep async boundaries clear.
- Avoid spawning work casually.
- Prefer bounded, explicit concurrency.
- Make ownership at async boundaries obvious.
- Avoid hidden background behavior unless the component is explicitly a background service.

## Comments

- Write comments for intent, invariants, and non-obvious reasoning.
- Do not write comments that restate the code.
- When a piece of code looks unusual, explain why it must be that way.

Good comment:
- why duplicate scheduled runs are prevented at this boundary

Bad comment:
- increment the counter

## Performance

- Optimize after the path is correct and understandable.
- Do not trade clarity for hypothetical performance wins.
- If performance-motivated code is less obvious, leave a short comment explaining the tradeoff.

## Gum-Specific Guidance

For Gum, prefer code that makes these things obvious:
- how runs are created
- how attempts are leased
- how retries requeue
- how schedules produce runs
- how logs are attached
- how failures propagate

The control plane should read like systems code with clear state transitions, not application glue with hidden magic.

## Litmus Test

Before merging Rust code, ask:

1. Is this safe Rust?
2. Is this the simplest code that correctly solves the problem?
3. Can another engineer debug this quickly?
4. Are errors explicit and non-panicking?
5. Does this look like maintainable infrastructure code?

If the answer to any of those is no, rewrite it.
