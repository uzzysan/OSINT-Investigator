# 7. MCP Server

The MCP surface is not a thin wrapper over the REST API. It is designed for a
different consumer: an agent with a limited context window, no screen, and a
tendency to work in batches. Same service layer underneath, different
ergonomics on top.

Built on [`rmcp`](https://docs.rs/rmcp), the official Rust MCP SDK.

## 7.1 Transports

| Transport | How | For |
| --- | --- | --- |
| **stdio** | `osintd mcp` | Local agents (Claude Code, Claude Desktop, Cowork) launched as a subprocess |
| **streamable HTTP** | `POST/GET http://host:port/mcp` | Remote agents, and anything already talking to the web server |

Both are served by the same handler set. The HTTP transport honours the same
auth mode as the REST API.

Client configuration example (stdio):

```json
{
  "mcpServers": {
    "osint-investigator": {
      "command": "/opt/osint-investigator/bin/osintd",
      "args": ["mcp"],
      "env": { "OSINT_DB_PATH": "/var/lib/osint-investigator/osint.sqlite" }
    }
  }
}
```

## 7.2 Design rules for the tool surface

1. **Compact by default.** `entity_get` returns caption, schema, key properties
   and relation counts. Full property bags, sources and timelines arrive only
   when asked for via `include`. Token budget is a real constraint.
2. **Batch-shaped.** Agents discover ten things at once. `graph_write` takes a
   whole subgraph in one transactional call, with local reference ids so
   relations can point at entities created in the same call.
3. **Errors teach.** Every validation error carries a `fix` string written for a
   model to act on, not for a human to read.
4. **Provenance is required, not optional.** Write tools take a `source`
   argument. Omitting it is allowed but the entity is flagged unsourced and the
   tool response says so.
5. **Reads are safe, writes are declared.** Read tools are annotated
   `readOnlyHint`; destructive tools are annotated `destructiveHint` so clients
   can gate them.
6. **The agent proposes, the human disposes.** By default an agent may *read*
   the match queue and *propose* resolutions, but `match_resolve` with
   `action: "merge"` requires `OSINT_MCP_ALLOW_MERGE=true`.

## 7.3 Tools

### Discovery

| Tool | Description |
| --- | --- |
| `schema_list` | All entity and relation schemas with their properties. The agent's map of what can be modelled. |
| `schema_describe` | One schema in full, with property types, examples and validation rules. |
| `case_list` | All cases with entity/relation counts and last activity. |
| `case_get` | One case: description, counts by schema, open questions, unresolved matches. |
| `case_create` | Create a case. |

### Reading the graph

| Tool | Description |
| --- | --- |
| `search` | Full-text and fuzzy search across all cases or one. Returns compact hits with case attribution. |
| `entity_get` | One entity. `include: ["properties","relations","sources","timeline","matches"]`. |
| `entity_list` | Filter by schema, tag, confidence, updated-since. Paginated. |
| `neighbours` | Entities within N hops, with the connecting relations. `follow_links` crosses confirmed same-as bridges. |
| `path_find` | Shortest path (and optionally all paths) between two entities. The tool an agent reaches for when asked "how is X connected to Y?". |
| `timeline_get` | Dated events in a case or for a subject, in order, with precision. |
| `relation_list` | Relations by case, schema, or endpoint. |

### Writing

| Tool | Description |
| --- | --- |
| `entity_create` | One entity with properties and a source. |
| `entity_update` | Add/replace/remove properties; change confidence and tags. |
| `relation_create` | Typed relation with properties, interval and source. |
| `graph_write` | **The main write tool.** Takes `entities[]` and `relations[]` in one transaction. Entities carry a `ref` (a local id); relations reference `ref` or a real id. Returns the mapping from refs to created ids. |
| `observation_add` | A dated statement about an entity or relation, for the timeline. |
| `source_add` | Register a source (url/file/manual) and cite it on targets. |
| `attachment_add` | Attach a file by base64 payload or by a path the server can read. |
| `entity_delete` | Soft delete. Annotated destructive. |

`graph_write` request shape:

```json
{
  "case_id": "01J...",
  "actor": "recon-agent",
  "source": { "kind": "url", "url": "https://...", "title": "KRS extract",
              "retrieved_at": "2026-09-08T09:12:00Z" },
  "entities": [
    { "ref": "p1", "schema": "Person",
      "properties": { "name": ["Jan Kowalski"], "birthDate": ["1978-04-12"] } },
    { "ref": "c1", "schema": "Company",
      "properties": { "name": ["ACME Sp. z o.o."], "taxNumber": ["5252248481"] } }
  ],
  "relations": [
    { "schema": "Directorship", "source": "p1", "target": "c1",
      "properties": { "role": ["Prezes Zarządu"] },
      "date_start": "2019-06-01", "date_precision": "day" }
  ]
}
```

Response returns `{"refs": {"p1": "01J...", "c1": "01J..."}, "matches_pending": 2}`
— the last field tells the agent that duplicate suggestions are being computed.

### Entity resolution

| Tool | Description |
| --- | --- |
| `match_queue` | Open match candidates for a case, sorted by score, with explanations. |
| `match_find` | On-demand duplicate search for one entity, lower threshold. |
| `match_resolve` | Apply `merge` \| `link` \| `different` \| `defer`. `merge` gated by config. |

### Output

| Tool | Description |
| --- | --- |
| `case_report` | Render a Markdown report: summary, key entities, relation tables, timeline, Mermaid graph, source bibliography. Sections selectable. |
| `case_export` | JSON or FollowTheMoney JSON-lines. |
| `subgraph_render` | A Mermaid diagram of a selection or a path, for embedding in an agent's own answer. |

## 7.4 Resources

| URI | Content |
| --- | --- |
| `case://{id}/summary` | Markdown case briefing: what it is, key entities, open threads |
| `case://{id}/schema-usage` | Which schemas are in use and how heavily |
| `entity://{id}` | Markdown dossier for one entity |
| `osint://schemas` | The whole ontology as one readable document |

## 7.5 Prompts

| Prompt | Arguments | Purpose |
| --- | --- | --- |
| `profile_entity` | `entity_id` | Assemble everything known about an entity and identify gaps worth investigating |
| `write_case_report` | `case_id`, `audience` | Produce a report at the right level of detail |
| `triage_matches` | `case_id` | Walk the match queue, recommend an action per pair with reasoning |
| `ingest_findings` | `case_id` | Turn unstructured research notes into `graph_write` calls |
| `find_connection` | `a`, `b` | Explain how two entities relate, with the path and the evidence |

## 7.6 Error contract

```json
{
  "isError": true,
  "content": [{
    "type": "text",
    "text": "Validation failed for entity ref 'p1':\n  properties.birthDate[0]: expected ISO-8601 date, got '12/04/1978'.\n  Fix: use '1978-04-12'; partial dates such as '1978' or '1978-04' are accepted.\n\nNo entities were created; the batch was rolled back."
  }]
}
```

Batches are all-or-nothing and the error says so, because an agent that cannot
tell whether a partial write happened will duplicate data on retry.

## 7.7 Safety notes

- MCP write tools are annotated so a client can require confirmation.
- Every MCP-originated write records `actor` in the audit log; the UI shows an
  "agent" badge and the case dashboard can filter to agent-created entities.
- `attachment_add` by server path is restricted to a configured directory.
- Tool output containing entity data is data, not instruction — the tool
  descriptions state this so a downstream agent does not treat scraped text
  stored in a `notes` property as a command.
