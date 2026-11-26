# Contributing

## Before you open a change

Run the checks CI runs:

```
scripts/check.sh          # Linux and macOS
scripts/check.ps1         # Windows
```

Or run them one by one:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
```

Use `--locked` so the build matches `Cargo.lock`. If a dependency change is intended, update the lock file in the same change.

## Style rules

- Formatting is `rustfmt` defaults, with LF line endings (see `.gitattributes`). Clippy warnings are errors.
- No comments in source, except `// SAFETY:` comments on `unsafe` blocks. Documentation goes in `docs/` or a crate README, and can be included in rustdoc with `#[doc = include_str!(...)]`.
- Every `unsafe` block has a `// SAFETY:` comment that states the invariant it relies on.
- Each crate has a README that states its boundary and its owner area.
- `weave-heart` depends on no other project crate. `weave-macros` depends on no project crate.

## Changes

- Every merged change includes its tests.
- Public behaviour that scripts depend on gets a test that names the rule it checks.
- Efficiency changes include before and after numbers on both reference tiers (see the plan, Section 9).
- Every benchmark run is logged in `docs/benchmarks.md` with its command, environment, and result (decision D-77). Add the entry in the same change that makes the claim.
- Decisions that change a recorded choice get an entry in `docs/decisions.md`.
