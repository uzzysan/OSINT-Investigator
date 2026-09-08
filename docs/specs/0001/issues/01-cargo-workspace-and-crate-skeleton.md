# 01: Cargo workspace and crate skeleton

**What to build:** a maintainer can clone the repository, run one command, and get
a green build with every crate boundary already in place. Nothing runs yet, but
the shape of the system is established and the compiler enforces it from here on.

The eight crates from the architecture document exist as compiling stubs, wired
in the one-way dependency direction that document specifies. `vesica-core` sits at
the bottom with no internal dependencies and no I/O crates — a mistake that
would erode that boundary fails the build rather than passing review.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] Workspace builds clean with all eight crates present as stubs
- [ ] `vesica-core` has no internal dependencies and no I/O crates in its manifest
- [ ] The one-way dependency direction is verified mechanically, not by convention
- [ ] Every crate carries `#![forbid(unsafe_code)]`
- [ ] Shared dependency versions are declared once at the workspace level
- [ ] A `just` recipe exists for each of dev, build, test, lint, fmt and audit
- [ ] `cargo fmt --check` and `cargo clippy -D warnings` both pass
- [ ] `cargo test --workspace` runs and reports zero tests without failing
- [ ] Rust edition and minimum supported toolchain version are pinned
