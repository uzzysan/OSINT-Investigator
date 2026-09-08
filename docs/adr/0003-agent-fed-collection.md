# ADR-0003 — No built-in collectors in v1; agents feed the graph

**Status**: Accepted, 2026-09-08

## Context

Maltego's defining feature is the transform: right-click an entity, run a query
against a third-party service, get new entities back. It is genuinely useful and
it is also the single largest source of complexity in that product — credential
storage, rate limiting, per-service ToS, output mapping, error handling,
versioning, and a plugin API that has to stay stable.

Meanwhile the intended primary operator of this system is an AI agent that
already has web search, browser automation, shell access and its own OSINT
tooling. It does not need the application to fetch things; it needs somewhere
structured to *put* what it fetched.

## Decision

v1 has no built-in collectors. The application is a knowledge graph plus an MCP
server. Agents run whatever collection they run and write findings through
`graph_write`, always with a source.

The design keeps the seam open: `source.kind = "agent"` exists, the audit log
distinguishes machine writes, and a future `enricher` plugin interface is
sketched but not built.

## Alternatives considered

**Ship a plugin runner for collectors in v1.** Much closer to Maltego and a
stronger demo. Rejected: it roughly doubles v1 scope, and the doubled part is
the half that ages worst — every collector breaks when its upstream changes, and
maintaining twenty of them is a full-time job that competes with the graph
itself.

**Bulk import only, no live collection.** Effectively what we are doing, plus we
also get agents. Import is in v1 (F-IO-1) for exactly this reason.

## Consequences

**Accepted**
- Out of the box the graph is empty and stays empty until someone or something
  fills it. First-run experience needs a good demo case and clear agent
  onboarding, or the tool looks inert.
- Users comparing directly against Maltego will notice the missing right-click.
- Data quality depends on the agent. Confidence flags and mandatory-ish sourcing
  are the counterweight.

**Gained**
- No API keys to store, rotate or leak.
- No third-party ToS exposure baked into the product.
- v1 ships materially sooner, focused on the parts that are hard to replace:
  the model, the resolver, the canvas, the MCP surface.
- Collection stays where it evolves fastest — outside the application.
