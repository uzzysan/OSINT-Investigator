# 04: Health, readiness and the embedded web scaffold

**What to build:** an operator starts the service and can ask it two different
questions: are you alive, and are you able to serve? A supervisor uses the first
to decide whether to restart; a deployment script uses the second to decide
whether the install succeeded.

Readiness means something specific and is tested for it: migrations applied, the
database writable, the blob directory writable. A service whose data directory
has the wrong ownership reports itself unready rather than alive-and-broken.

This ticket also proves the frontend delivery mechanism end to end. A placeholder
page compiles into the binary and is served from it, so there is never a static
directory that disagrees with the build. A development mode proxies to the Rust
server so UI work in a later milestone is not gated on a full rebuild per change.

**Blocked by:** 03

**Status:** ready-for-agent

- [ ] The server binds the configured address and port and serves requests
- [ ] A liveness endpoint answers without touching the database
- [ ] A readiness endpoint verifies migrations, database writability and blob directory writability
- [ ] Readiness fails with a diagnostic body when the data directory is not writable
- [ ] The binary exits with a clear error when the configured port is already in use, rather than moving
- [ ] The web scaffold builds and its output is embedded in the binary
- [ ] The embedded page is served at the root with correct content types
- [ ] A development mode hot-reloads the frontend against the running server
- [ ] Graceful shutdown drains in-flight requests and closes the database cleanly
