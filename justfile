# Vesica developer recipes. Run `just` with no arguments to list them.

set shell := ["bash", "-uc"]

# Run the server with a debug build (once `vesica serve` exists).
dev:
    cargo run -p vesica -- serve

# Build every crate in the workspace.
build:
    cargo build --workspace

# Run every test in the workspace.
test:
    cargo test --workspace

# Lint: clippy across every target, warnings are errors.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Format check: fails without rewriting anything (use `cargo fmt` to fix).
fmt:
    cargo fmt --all -- --check

# Dependency-audit: a stub until ticket 05 (CI) wires `cargo-audit` (or
# `cargo-deny`) into a scheduled job with an advisory-db cache. Running an
# audit here today would need network access to fetch the RustSec advisory
# database on every invocation, which this recipe intentionally does not
# assume. Once ticket 05 lands, replace this with `cargo audit`.
audit:
    @echo "audit: stub — ticket 05 (CI) wires 'cargo audit' against the RustSec advisory database. Not run here."

# Mechanically verify the one-way crate dependency direction (see
# scripts/check-deps.py and 03-architecture.md section 3.3).
check-deps:
    python3 scripts/check-deps.py
