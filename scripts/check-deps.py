#!/usr/bin/env python3
"""Mechanically enforce the one-way crate dependency direction from
03-architecture.md section 3.3.

Why Python: the check only needs to parse TOML and compare sets of names —
no cargo-specific metadata (features, target cfgs) matters to it. The
standard library's `tomllib` (3.11+) does this with zero extra dependencies,
which keeps the check itself outside the dependency graph it's auditing —
a shell script would need a TOML parser anyway (yq/toml-cli aren't
guaranteed to be installed), and a `cargo` subcommand would mean compiling
something just to check that nothing needs compiling yet. Wired in as
`just check-deps`.

What it enforces, both hard requirements of ticket 0001-01:
  1. vesica-core has no internal (vesica-*) dependency and no I/O crate
     dependency, in any dependency table (normal, dev, build).
  2. Every internal (vesica-*) dependency edge that exists anywhere in the
     workspace is one the architecture doc allows. An edge that exists but
     is not on the allow-list fails the build.

This does not check that every *allowed* edge is present — a crate is free
to not yet depend on something it will eventually need.
"""

from __future__ import annotations

import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent

# The one-way dependency direction, transcribed verbatim from
# 03-architecture.md section 3.3:
#
#   vesica -> vesica-api, vesica-mcp
#   vesica-api, vesica-mcp -> vesica-report, vesica-import, vesica-resolve, vesica-store
#   vesica-resolve, vesica-store, vesica-import, vesica-report -> vesica-core
#   vesica-core -> nothing internal
ALLOWED_INTERNAL_EDGES: dict[str, set[str]] = {
    "vesica-core": set(),
    "vesica-store": {"vesica-core"},
    "vesica-resolve": {"vesica-core"},
    "vesica-import": {"vesica-core"},
    "vesica-report": {"vesica-core"},
    "vesica-api": {"vesica-core", "vesica-report", "vesica-import", "vesica-resolve", "vesica-store"},
    "vesica-mcp": {"vesica-core", "vesica-report", "vesica-import", "vesica-resolve", "vesica-store"},
    # The binary is the COMPOSITION ROOT. Its whole job is to construct the
    # store, hand it to the API and MCP layers, and run CLI subcommands that
    # act on it directly (`vesica migrate`, `backup`, `check`). Forcing those
    # through vesica-api would be layering theatre: it would add a passthrough
    # for no isolation, since the binary already links everything anyway.
    # The layering rule that matters is that no LIBRARY crate may reach
    # sideways or upwards, and that is still enforced above.
    "vesica": {"vesica-core", "vesica-api", "vesica-mcp", "vesica-store"},
}

# Crates that perform I/O (network, filesystem, database, async runtime).
# vesica-core may depend on none of these — see AGENT-BRIEF.md, "Hard rules"
# #2. Matched by exact name or by prefix, so a transitive-looking rename
# (e.g. a hypothetical `tokio-fs`) is still caught.
IO_CRATE_EXACT = {
    "sqlx",
    "tokio",
    "axum",
    "tower",
    "tower-http",
    "hyper",
    "reqwest",
    "rust-embed",
    "rusqlite",
    "mio",
    "socket2",
    "async-std",
    "smol",
    "std-fs",
    "notify",
    "walkdir",
}
IO_CRATE_PREFIXES = ("tokio", "sqlx", "hyper", "axum", "async-")

DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


def is_io_crate(name: str) -> bool:
    return name in IO_CRATE_EXACT or name.startswith(IO_CRATE_PREFIXES)


def load_manifest(path: pathlib.Path) -> dict:
    with path.open("rb") as f:
        return tomllib.load(f)


def crate_deps(manifest: dict) -> set[str]:
    names: set[str] = set()
    for table in DEP_TABLES:
        names.update(manifest.get(table, {}).keys())
    return names


def main() -> int:
    root_manifest_path = ROOT / "Cargo.toml"
    root_manifest = load_manifest(root_manifest_path)
    members = root_manifest.get("workspace", {}).get("members", [])
    if not members:
        print("check-deps: no workspace members found in Cargo.toml", file=sys.stderr)
        return 1

    failures: list[str] = []
    seen_crates: set[str] = set()

    for member in members:
        manifest_path = ROOT / member / "Cargo.toml"
        if not manifest_path.exists():
            failures.append(f"{member}: no Cargo.toml found")
            continue

        manifest = load_manifest(manifest_path)
        crate_name = manifest.get("package", {}).get("name")
        if not crate_name:
            failures.append(f"{member}: [package].name missing")
            continue
        seen_crates.add(crate_name)

        deps = crate_deps(manifest)
        internal_deps = {d for d in deps if d.startswith("vesica")} - {crate_name}
        io_deps = {d for d in deps if is_io_crate(d)}

        # Rule 1: vesica-core is I/O-free and depends on no workspace crate.
        if crate_name == "vesica-core":
            if internal_deps:
                failures.append(
                    f"vesica-core: forbidden internal dependency on "
                    f"{sorted(internal_deps)} (vesica-core must depend on no "
                    f"other workspace crate)"
                )
            if io_deps:
                failures.append(
                    f"vesica-core: forbidden I/O crate dependency "
                    f"{sorted(io_deps)} (vesica-core must perform no I/O)"
                )

        # Rule 2: every internal edge that exists must be on the allow-list.
        allowed = ALLOWED_INTERNAL_EDGES.get(crate_name)
        if allowed is None:
            failures.append(
                f"{crate_name}: not a recognised workspace crate — add it to "
                f"ALLOWED_INTERNAL_EDGES in scripts/check-deps.py"
            )
        else:
            disallowed = internal_deps - allowed
            if disallowed:
                failures.append(
                    f"{crate_name}: depends on {sorted(disallowed)}, which is "
                    f"not on the allowed edge list {sorted(allowed) or '{}'} "
                    f"from 03-architecture.md section 3.3"
                )

    expected_crates = set(ALLOWED_INTERNAL_EDGES.keys())
    missing_crates = expected_crates - seen_crates
    if missing_crates:
        failures.append(f"workspace is missing expected crate(s): {sorted(missing_crates)}")

    if failures:
        print("check-deps: FAILED", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(f"check-deps: OK ({len(seen_crates)} crates, dependency direction holds)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
