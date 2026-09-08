# 05: CI pipeline and one-command install

**What to build:** two guarantees that hold from here to release. First, a push
that breaks formatting, trips a lint, fails a test, pulls in a vulnerable
dependency or commits a secret is rejected automatically. Second, an operator can
clone the repository onto a clean machine, run one script, and end up with a
running service — with no manual configuration and without reading documentation.

The installer is written now, while the application does almost nothing, and this
is deliberate. Proving the deployment path against a trivial application is
cheap; discovering it does not work after eight milestones of features is not.

**Blocked by:** 04

**Status:** ready-for-agent

- [ ] CI runs formatting, lints as errors, workspace tests, dependency advisories, secret scanning and the web build
- [ ] A deliberately introduced secret fails the pipeline
- [ ] The installer detects the platform and refuses unsupported ones with an explanation
- [ ] Missing toolchains are installed, or the run fails clearly when installation is declined
- [ ] A free port is chosen when the preferred one is taken, and the choice is recorded
- [ ] An existing `.env` is never overwritten; new keys are reported instead
- [ ] A data directory is created under a dedicated system user with restrictive permissions
- [ ] Migrations run and a service is installed, started and verified against readiness
- [ ] Failure prints the last lines of the service log rather than only an exit code
- [ ] Final output states the URL, the data directory and how to follow the logs
- [ ] Re-running the installer on an installed system is safe
