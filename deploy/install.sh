#!/usr/bin/env bash
# Vesica installer — see 08-deployment.md section 8.2 for the step-by-step
# contract this implements, and section 8.3/8.4 for the free-port and
# systemd-unit details.
#
#   ./deploy/install.sh [flags]
#
# Flags:
#   --port N            Preferred port (default: from an existing .env, else 8787)
#   --bind ADDR          Bind address (default: from an existing .env, else 127.0.0.1)
#   --data-dir PATH      Data directory (default: /var/lib/vesica)
#   --auth solo|token    Access profile for a freshly written .env (default: solo)
#   --no-service         Prepare everything but do not install/start systemd;
#                         print the foreground command instead
#   --no-install         Fail rather than installing a missing toolchain
#   --dry-run            Print what would happen; change nothing
#   -h, --help            Show this help and exit
#
# Run as root (except --dry-run): it creates a system user, writes under
# /opt and /var/lib, and installs a systemd unit.
set -euo pipefail

# ---------------------------------------------------------------------------
# Constants — see AGENT-BRIEF.md "Naming": these are settled, not guesses.
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readonly REPO_ROOT
readonly SERVICE_NAME="vesica"
readonly SERVICE_USER="vesica"
readonly SERVICE_GROUP="vesica"
readonly INSTALL_DIR="/opt/vesica"
readonly DEFAULT_DATA_DIR="/var/lib/vesica"
readonly DEFAULT_BIND_ADDR="127.0.0.1"
readonly DEFAULT_PORT=8787
readonly PORT_SCAN_LIMIT=8900
readonly ENV_EXAMPLE="${REPO_ROOT}/.env.example"
readonly SYSTEMD_UNIT_SRC="${SCRIPT_DIR}/systemd/vesica.service"
readonly SYSTEMD_UNIT_DST="/etc/systemd/system/vesica.service"
readonly READY_TIMEOUT_SECS=60
readonly LOG_TAIL_LINES=50

# ---------------------------------------------------------------------------
# Flags
# ---------------------------------------------------------------------------
OPT_PORT=""
OPT_BIND=""
OPT_DATA_DIR=""
OPT_AUTH="solo"
OPT_NO_SERVICE=false
OPT_NO_INSTALL=false
OPT_DRY_RUN=false

usage() {
    sed -n '2,26p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --port)
                OPT_PORT="${2:?--port needs a value}"
                shift 2
                ;;
            --bind)
                OPT_BIND="${2:?--bind needs a value}"
                shift 2
                ;;
            --data-dir)
                OPT_DATA_DIR="${2:?--data-dir needs a value}"
                shift 2
                ;;
            --auth)
                OPT_AUTH="${2:?--auth needs a value}"
                if [[ "${OPT_AUTH}" != "solo" && "${OPT_AUTH}" != "token" ]]; then
                    die "--auth must be 'solo' or 'token', got '${OPT_AUTH}'"
                fi
                shift 2
                ;;
            --no-service) OPT_NO_SERVICE=true; shift ;;
            --no-install) OPT_NO_INSTALL=true; shift ;;
            --dry-run) OPT_DRY_RUN=true; shift ;;
            -h|--help) usage; exit 0 ;;
            *) die "unknown flag '$1' — see --help" ;;
        esac
    done
}

# ---------------------------------------------------------------------------
# Logging — mirrors 08-deployment.md's "✔ Detected ..." sample output.
# ---------------------------------------------------------------------------
step_ok()   { printf '  \xe2\x9c\x94 %s\n' "$1"; }
step_info() { printf '    %s\n' "$1"; }
step_warn() { printf '  \xe2\x9a\xa0 %s\n' "$1" >&2; }
die()       { printf '  \xe2\x9c\x98 %s\n' "$1" >&2; exit 1; }

section() { printf '\n%s\n' "$1"; }

dry() {
    if "${OPT_DRY_RUN}"; then
        printf '    (dry-run) would run: %s\n' "$*"
        return 0
    fi
    "$@"
}

# ---------------------------------------------------------------------------
# 1. Preflight — OS, arch, RAM, disk. Refuse politely if unsupported.
#    See 08-deployment.md section 8.9 for the support matrix.
# ---------------------------------------------------------------------------
OS_KIND=""      # linux | darwin
OS_ID=""        # ubuntu, debian, fedora, rhel, alpine, ... (linux only)
OS_VERSION=""
ARCH=""         # x86_64 | aarch64

detect_os_arch() {
    local uname_s uname_m
    uname_s="$(uname -s)"
    uname_m="$(uname -m)"

    case "${uname_s}" in
        Linux) OS_KIND="linux" ;;
        Darwin) OS_KIND="darwin" ;;
        MINGW*|MSYS*|CYGWIN*)
            die "Native Windows is not a target (see 08-deployment.md section 8.9). Run this installer from inside WSL2 instead."
            ;;
        *) die "Unrecognised OS '${uname_s}'. Supported: Debian/Ubuntu, Fedora/RHEL, Alpine, macOS, or Windows via WSL2." ;;
    esac

    case "${uname_m}" in
        x86_64|amd64) ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *) die "Unsupported architecture '${uname_m}'. Vesica ships x86_64 and aarch64 builds only." ;;
    esac

    if [[ "${OS_KIND}" == "linux" ]]; then
        if [[ -r /etc/os-release ]]; then
            # shellcheck source=/dev/null
            source /etc/os-release
            OS_ID="${ID:-unknown}"
            OS_VERSION="${VERSION_ID:-0}"
        else
            OS_ID="unknown"
            OS_VERSION="0"
        fi
    else
        OS_ID="macos"
        OS_VERSION="$(sw_vers -productVersion 2>/dev/null || echo 0)"
    fi
}

version_ge() { # version_ge A B -> true if A >= B (dotted version strings)
    [[ "$1" == "$2" ]] && return 0
    local lower
    lower="$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)"
    [[ "${lower}" == "$2" ]]
}

check_platform_supported() {
    case "${OS_ID}" in
        ubuntu)
            version_ge "${OS_VERSION}" "22.04" || \
                die "Ubuntu ${OS_VERSION} is below the supported floor (22.04). See 08-deployment.md section 8.9."
            ;;
        debian)
            version_ge "${OS_VERSION}" "12" || \
                die "Debian ${OS_VERSION} is below the supported floor (12). See 08-deployment.md section 8.9."
            ;;
        fedora)
            step_warn "Fedora is supported but less tested than Debian/Ubuntu."
            ;;
        rhel|rocky|almalinux|centos)
            version_ge "${OS_VERSION}" "9" || \
                die "RHEL-family ${OS_VERSION} is below the supported floor (9). See 08-deployment.md section 8.9."
            step_warn "RHEL-family is supported but less tested than Debian/Ubuntu."
            ;;
        alpine)
            step_warn "Alpine is supported via the musl target; this script assumes a glibc toolchain path and may need manual toolchain steps."
            ;;
        macos)
            step_warn "macOS is a development target (08-deployment.md section 8.9): --no-service is effectively required here, since this script installs a systemd unit and macOS has none. Use launchd yourself, or run 'vesica serve' directly."
            ;;
        *)
            step_warn "Unrecognised Linux distribution '${OS_ID}' — continuing, but this platform isn't in 08-deployment.md's support matrix. If the build or service steps fail, that's why."
            ;;
    esac
}

check_resources() {
    local mem_kb=0 disk_kb=0

    if [[ "${OS_KIND}" == "linux" && -r /proc/meminfo ]]; then
        mem_kb="$(awk '/^MemTotal:/ {print $2}' /proc/meminfo)"
    elif [[ "${OS_KIND}" == "darwin" ]]; then
        mem_kb=$(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1024 ))
    fi

    local check_dir
    check_dir="$(dirname "${OPT_DATA_DIR:-${DEFAULT_DATA_DIR}}")"
    [[ -d "${check_dir}" ]] || check_dir="/"
    disk_kb="$(df -Pk "${check_dir}" 2>/dev/null | awk 'NR==2{print $4}')"
    disk_kb="${disk_kb:-0}"

    if (( mem_kb > 0 && mem_kb < 524288 )); then
        die "Only $(( mem_kb / 1024 )) MiB RAM detected; Vesica needs at least 512 MiB to run, more to build from source. Use a bigger machine, or build elsewhere and drop the binary in dist/."
    elif (( mem_kb > 0 && mem_kb < 2097152 )); then
        step_warn "$(( mem_kb / 1024 )) MiB RAM detected. A from-source release build can OOM under 2 GiB (see 08-deployment.md's troubleshooting table) — put a prebuilt binary in dist/, or add swap, if the build step below fails."
    fi

    if (( disk_kb > 0 && disk_kb < 512000 )); then
        die "Only $(( disk_kb / 1024 )) MiB free disk at ${check_dir}; Vesica's build and data need at least 500 MiB. Free up space and re-run."
    elif (( disk_kb > 0 && disk_kb < 3145728 )); then
        step_warn "$(( disk_kb / 1024 )) MiB free disk at ${check_dir}. A release build plus toolchains can use a few GiB; watch this."
    fi
}

preflight() {
    section "Preflight"
    detect_os_arch
    check_platform_supported
    step_ok "Detected ${OS_ID} ${OS_VERSION} (${ARCH})"
    check_resources
    step_ok "RAM and disk are sufficient to proceed"

    if [[ "${EUID}" -ne 0 ]] && ! "${OPT_DRY_RUN}"; then
        die "Run as root (this script writes to ${INSTALL_DIR}, ${OPT_DATA_DIR:-${DEFAULT_DATA_DIR}}, and /etc/systemd/system — try: sudo $0 <same flags>). Use --dry-run to preview without root."
    fi
}

# ---------------------------------------------------------------------------
# 2. Toolchain — cargo and node. Install if missing, unless --no-install.
# ---------------------------------------------------------------------------
NEED_BUILD=true  # flipped off if a matching dist/ binary is found (step 3)

have_cmd() { command -v "$1" >/dev/null 2>&1; }

install_rust() {
    if "${OPT_NO_INSTALL}"; then
        die "cargo not found and --no-install was given. Install Rust yourself (https://rustup.rs) and re-run."
    fi
    step_info "Installing Rust via rustup..."
    dry curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh
    if ! "${OPT_DRY_RUN}"; then
        sh /tmp/rustup-init.sh -y --default-toolchain none --profile minimal
        # shellcheck source=/dev/null
        source "${HOME}/.cargo/env"
    fi
}

install_node() {
    if "${OPT_NO_INSTALL}"; then
        die "node not found and --no-install was given. Install Node 22 yourself (https://nodejs.org, or fnm) and re-run."
    fi
    step_info "Installing Node via NodeSource..."
    if have_cmd apt-get; then
        dry bash -c 'curl -fsSL https://deb.nodesource.com/setup_22.x | bash -'
        dry apt-get install -y nodejs
    elif have_cmd dnf; then
        dry bash -c 'curl -fsSL https://rpm.nodesource.com/setup_22.x | bash -'
        dry dnf install -y nodejs
    elif have_cmd apk; then
        dry apk add --no-cache nodejs npm
    else
        die "No supported package manager found to install Node automatically. Install Node 22 yourself and re-run, or pass --no-install to fail fast next time."
    fi
}

detect_toolchains() {
    section "Toolchain"

    if ! "${NEED_BUILD}"; then
        step_ok "Prebuilt binary present in dist/ — skipping toolchain checks (nothing to compile)"
        return
    fi

    if have_cmd cargo; then
        step_ok "Rust toolchain $(cargo --version | awk '{print $2}') present"
    else
        install_rust
        have_cmd cargo || "${OPT_DRY_RUN}" || die "rustup install finished but cargo is still not on PATH — open a new shell and re-run."
        "${OPT_DRY_RUN}" || step_ok "Rust toolchain installed"
    fi

    if have_cmd node; then
        local node_major
        node_major="$(node --version | sed -E 's/^v([0-9]+).*/\1/')"
        step_ok "Node $(node --version) present"
        if (( node_major < 18 )); then
            step_warn "Node ${node_major} is older than this project has been built against (22). The web build may fail."
        fi
    else
        install_node
        "${OPT_DRY_RUN}" || step_ok "Node installed"
    fi
}

# ---------------------------------------------------------------------------
# 3. Build — npm ci && npm run build, then cargo build --release -p vesica.
#    Assets are embedded into the binary by vesica-api's rust-embed setup,
#    so the release binary is the entire runtime artifact.
# ---------------------------------------------------------------------------
DIST_BINARY=""

find_prebuilt_binary() {
    local candidate
    for candidate in \
        "${REPO_ROOT}/dist/vesica-${OS_KIND}-${ARCH}" \
        "${REPO_ROOT}/dist/${OS_KIND}-${ARCH}/vesica"
    do
        if [[ -x "${candidate}" ]]; then
            DIST_BINARY="${candidate}"
            NEED_BUILD=false
            return
        fi
    done
}

build() {
    section "Build"

    if ! "${NEED_BUILD}"; then
        step_ok "Using prebuilt binary ${DIST_BINARY#"${REPO_ROOT}"/}"
        return
    fi

    step_info "Building web assets..."
    dry bash -c "cd '${REPO_ROOT}/web' && npm ci && npm run build"
    "${OPT_DRY_RUN}" || step_ok "Built web assets"

    step_info "Building vesica --release (this can take a few minutes)..."
    dry bash -c "cd '${REPO_ROOT}' && cargo build --release -p vesica"
    "${OPT_DRY_RUN}" || step_ok "Built vesica --release"
}

resolved_binary_path() {
    if [[ -n "${DIST_BINARY}" ]]; then
        printf '%s\n' "${DIST_BINARY}"
    else
        # Respect CARGO_TARGET_DIR (or a .cargo/config.toml build.target-dir
        # picked up the same way cargo itself does) rather than assuming the
        # default ./target — CI and some local setups redirect it.
        printf '%s\n' "${CARGO_TARGET_DIR:-${REPO_ROOT}/target}/release/vesica"
    fi
}

# ---------------------------------------------------------------------------
# 4. Port selection — 08-deployment.md section 8.3. Only scanned on a first
#    install (no .env yet); an existing .env's VESICA_PORT is trusted as-is,
#    so re-running the installer never silently moves a configured port.
# ---------------------------------------------------------------------------
port_is_free() {
    local bind_addr="$1" port="$2"
    if have_cmd python3; then
        python3 - "${bind_addr}" "${port}" <<'PY' 2>/dev/null
import socket, sys
addr, port = sys.argv[1], int(sys.argv[2])
family = socket.AF_INET6 if ":" in addr else socket.AF_INET
s = socket.socket(family, socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try:
    s.bind((addr, port))
except OSError:
    sys.exit(1)
finally:
    s.close()
sys.exit(0)
PY
    else
        # No python3: approximate with a listening-socket check rather than
        # a real bind() attempt. Good enough to avoid the common case
        # (something already listening); a transient hold some other way
        # won't be caught, and the service's own startup check is the
        # backstop either way.
        if ss -ltn 2>/dev/null | awk '{print $4}' | grep -qE "[.:]${port}\$"; then
            return 1
        fi
        return 0
    fi
}

find_free_port() {
    local start="$1" bind_addr="$2" candidate="$1"
    while (( candidate < PORT_SCAN_LIMIT )); do
        if port_is_free "${bind_addr}" "${candidate}"; then
            printf '%s\n' "${candidate}"
            return 0
        fi
        candidate=$(( candidate + 1 ))
    done
    die "No free port found for ${bind_addr} between ${start} and ${PORT_SCAN_LIMIT}. Free one up, or pass --port to choose a different range."
}

existing_env_value() {
    # Prints VESICA_<key>'s value from an existing INSTALL_DIR/.env, or
    # nothing if absent.
    local key="$1" env_file="${INSTALL_DIR}/.env"
    [[ -f "${env_file}" ]] || return 0
    grep -E "^VESICA_${key}=" "${env_file}" | tail -n1 | cut -d= -f2- || true
}

RESOLVED_PORT=""
RESOLVED_BIND=""

select_port_and_bind() {
    section "Configuration"

    RESOLVED_BIND="${OPT_BIND:-$(existing_env_value BIND_ADDR)}"
    RESOLVED_BIND="${RESOLVED_BIND:-${DEFAULT_BIND_ADDR}}"

    local existing_port
    existing_port="$(existing_env_value PORT)"
    if [[ -n "${existing_port}" && -z "${OPT_PORT}" ]]; then
        RESOLVED_PORT="${existing_port}"
        step_ok "Using previously configured port ${RESOLVED_PORT} (from ${INSTALL_DIR}/.env; re-run with --port to pick a fresh one)"
        return
    fi

    local preferred="${OPT_PORT:-${DEFAULT_PORT}}"
    RESOLVED_PORT="$(find_free_port "${preferred}" "${RESOLVED_BIND}")"
    if [[ "${RESOLVED_PORT}" == "${preferred}" ]]; then
        step_ok "Port ${RESOLVED_PORT} is free"
    else
        step_ok "Port ${preferred} was taken; using ${RESOLVED_PORT} instead"
    fi
}

# ---------------------------------------------------------------------------
# 5. .env — copy from .env.example, filling in the chosen port/paths. Never
#    overwrites an existing .env: diffs it against the example instead and
#    reports any new keys.
# ---------------------------------------------------------------------------
write_env() {
    local env_file="${INSTALL_DIR}/.env" data_dir="$1"

    if [[ -f "${env_file}" ]]; then
        step_ok "${env_file} already exists — leaving it untouched"
        local example_keys existing_keys new_keys
        example_keys="$(grep -oE '^VESICA_[A-Z_]+=' "${ENV_EXAMPLE}" | sort -u)"
        existing_keys="$(grep -oE '^VESICA_[A-Z_]+=' "${env_file}" | sort -u)"
        new_keys="$(comm -23 <(printf '%s\n' "${example_keys}") <(printf '%s\n' "${existing_keys}"))"
        if [[ -n "${new_keys}" ]]; then
            step_warn "New configuration keys are available (not added — edit ${env_file} yourself):"
            while IFS= read -r key; do
                [[ -n "${key}" ]] || continue
                local comment default_line
                default_line="$(grep -F "${key}" "${ENV_EXAMPLE}")"
                comment="$(grep -B1 -F "${key}" "${ENV_EXAMPLE}" | head -n1)"
                [[ "${comment}" == "${default_line}" ]] && comment=""
                step_info "  ${default_line}${comment:+   # ${comment#\# }}"
            done <<< "${new_keys}"
        else
            step_ok "No new configuration keys since this .env was written"
        fi
        return
    fi

    step_info "Writing ${env_file}..."
    local token=""
    if [[ "${OPT_AUTH}" == "token" ]]; then
        if have_cmd openssl; then
            token="$(openssl rand -hex 32)"
        else
            token="$(head -c32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
        fi
    fi

    local public_host="${RESOLVED_BIND}"
    [[ "${public_host}" == "0.0.0.0" || "${public_host}" == "::" ]] && public_host="127.0.0.1"

    if "${OPT_DRY_RUN}"; then
        step_info "(dry-run) would write ${env_file} with port=${RESOLVED_PORT} bind=${RESOLVED_BIND} auth=${OPT_AUTH} data_dir=${data_dir}"
        return
    fi

    awk -v port="${RESOLVED_PORT}" \
        -v bind="${RESOLVED_BIND}" \
        -v public_url="http://${public_host}:${RESOLVED_PORT}" \
        -v db_path="${data_dir}/vesica.sqlite" \
        -v blob_dir="${data_dir}/blobs" \
        -v backup_dir="${data_dir}/backups" \
        -v auth="${OPT_AUTH}" \
        -v token="${token}" '
        BEGIN {
            overrides["VESICA_BIND_ADDR"] = bind
            overrides["VESICA_PORT"] = port
            overrides["VESICA_PUBLIC_URL"] = public_url
            overrides["VESICA_DB_PATH"] = db_path
            overrides["VESICA_BLOB_DIR"] = blob_dir
            overrides["VESICA_BACKUP_DIR"] = backup_dir
            overrides["VESICA_AUTH_MODE"] = auth
            overrides["VESICA_API_TOKEN"] = token
        }
        {
            eq = index($0, "=")
            if (eq > 0) {
                key = substr($0, 1, eq - 1)
                if (key in overrides) {
                    print key "=" overrides[key]
                    next
                }
            }
            print $0
        }
    ' "${ENV_EXAMPLE}" > "${env_file}"
    chmod 0640 "${env_file}"
    if [[ "${EUID}" -eq 0 ]] && id "${SERVICE_USER}" >/dev/null 2>&1; then
        chown "${SERVICE_USER}:${SERVICE_GROUP}" "${env_file}"
    fi
    step_ok "Wrote ${env_file}"
    if [[ "${OPT_AUTH}" == "token" ]]; then
        step_info "Generated an API token (VESICA_API_TOKEN in ${env_file}) — it is not printed here; read it from the file."
    fi
}

# ---------------------------------------------------------------------------
# 6. Data directory under a dedicated system user, 0750.
# ---------------------------------------------------------------------------
ensure_service_user() {
    if id "${SERVICE_USER}" >/dev/null 2>&1; then
        step_ok "System user '${SERVICE_USER}' already exists"
        return
    fi
    step_info "Creating system user '${SERVICE_USER}'..."
    dry useradd --system --no-create-home --shell /usr/sbin/nologin "${SERVICE_USER}"
    "${OPT_DRY_RUN}" || step_ok "Created system user '${SERVICE_USER}'"
}

setup_data_dir() {
    local data_dir="$1"
    section "Data directory"

    if "${OPT_DRY_RUN}"; then
        step_info "(dry-run) would create ${data_dir} (0750, owned by ${SERVICE_USER}:${SERVICE_GROUP})"
        return
    fi

    ensure_service_user
    mkdir -p "${data_dir}" "${data_dir}/blobs" "${data_dir}/backups"
    chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${data_dir}"
    chmod 0750 "${data_dir}"
    step_ok "Created ${data_dir} (0750, ${SERVICE_USER}:${SERVICE_GROUP})"
}

# ---------------------------------------------------------------------------
# 7. Migrate
# ---------------------------------------------------------------------------
run_migrations() {
    section "Migrate"
    local bin="${INSTALL_DIR}/bin/vesica"

    if "${OPT_DRY_RUN}"; then
        step_info "(dry-run) would run: (cd ${INSTALL_DIR} && ./bin/vesica migrate)"
        return
    fi

    # Run as the service user (when we have the privilege to), not root, so
    # the database file it creates is owned by the same account the
    # systemd unit (or the --no-service foreground command) runs as —
    # otherwise a root-created vesica.sqlite inside a vesica:vesica data
    # directory is unwritable by the service that has to open it next.
    local migrate_ok=true
    if [[ "${EUID}" -eq 0 ]] && id "${SERVICE_USER}" >/dev/null 2>&1; then
        (cd "${INSTALL_DIR}" && runuser -u "${SERVICE_USER}" -- "${bin}" migrate) || migrate_ok=false
    else
        (cd "${INSTALL_DIR}" && "${bin}" migrate) || migrate_ok=false
    fi

    "${migrate_ok}" || die "Migrations failed. Re-run with more logging via VESICA_LOG_LEVEL=debug ${bin} migrate from ${INSTALL_DIR}, or see 08-deployment.md's troubleshooting table."
    step_ok "Applied migrations"
}

# ---------------------------------------------------------------------------
# 8. Service — install the systemd unit, start it, poll readiness. With
#    --no-service, skip installing/starting anything and print the
#    foreground command instead.
# ---------------------------------------------------------------------------
stage_install_dir() {
    local bin
    bin="$(resolved_binary_path)"

    if "${OPT_DRY_RUN}"; then
        step_info "(dry-run) would install to ${INSTALL_DIR} (binary from ${bin})"
        return
    fi

    mkdir -p "${INSTALL_DIR}/bin"
    install -m 0755 "${bin}" "${INSTALL_DIR}/bin/vesica"
    step_ok "Installed ${INSTALL_DIR}/bin/vesica"
}

service_logs_tail() {
    if have_cmd journalctl; then
        journalctl -u "${SERVICE_NAME}" -n "${LOG_TAIL_LINES}" --no-pager 2>&1
    else
        echo "(journalctl not available to fetch service logs)"
    fi
}

poll_ready() {
    # Polls http://<bind-or-127.0.0.1>:<port>/api/v1/ready until it answers
    # 200, or gives up after READY_TIMEOUT_SECS and reports failure. Split
    # out from install_and_start_service so it can be exercised against any
    # already-running process (see the installer's own verification, which
    # points this at a foreground `vesica serve` — there is no systemd in a
    # container to install the unit into).
    local url="$1" log_fn="$2" waited=0
    while (( waited < READY_TIMEOUT_SECS )); do
        if curl -fsS -o /dev/null "${url}"; then
            return 0
        fi
        sleep 1
        waited=$(( waited + 1 ))
    done
    step_warn "Service did not become ready within ${READY_TIMEOUT_SECS}s. Last ${LOG_TAIL_LINES} log lines:"
    "${log_fn}" | tail -n "${LOG_TAIL_LINES}" >&2
    return 1
}

install_and_start_service() {
    section "Service"

    local probe_host="${RESOLVED_BIND}"
    [[ "${probe_host}" == "0.0.0.0" || "${probe_host}" == "::" ]] && probe_host="127.0.0.1"
    local ready_url="http://${probe_host}:${RESOLVED_PORT}/api/v1/ready"

    if "${OPT_NO_SERVICE}"; then
        step_ok "Skipping systemd (--no-service)"
        step_info "Start it yourself — 'vesica' reads ./.env from its working directory, the same file systemd's EnvironmentFile would point at:"
        step_info "  cd ${INSTALL_DIR} && sudo -u ${SERVICE_USER} ./bin/vesica serve"
        step_info "Then check: curl ${ready_url}"
        return
    fi

    if "${OPT_DRY_RUN}"; then
        step_info "(dry-run) would install ${SYSTEMD_UNIT_DST}, run 'systemctl daemon-reload', 'systemctl enable --now ${SERVICE_NAME}', then poll ${ready_url}"
        return
    fi

    have_cmd systemctl || die "systemctl not found — this host has no systemd to install a service into. Use --no-service and run '${INSTALL_DIR}/bin/vesica serve' yourself (as ${SERVICE_USER}, with ${INSTALL_DIR}/.env sourced), or install systemd."
    # The systemctl binary can be present (e.g. inside a container image)
    # without systemd actually running as PID 1 — /run/systemd/system only
    # exists when it really is, and that's a much friendlier check than
    # letting `systemctl daemon-reload` fail with a raw D-Bus error.
    [[ -d /run/systemd/system ]] || die "systemctl is installed, but this host is not actually running under systemd (no /run/systemd/system — common inside containers). Use --no-service and run '${INSTALL_DIR}/bin/vesica serve' yourself (as ${SERVICE_USER}, with ${INSTALL_DIR}/.env sourced), or run this on a real systemd host."

    install -m 0644 "${SYSTEMD_UNIT_SRC}" "${SYSTEMD_UNIT_DST}"
    systemctl daemon-reload
    systemctl enable "${SERVICE_NAME}"
    systemctl restart "${SERVICE_NAME}"
    step_ok "Installed systemd unit ${SERVICE_NAME}.service"

    if poll_ready "${ready_url}" service_logs_tail; then
        step_ok "Service started and healthy"
    else
        die "Service did not become healthy — see the log lines above (also: journalctl -u ${SERVICE_NAME} -f)."
    fi
}

# ---------------------------------------------------------------------------
# 9. Final report
# ---------------------------------------------------------------------------
final_report() {
    local data_dir="$1"
    local host="${RESOLVED_BIND}"
    [[ "${host}" == "0.0.0.0" || "${host}" == "::" ]] && host="127.0.0.1"
    local url="http://${host}:${RESOLVED_PORT}"

    section "Vesica is ready"
    printf '  Web UI      %s\n' "${url}"
    printf '  MCP (HTTP)  %s/mcp\n' "${url}"
    printf '  MCP (stdio) %s/bin/vesica mcp\n' "${INSTALL_DIR}"
    printf '  API docs    %s/api/docs\n\n' "${url}"
    printf '  Data        %s\n' "${data_dir}"
    if "${OPT_NO_SERVICE}"; then
        printf '  Logs        whatever you redirect the foreground process to\n'
    else
        printf '  Logs        journalctl -u %s -f\n' "${SERVICE_NAME}"
    fi
}

# ---------------------------------------------------------------------------
main() {
    parse_args "$@"

    preflight
    find_prebuilt_binary
    detect_toolchains
    build

    local data_dir="${OPT_DATA_DIR:-${DEFAULT_DATA_DIR}}"

    select_port_and_bind
    if "${OPT_DRY_RUN}"; then
        step_info "(dry-run) would install to ${INSTALL_DIR}"
    else
        mkdir -p "${INSTALL_DIR}"
    fi
    stage_install_dir
    write_env "${data_dir}"
    setup_data_dir "${data_dir}"
    run_migrations
    install_and_start_service

    if "${OPT_DRY_RUN}"; then
        section "Dry run complete — nothing was changed"
    else
        final_report "${data_dir}"
    fi
}

# Allow this script to be sourced (e.g. for testing individual functions —
# see the ticket's verification) without running main.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
