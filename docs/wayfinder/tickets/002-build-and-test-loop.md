---
id: 002
title: Build and test loop location
type: grilling
status: resolved
blocked_by: []
assignee: claude
---

## Question

Where does `cargo build` actually run during development?

**What we know.** The project lives at `C:\Kodzenie\1_Projekty\Vesica`
on the Windows machine `rafal-y540`. The agent reaches that folder through a
Linux VM that has Node 22, Python 3.10, git and network access — but **no Rust
toolchain and no Docker**. The cloud container the agent also has can install
anything, but its filesystem is separate from the project folder, and moving
files between the two is slow and drift-prone.

So there are three candidate loops, and this is the difference between a fast
inner loop and a painful one:

- **A — Install Rust in the connected Linux VM.** Edit and build in the same
  place the files live. `rustup` reaches `static.rust-lang.org` from there, so
  this is likely to work. Downside: each shell call there has a ~45-second
  ceiling, so a cold `cargo build --release` must be split across calls or run
  detached with a log.
- **B — Build on Windows directly.** The developer has `.cargo` and `.rustup` in
  their home directory, so a toolchain already exists on the host. The agent has
  no direct Windows shell, so builds would be the developer's action, with the
  agent writing code and reading results. Slowest feedback for the agent, most
  natural for the human.
- **C — Mirror into the cloud container.** Fast, unrestricted builds, but two
  copies of the tree to keep in sync and a real risk of overwriting the
  developer's edits.
- **D — WSL2 on the host.** A `Pengwin` directory exists in the home folder, so
  WSL is installed. A WSL toolchain would give a fast native Linux build over the
  same files, but the agent's access to it is unclear.

**What to decide**: which loop is primary, and whether long builds run detached
with a log file the agent polls.

This blocks nothing conceptually but shapes every implementation session from M0
onward, so it is worth settling first.

## Answer

**C, with A as the fallback and the verification bench.**

Agents build and test in the **cloud container**: cargo 1.95 is already installed
there, it has 7 GB of RAM against the device VM's 3.9 GB, a real shell with no
45-second ceiling, and 30 GB free. The device folder stays the **single source of
truth**: at the end of each ticket the changed files are written into it and
committed there, so there is one sync boundary per ticket rather than per edit,
and never a second copy drifting between them.

The Rust toolchain was also installed in the device VM. It is not the inner loop,
but it gives a second Linux box for exercising `deploy/install.sh` in something
closer to a clean target, which ticket 05 needs.

Long builds anywhere run detached with output to a log that gets polled, since a
cold workspace build exceeds any single shell call.
