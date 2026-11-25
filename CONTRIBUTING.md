# Contributing

## Before you open a change

Run the same checks as CI:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
```

## Style rules

- Formatting is `rustfmt` defaults. Clippy warnings are errors.
- No comments in source, except `// SAFETY:` comments on `unsafe` blocks. Documentation goes in `docs/`.
- Every `unsafe` block has a `// SAFETY:` comment that states the invariant it relies on.
- Each crate has a README that states its boundary and its owner area.
- Dependencies follow the rules in `docs/decisions.md` and the plan's dependency rule. `weave-heart` depends on no other project crate.

## Changes

- Every merged change includes its tests.
- Efficiency changes include before and after benchmark results on both reference tiers (see the plan, Section 9).
- Decisions that change a recorded choice get an entry in `docs/decisions.md`.
