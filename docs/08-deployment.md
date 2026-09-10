# 8. Deployment

The requirement: clone the repository onto a dedicated machine, run one script,
end up with a service listening on a free port that a human or an AI agent can
reach. No manual configuration, no database server, no container runtime.

## 8.1 The happy path

```bash
git clone https://…/vesica.git
cd vesica
./deploy/install.sh
```

Output:

```
Vesica — installer

  ✔ Detected Ubuntu 24.04 (x86_64)
  ✔ Rust toolchain 1.8x present
  ✔ Node 22 present
  ✔ Building web assets            (38s)
  ✔ Building vesica --release      (2m41s)
  ✔ Port 8787 is free
  ✔ Wrote .env
  ✔ Created /var/lib/vesica
  ✔ Applied 14 migrations
  ✔ Installed systemd unit vesica.service
  ✔ Service started and healthy

  Web UI      http://127.0.0.1:8787
  MCP (HTTP)  http://127.0.0.1:8787/mcp
  MCP (stdio) /opt/vesica/bin/vesica mcp
  API docs    http://127.0.0.1:8787/api/docs

  Data        /var/lib/vesica
  Logs        journalctl -u vesica -f
```

## 8.2 What the installer does

1. **Preflight** — detect OS, architecture, available RAM and disk. Refuse
   politely and explain if unsupported.
2. **Toolchain** — check for `cargo` and `node`. If missing, offer to install
   Rust via `rustup` and Node via the distro or `fnm`, honouring `--no-install`
   for air-gapped hosts. If a prebuilt release binary matching the platform is
   present in `dist/`, skip the build entirely.
3. **Build** — `npm ci && npm run build` in `web/`, then
   `cargo build --release -p vesica`. Assets are embedded into the binary.
4. **Port selection** — read `VESICA_PORT` from `.env` if it exists, else start at
   8787 and scan upward for the first free port, skipping the IANA registered
   ranges in use. The chosen port is written to `.env` and to `runtime/port`.
5. **Configuration** — copy `.env.example` to `.env` if absent, fill in the
   chosen port and data paths, generate an API token if `--auth=token`. Never
   overwrite an existing `.env`; if one exists, the script diffs it against the
   example and reports new keys.
6. **Data directory** — create `$VESICA_DATA_DIR` (default
   `/var/lib/vesica`) with `0750` and a dedicated system user
   `vesica`.
7. **Migrate** — `vesica migrate`.
8. **Service** — install and enable a systemd unit, start it, poll `/ready`
   until healthy or fail with the last 50 log lines.
9. **Report** — print the URLs and next steps.

Flags: `--port N`, `--bind ADDR`, `--data-dir PATH`, `--auth solo|token`,
`--no-service` (run in the foreground instead), `--no-install` (fail rather than
install toolchains), `--dry-run`.

## 8.3 Free-port discovery

```
candidate = VESICA_PORT or 8787
while candidate < 8900:
    if bind(VESICA_BIND_ADDR, candidate) succeeds: use it
    candidate += 1
```

The bind test is done by the installer and repeated by the service at startup;
if the configured port is taken at boot the service logs loudly and exits rather
than silently moving, because a moving port breaks agent configuration. Moving is
an installer-time decision only.

## 8.4 systemd unit

```ini
[Unit]
Description=Vesica
After=network-online.target
Wants=network-online.target

[Service]
Type=exec
User=vesica
Group=vesica
WorkingDirectory=/opt/vesica
EnvironmentFile=/opt/vesica/.env
ExecStart=/opt/vesica/bin/vesica serve
Restart=on-failure
RestartSec=3
StateDirectory=vesica

# hardening
NoNewPrivileges=true
PrivateTmp=true
PrivateDevices=true
ProtectSystem=strict
ProtectHome=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
ReadWritePaths=/var/lib/vesica
LockPersonality=true
MemoryDenyWriteExecute=true

[Install]
WantedBy=multi-user.target
```

## 8.5 Updating

```bash
cd /opt/vesica
./deploy/update.sh
```

Pulls, backs up the database, rebuilds, runs migrations, restarts, verifies
`/ready`, and rolls back to the previous binary and database if the health check
fails. The previous binary is kept as `bin/vesica.prev`.

## 8.6 Backups

`deploy/backup.sh` (and `vesica backup`) produces a consistent snapshot using
SQLite's online backup API — safe while the service is running — plus a manifest
of blob hashes and, optionally, a tar of the blob directory.

```
data/backups/2026-09-08T02-00-00/
  vesica.sqlite
  blobs.manifest.jsonl
  blobs.tar.zst          # when --with-blobs
  VERSION
```

Nightly via a systemd timer when `VESICA_BACKUP_ENABLED=true`, retaining
`VESICA_BACKUP_KEEP` snapshots. Restore is: stop the service, copy the file back,
restore blobs, start.

Because the whole state is one file plus one directory, offsite backup is
whatever the operator already uses — `restic`, `rclone`, a cron `scp`.

## 8.7 Reverse proxy and TLS

The service speaks plain HTTP and does not terminate TLS. For anything beyond
localhost, put Caddy or nginx in front. `deploy/caddy/Caddyfile.example`:

```
osint.example.org {
    reverse_proxy 127.0.0.1:8787
    encode zstd gzip
}
```

Caddy is suggested because it gets a certificate without being asked twice, and
this is a tool people want running, not administering.

## 8.8 Docker (secondary)

A `Dockerfile` and `compose.yml` are provided for operators who prefer
containers. The image is a multi-stage build ending in a distroless base with
one binary and one volume. This is a convenience path, not the supported one —
the native install is what the documentation and the troubleshooting guide
assume.

## 8.9 Platform support

| Platform | Status |
| --- | --- |
| Debian 12+, Ubuntu 22.04+ (x86-64, arm64) | Primary. Tested by CI. |
| Fedora / RHEL 9+ | Supported, less tested |
| Alpine | Supported via musl target |
| macOS (Apple Silicon, Intel) | Development; `install.sh` uses launchd instead of systemd |
| Windows | Via WSL2 only. Native Windows is not a target. |
| Raspberry Pi 4/5 (arm64) | Works; expect slower builds — use the prebuilt binary |

## 8.10 Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| `install.sh` fails at the Rust build with OOM | Under 2 GB RAM | Use a prebuilt binary from `dist/`, or add swap |
| Service starts then exits immediately | Configured port taken | `ss -ltnp \| grep <port>`, change `VESICA_PORT`, restart |
| `/ready` returns 503 | Data directory not writable | Check ownership of `/var/lib/vesica` |
| Agent cannot reach `/mcp` | `VESICA_BIND_ADDR=127.0.0.1` | Bind to the LAN address and put a proxy in front |
| Database locked errors | Another `vesica` holds the file | Only one process per database file |
