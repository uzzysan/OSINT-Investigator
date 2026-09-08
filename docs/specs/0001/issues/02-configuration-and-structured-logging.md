# 02: Configuration and structured logging

**What to build:** an operator can configure the application entirely through
environment variables and a `.env` file, and gets a clear error naming the
offending key when a value is wrong — rather than a panic or a silent default.
Logs go to stdout in a form journald can read, at a level the operator chooses
without rebuilding.

Every key in `.env.example` is represented in the configuration type. Secrets are
never written to a log, not even truncated.

The binary gains its CLI shape here: the subcommands from the architecture
document exist and parse, even though most only print that they are not yet
implemented. This means later tickets add behaviour behind an interface that
already exists rather than inventing one each time.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] Configuration loads from environment variables layered over `.env` over defaults
- [ ] Every key in `.env.example` maps to a field; the two cannot silently diverge
- [ ] An invalid value fails startup with a message naming the key and what was expected
- [ ] A missing `.env` is not an error — defaults apply
- [ ] Secret-bearing fields are redacted in every debug and log rendering
- [ ] Structured logging to stdout at a configurable level, with an optional JSON format
- [ ] All CLI subcommands parse and dispatch; unimplemented ones say so and exit non-zero
- [ ] `--help` output describes each subcommand accurately
