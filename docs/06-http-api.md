# 6. HTTP API

Base path `/api/v1`. JSON in, JSON out. The API and the MCP server call the same
service layer, so anything an agent can do through MCP is reachable here and
vice versa.

## 6.1 Conventions

- Ids are ULIDs (26 chars, lexicographically sortable).
- Timestamps are ISO-8601 UTC (`2026-09-08T13:40:12Z`).
- Partial dates are ISO-8601 prefixes (`2019`, `2019-03`) with an explicit
  `precision` field alongside.
- Lists are cursor-paginated: `?limit=50&cursor=<opaque>`; responses carry
  `next_cursor` when more exists.
- Errors are RFC 9457 problem documents:

```json
{
  "type": "https://osint.invalid/errors/validation",
  "title": "Invalid property value",
  "status": 422,
  "detail": "Property 'birthDate' expects an ISO-8601 date; got '12/04/1978'.",
  "fix": "Use '1978-04-12'. Partial dates like '1978' are accepted.",
  "pointer": "/properties/birthDate/0"
}
```

  The `fix` field is deliberate: it is what an agent reads to correct itself
  without a round trip to a human.
- Writes accept an optional `Idempotency-Key` header.
- `X-Actor` identifies the writer (`human` by default, an agent name otherwise)
  and lands in the audit log.

## 6.2 Endpoints

### Cases

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/cases` | List cases with counts |
| `POST` | `/cases` | Create |
| `GET` | `/cases/{id}` | Detail + dashboard stats |
| `PATCH` | `/cases/{id}` | Update |
| `DELETE` | `/cases/{id}` | Soft delete (`?purge=true` to destroy) |
| `GET` | `/cases/{id}/export` | `?format=json\|ftm\|markdown` |
| `POST` | `/cases/import` | Import a previously exported bundle |

### Entities

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/cases/{case}/entities` | List/filter: `schema`, `tag`, `q`, `confidence`, `updated_since` |
| `POST` | `/cases/{case}/entities` | Create |
| `GET` | `/entities/{id}` | Detail; `?include=relations,sources,timeline,matches,audit` |
| `PATCH` | `/entities/{id}` | Update properties, caption, confidence, tags |
| `DELETE` | `/entities/{id}` | Soft delete |
| `POST` | `/entities/{id}/restore` | Undelete |
| `GET` | `/entities/{id}/neighbours` | `?depth=2&limit=500&schemas=&follow_links=false` |
| `POST` | `/entities/batch` | Transactional bulk create/update |

Create body:

```json
{
  "schema": "Person",
  "confidence": "probable",
  "properties": {
    "name": ["Jan Kowalski"],
    "birthDate": ["1978-04-12"],
    "phone": ["+48 601 234 567"],
    "email": ["j.kowalski@example.com"]
  },
  "tags": ["target"],
  "source": {
    "kind": "url",
    "url": "https://rejestr.example/company/12345",
    "title": "KRS extract",
    "retrieved_at": "2026-09-08T09:12:00Z"
  }
}
```

### Relations

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/cases/{case}/relations` | List/filter |
| `POST` | `/cases/{case}/relations` | Create |
| `GET` | `/relations/{id}` | Detail |
| `PATCH` / `DELETE` | `/relations/{id}` | Update / soft delete |

### Graph

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/cases/{case}/graph` | Nodes + edges for the canvas, with filters |
| `GET` | `/graph/path` | `?from=&to=&max_depth=6&follow_links=true` — shortest path |
| `GET` | `/graph/paths` | All simple paths up to depth, budgeted |
| `POST` | `/cases/{case}/graph/metrics` | Degree, betweenness, components, communities |

Graph responses always carry `{ "truncated": bool, "node_budget": n }`.

### Search

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/search` | `?q=&case=&schema=&fuzzy=true&limit=` — across all cases |
| `GET` | `/search/structured` | Field-scoped query language |

### Sources, blobs, attachments

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/cases/{case}/sources` | Create a source |
| `POST` | `/sources/{id}/attachments` | Multipart upload; streams to disk, hashes, dedupes |
| `GET` | `/blob/{sha256}` | Serve, with `Content-Disposition` and strong caching |
| `GET` | `/blob/{sha256}/thumb` | Thumbnail |
| `POST` | `/citations` | Cite a source on an entity/relation/property/observation |

### Timeline

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/cases/{case}/timeline` | `?from=&to=&subjects=&kinds=` |
| `POST` | `/cases/{case}/observations` | Add a dated observation |

### Matching

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/cases/{case}/matches` | Review queue, sorted by score |
| `GET` | `/entities/{id}/matches` | Candidates for one entity |
| `POST` | `/entities/{id}/find-duplicates` | On-demand pass with a lower threshold |
| `POST` | `/matches/{id}/resolve` | `{"action":"merge"\|"link"\|"different"\|"defer"}` |
| `POST` | `/merges/{batch_id}/undo` | Reverse a merge |

### Schemas

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/schemas` | All entity and relation schemas, built-in and custom |
| `GET` | `/schemas/{id}` | One, with full property definitions |
| `POST` | `/schemas` | Define a custom schema |
| `PATCH` | `/schemas/{id}` | Extend a schema with additional properties |

### Import / export / reports

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/imports` | Upload a CSV/XLSX/FtM file; returns a preview and a mapping suggestion |
| `POST` | `/imports/{id}/dry-run` | What would be created vs. matched |
| `POST` | `/imports/{id}/commit` | Apply under one `batch_id` |
| `POST` | `/imports/{id}/undo` | Reverse |
| `GET` | `/cases/{case}/report` | `?format=markdown\|json&sections=` |

### Operations

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/health` | Liveness |
| `GET` | `/ready` | Migrations applied, DB and blob dir writable |
| `GET` | `/stats` | Counts, match-queue depth, resolver lag |
| `GET` | `/events` | SSE stream of change events for live UI updates |
| `GET` | `/audit` | Audit log, filterable by case/target/actor/batch |

## 6.3 Live updates

`GET /events` is a Server-Sent Events stream. Event types:
`entity.created`, `entity.updated`, `entity.deleted`, `relation.*`,
`match.suggested`, `import.progress`, `case.updated`. Each carries the case id so
the client filters cheaply. This is how a browser tab reflects an agent's writes
without polling.

## 6.4 Access control

In `solo` mode (the default) the API is unauthenticated and expected to be bound
to `127.0.0.1` or a trusted network. In `token` mode every request needs
`Authorization: Bearer <VESICA_API_TOKEN>`, checked in constant time.

The middleware layer that enforces this is written once and applied to every
route including `/mcp`, so introducing real user accounts later is a change
inside that layer rather than across the codebase. See
[ADR-0004](adr/0004-solo-mode-access.md).

## 6.5 OpenAPI

The spec is generated from the handlers with `utoipa` and served at
`/api/v1/openapi.json`, with a Scalar UI at `/api/docs`. Agents that prefer REST
to MCP can bootstrap entirely from it.
