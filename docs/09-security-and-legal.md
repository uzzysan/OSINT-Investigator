# 9. Security, Privacy and Legal

This application stores dossiers on identifiable people. That deserves more care
than a typical side project, both for the operator's sake and for the people in
the database.

## 9.1 Threat model

**What we defend against**

| Threat | Mitigation |
| --- | --- |
| Someone on the network reaching the instance | Default bind `127.0.0.1`; `token` mode for anything wider; TLS via a reverse proxy |
| Data exfiltration through the app | No outbound calls the operator did not configure; no telemetry; CSP forbidding external origins |
| Stored XSS from scraped content landing in properties | All rendering escapes by default; Markdown sanitised; no `dangerouslySetInnerHTML` on user data |
| Malicious uploads | MIME sniffing on content not filename; blobs served with `Content-Disposition: attachment` and `X-Content-Type-Options: nosniff` from a path that never executes |
| Path traversal via filenames | Blobs stored under their SHA-256, never their filename |
| SQL injection | `sqlx` compile-time-checked parameterised queries throughout; no string-built SQL |
| Prompt injection reaching agents through stored data | Tool descriptions state that entity content is data; report/dossier output marks scraped text as untrusted |
| Accidental data loss | Soft deletes, undo, audit log, nightly backups, integrity check |
| Backup theft | Backups inherit `0750` and the `vesica` user; encryption at rest is the operator's layer (LUKS, or `restic` with a passphrase) |

**What we do not defend against**

- A compromised host. If someone has root, they have the SQLite file.
- A malicious operator. This tool has no controls that limit what its user does.
- Traffic analysis of a reverse-proxied deployment.
- Correctness of the data. Everything in it is an assertion by whoever entered it.

## 9.2 Secret handling

- All configuration through `.env` and environment variables. `.env` is in
  `.gitignore`; `.env.example` carries placeholders only.
- No credentials in the database, in logs, or in exports.
- API tokens are compared in constant time and are never logged, not even
  truncated.
- CI runs a secret scan (`gitleaks`) on every push; a hit fails the build.
- If v2 adds collectors with third-party API keys, they go in `.env` too, and the
  UI shows key *presence*, never value.

## 9.3 Privacy and data protection

The operator is the data controller. The application's job is to make good
practice the easy path.

| Obligation | How the app helps |
| --- | --- |
| **Purpose limitation** | Data lives in a case; cases have a stated purpose in their description; the dashboard shows what a case contains |
| **Data minimisation** | Nothing is collected automatically in v1 — every record is a deliberate act |
| **Accuracy** | Confidence flags (`confirmed` / `probable` / `hypothesis`) are mandatory and visible; unsourced facts are flagged |
| **Storage limitation** | Case archival and purge; soft-deleted records purge after 30 days; retention is configurable |
| **Right of access / erasure** | Global search finds every mention of a person across all cases; purge removes an entity and its properties, with the audit log retaining only the fact that a deletion occurred |
| **Accountability** | Append-only audit log records who or what wrote each fact, and when |
| **Provenance** | Every fact can carry its source and capture timestamp — which is also what makes a report defensible |

Two things the software will not do, deliberately: it will not present inferred
links as facts, and it will not merge records without a human decision. Both are
places where an automated system quietly manufactures false conclusions about
real people.

The repository will carry a short `ETHICS.md` covering acceptable use, and the
README will state plainly that lawful basis and proportionality are the
operator's responsibility, not the tool's.

## 9.4 Application security practices

- `cargo deny` and `cargo audit` in CI for advisories, licences and duplicate
  dependencies.
- `npm audit` / `osv-scanner` for the frontend.
- `#![forbid(unsafe_code)]` in every crate unless a documented exception exists.
- Dependency additions are reviewed: this is a project where a supply-chain
  compromise reads other people's investigations.
- Upload limits, request body limits, and per-connection timeouts on every route.
- Rate limiting on search and traversal endpoints, which are the expensive ones.
- Graph traversal is always bounded by depth *and* node budget, so a crafted
  query cannot exhaust memory.

## 9.5 Content Security Policy

```
default-src 'self';
img-src 'self' data: blob:;
script-src 'self';
style-src 'self' 'unsafe-inline';
connect-src 'self';
frame-ancestors 'none';
object-src 'none';
base-uri 'none'
```

No CDN, no external fonts, no analytics. Everything the browser loads comes from
the binary. This is also what lets the tool run on an air-gapped machine.

## 9.6 Legal note

Different jurisdictions treat the collection and retention of personal data very
differently, and "it was publicly available" is not a universal defence. Nothing
in this repository is legal advice; operators are expected to know the rules that
apply to them.
