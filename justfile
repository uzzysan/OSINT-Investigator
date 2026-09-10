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

# Dependency-audit: licences, RustSec advisories, banned/duplicate crates and
# dependency sources — see deny.toml for the actual policy. Needs network
# access on first run (or a stale local cache) to fetch the RustSec advisory
# database; CI runs this via EmbarkStudios/cargo-deny-action, which caches it.
audit:
    cargo deny check

# Mechanically verify the one-way crate dependency direction (see
# scripts/check-deps.py and 03-architecture.md section 3.3).
check-deps:
    python3 scripts/check-deps.py
