# ADR-0004 — Solo mode, no authentication, with an auth seam

**Status**: Accepted, 2026-09-08

## Context

The target deployment is one analyst on a machine they control, plus their
agents. Authentication is not free: accounts, sessions, password reset, per-case
permissions and the query changes that enforce them touch nearly every endpoint,
and none of it makes the tool better for its actual user.

But shipping something that is structurally impossible to secure later is a trap,
and this application holds sensitive data.

## Decision

Default to **solo mode**: no login, bound to `127.0.0.1`, one implicit user. A
`token` mode is available from v1 — a single shared bearer token for anything
reachable beyond localhost.

Preserve the seam that makes real auth a contained change:

1. All access checks pass through **one** middleware layer applied to every
   route, including `/mcp` and `/blob`. No handler decides for itself.
2. Every write records an **actor** already (`human`, or an agent name), so the
   audit log does not need reshaping when actors become accounts.
3. Read and write paths take a **scope** parameter — currently always
   "everything" — so per-case filtering has somewhere to go.
4. No schema assumption that there is exactly one user: `created_by` is a string,
   not a boolean.

## Alternatives considered

**Full multi-user with RBAC in v1.** Rejected as scope that serves nobody today.

**Multi-tenant with org isolation.** A different product. Rejected outright.

**No auth at all, not even token mode.** Rejected: someone will put this on a
LAN or behind a tunnel on day one, and there needs to be something better than
"don't".

## Consequences

**Accepted**
- Anyone who can reach the port in solo mode has full control. The documentation
  says so plainly, in the README, the installer output and the deployment guide.
- The installer defaults to binding `127.0.0.1` and requires an explicit
  `--bind` to do otherwise; choosing a non-loopback bind with `--auth solo`
  prints a warning.
- Audit attribution in solo mode is "human" — useful for separating human from
  agent, not for separating one person from another.
- When accounts arrive, existing rows have `created_by='human'` and will need a
  migration assigning them to an owner.

**Gained**
- No login wall between the analyst and their work.
- Agents authenticate with one token or none, which removes a common source of
  broken MCP configuration.
- Meaningfully less code, and less code that must be right.
