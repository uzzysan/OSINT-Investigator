# 4. Data Model

Two layers: the **ontology** (what an entity *is*) and the **storage schema**
(how it sits in SQLite). They are deliberately separate — the ontology is data,
loaded from YAML and from the database, so adding an entity type is not a
migration.

---

## 4.1 Ontology

### 4.1.1 Property types

Every property has a type. The type decides how a value is validated, how it is
normalised, how it is indexed, and how much weight it carries when the resolver
compares two entities.

| Type | Example | Normalisation | Match weight |
| --- | --- | --- | --- |
| `name` | `Jan Kowalski` | Unicode NFKD fold, lowercase, strip punctuation, collapse whitespace, sort tokens for the blocking key | medium |
| `email` | `J.Kowalski@Example.COM` | lowercase; strip `+tag`; punctuation-fold gmail locals | **strong** |
| `phone` | `+48 601 234 567` | E.164 via `phonenumber`, default region from the case | **strong** |
| `url` | `https://Example.com/Path/` | lowercase scheme+host, strip default port, strip tracking params | medium |
| `domain` | `Example.COM.` | lowercase, punycode, strip trailing dot, registrable domain kept separately | **strong** |
| `ip` | `2001:db8::1` | canonical form | strong |
| `identifier` | `NIP 5252248481`, IBAN, VIN, passport no. | strip separators, uppercase, checksum-validate where the scheme allows (IBAN mod-97, VIN, PESEL, NIP) | **very strong** |
| `date` | `2019-03`, `2019` | ISO-8601 partial dates, precision preserved | medium |
| `country` | `Poland`, `PL` | ISO-3166-1 alpha-2 | weak |
| `address` | `ul. Piękna 12/4, 00-549 Warszawa` | token fold, extract postcode + house number as separate keys | medium |
| `text` | free prose | trim only | none |
| `number` | `42.5` | numeric | weak |
| `entity` | reference to another entity | — | n/a |
| `checksum` | `sha256:…` | lowercase hex | **very strong** |
| `iban` | bank account | strip separators, uppercase, mod-97 validate | **very strong** |
| `wallet` | crypto address | **case is meaningful — never blanket-uppercase.** bech32 lower-cased and polymod-checked; EVM `0x` + 40 hex lower-cased (mixed case there is the EIP-55 checksum); base58 left exactly as given | **very strong** |

"Very strong" identifiers alone can carry a pair over the suggest threshold.
Names alone never can.

### 4.1.2 Entity schemas

A schema is:

```yaml
id: Person
label: Person
plural: People
extends: [LegalEntity]          # property inheritance
icon: user
color: "#3b82f6"
caption: "{name}"               # template, first non-empty wins
matchable: true
properties:
  - { name: name,        type: name,       label: Name,            multiple: true, required: true }
  - { name: alias,       type: name,       label: Also known as,   multiple: true }
  - { name: birthDate,   type: date,       label: Date of birth }
  - { name: birthPlace,  type: address,    label: Place of birth }
  - { name: nationality, type: country,    label: Nationality,     multiple: true }
  - { name: gender,      type: text,       label: Gender }
  - { name: idNumber,    type: identifier, label: ID number,       multiple: true }
  - { name: taxNumber,   type: identifier, label: Tax number,      multiple: true }
  - { name: email,       type: email,      label: Email,           multiple: true }
  - { name: phone,       type: phone,      label: Phone,           multiple: true }
  - { name: address,     type: address,    label: Address,         multiple: true }
  - { name: position,    type: text,       label: Position,        multiple: true }
  - { name: notes,       type: text,       label: Notes }
```

Built-in schemas ship in `crates/vesica-core/schemas/*.yaml`, embedded in the
binary. Custom schemas live in a `schema_def` table and are layered on at
startup; a custom schema may extend a built-in one and may add properties to it.

Schema ids and property names are **stable identifiers**. Renaming a label is
free; renaming an id is a migration.

### 4.1.3 Relation schemas

```yaml
id: Ownership
label: Ownership
source: { label: Owner, schemas: [LegalEntity, Person, Company, Organization] }
target: { label: Asset, schemas: [Thing] }
directed: true
interval: true
color: "#f59e0b"
properties:
  - { name: percentage,   type: number, label: Share percent }
  - { name: sharesCount,  type: number, label: Number of shares }
  - { name: role,         type: text,   label: Role }
```

`source.schemas` / `target.schemas` are advisory: the UI offers only valid
combinations, but an explicit override is allowed and flagged, because
investigations routinely encounter things the ontology did not anticipate.

### 4.1.4 Abstract schemas

`Thing`, `LegalEntity`, `Asset`, `Interval`, `Analyzable` exist for property
inheritance and for relation constraints. They cannot be instantiated.

---

## 4.2 Storage schema

SQLite, WAL mode, foreign keys on. Ids are ULIDs stored as 26-char TEXT.
Timestamps are ISO-8601 UTC TEXT. JSON columns hold structured extras.

### 4.2.1 Core tables

```sql
CREATE TABLE kase (                       -- "case" is reserved in SQL
  id           TEXT PRIMARY KEY,
  slug         TEXT NOT NULL UNIQUE,
  name         TEXT NOT NULL,
  description  TEXT,
  color        TEXT,
  status       TEXT NOT NULL DEFAULT 'active',  -- active | archived
  default_region TEXT,                    -- ISO country, for phone parsing
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL,
  deleted_at   TEXT
);

CREATE TABLE entity (
  id           TEXT PRIMARY KEY,
  case_id      TEXT NOT NULL REFERENCES kase(id) ON DELETE CASCADE,
  schema_id    TEXT NOT NULL,
  caption      TEXT NOT NULL,
  confidence   TEXT NOT NULL DEFAULT 'probable',  -- confirmed|probable|hypothesis
  extra        TEXT,                     -- JSON: custom/unmapped fields
  created_at   TEXT NOT NULL,
  created_by   TEXT NOT NULL,            -- 'human' | agent name
  updated_at   TEXT NOT NULL,
  deleted_at   TEXT,
  merged_into  TEXT REFERENCES entity(id)  -- set when this entity was merged away
);
CREATE INDEX ix_entity_case   ON entity(case_id) WHERE deleted_at IS NULL;
CREATE INDEX ix_entity_schema ON entity(case_id, schema_id) WHERE deleted_at IS NULL;

-- Entity-Attribute-Value: properties repeat, so they cannot be columns.
CREATE TABLE entity_prop (
  id           INTEGER PRIMARY KEY,
  entity_id    TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  prop         TEXT NOT NULL,            -- property name within the schema
  ptype        TEXT NOT NULL,            -- denormalised property type
  seq          INTEGER NOT NULL DEFAULT 0,
  value        TEXT NOT NULL,            -- exactly as entered
  value_norm   TEXT,                     -- normalised, for exact matching
  value_num    REAL,                     -- for numeric comparison
  date_start   TEXT,                     -- partial-date lower bound
  date_end     TEXT,                     -- partial-date upper bound
  date_prec    TEXT,                     -- year|month|day|instant
  source_id    TEXT REFERENCES source(id) ON DELETE SET NULL
);
CREATE INDEX ix_prop_entity ON entity_prop(entity_id);
CREATE INDEX ix_prop_norm   ON entity_prop(ptype, value_norm);   -- identifier lookups
CREATE INDEX ix_prop_name   ON entity_prop(prop, value_norm);

CREATE TABLE relation (
  id           TEXT PRIMARY KEY,
  case_id      TEXT NOT NULL REFERENCES kase(id) ON DELETE CASCADE,
  schema_id    TEXT NOT NULL,
  source_id_   TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  target_id    TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  directed     INTEGER NOT NULL DEFAULT 1,
  date_start   TEXT, date_end TEXT, date_prec TEXT,
  confidence   TEXT NOT NULL DEFAULT 'probable',
  caption      TEXT,
  created_at   TEXT NOT NULL, created_by TEXT NOT NULL,
  updated_at   TEXT NOT NULL, deleted_at TEXT
);
CREATE INDEX ix_rel_src  ON relation(source_id_) WHERE deleted_at IS NULL;
CREATE INDEX ix_rel_tgt  ON relation(target_id)  WHERE deleted_at IS NULL;
CREATE INDEX ix_rel_case ON relation(case_id)    WHERE deleted_at IS NULL;

CREATE TABLE relation_prop (
  id INTEGER PRIMARY KEY,
  relation_id TEXT NOT NULL REFERENCES relation(id) ON DELETE CASCADE,
  prop TEXT NOT NULL, ptype TEXT NOT NULL, seq INTEGER NOT NULL DEFAULT 0,
  value TEXT NOT NULL, value_norm TEXT, value_num REAL,
  source_id TEXT REFERENCES source(id) ON DELETE SET NULL
);
```

### 4.2.2 Evidence

```sql
CREATE TABLE source (
  id           TEXT PRIMARY KEY,
  case_id      TEXT NOT NULL REFERENCES kase(id) ON DELETE CASCADE,
  kind         TEXT NOT NULL,            -- url | file | manual | agent
  title        TEXT,
  url          TEXT,
  url_norm     TEXT,
  excerpt      TEXT,
  retrieved_at TEXT,
  created_at   TEXT NOT NULL,
  created_by   TEXT NOT NULL,
  meta         TEXT                      -- JSON
);
CREATE INDEX ix_source_case ON source(case_id);
CREATE INDEX ix_source_url  ON source(url_norm);

CREATE TABLE blob (                       -- content-addressed, deduplicated
  sha256       TEXT PRIMARY KEY,
  bytes        INTEGER NOT NULL,
  mime         TEXT NOT NULL,
  path         TEXT NOT NULL,            -- data/blobs/ab/cd/<sha256>
  thumb_path   TEXT,
  created_at   TEXT NOT NULL
);

CREATE TABLE attachment (
  id           TEXT PRIMARY KEY,
  source_id    TEXT NOT NULL REFERENCES source(id) ON DELETE CASCADE,
  sha256       TEXT NOT NULL REFERENCES blob(sha256),
  filename     TEXT NOT NULL,
  created_at   TEXT NOT NULL
);

-- A source can be cited by an entity, a relation, a property or an observation.
CREATE TABLE citation (
  id           INTEGER PRIMARY KEY,
  source_id    TEXT NOT NULL REFERENCES source(id) ON DELETE CASCADE,
  target_kind  TEXT NOT NULL,            -- entity | relation | prop | observation
  target_id    TEXT NOT NULL,
  note         TEXT,
  created_at   TEXT NOT NULL
);
CREATE INDEX ix_citation_target ON citation(target_kind, target_id);
```

### 4.2.3 Timeline

```sql
CREATE TABLE observation (
  id           TEXT PRIMARY KEY,
  case_id      TEXT NOT NULL REFERENCES kase(id) ON DELETE CASCADE,
  subject_kind TEXT NOT NULL,            -- entity | relation | case
  subject_id   TEXT NOT NULL,
  title        TEXT NOT NULL,
  body         TEXT,
  date_start   TEXT NOT NULL,
  date_end     TEXT,
  date_prec    TEXT NOT NULL,
  kind         TEXT,                     -- sighting | filing | transaction | ...
  created_at   TEXT NOT NULL, created_by TEXT NOT NULL
);
CREATE INDEX ix_obs_case ON observation(case_id, date_start);
```

The timeline is a **view** over three producers: `observation` rows, relation
intervals, and date-typed `entity_prop` rows. A materialised
`timeline_event` table is populated by triggers so the timeline query stays a
single indexed scan.

### 4.2.4 Matching and linking

```sql
CREATE TABLE match_job (                  -- durable work queue
  entity_id  TEXT PRIMARY KEY REFERENCES entity(id) ON DELETE CASCADE,
  enqueued_at TEXT NOT NULL,
  attempts   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE block_key (                  -- blocking index for candidate lookup
  entity_id  TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL,               -- ident | phone | email | domain
                                          -- | name_sorted | name_phonetic
                                          -- | dob | addr_key
  key        TEXT NOT NULL,
  PRIMARY KEY (entity_id, kind, key)
);
CREATE INDEX ix_block_lookup ON block_key(kind, key);

CREATE TABLE match_candidate (
  id          INTEGER PRIMARY KEY,
  left_id     TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  right_id    TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  score       REAL NOT NULL,
  features    TEXT NOT NULL,              -- JSON: per-feature contributions
  cross_case  INTEGER NOT NULL,
  status      TEXT NOT NULL DEFAULT 'open', -- open|merged|linked|different|deferred
  decided_at  TEXT, decided_by TEXT,
  created_at  TEXT NOT NULL,
  UNIQUE (left_id, right_id)              -- left_id < right_id enforced on write
);
CREATE INDEX ix_match_open ON match_candidate(status, score DESC);

CREATE TABLE entity_link (                -- confirmed same-as, cross-case
  id          TEXT PRIMARY KEY,
  a_id        TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  b_id        TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  kind        TEXT NOT NULL DEFAULT 'same_as',
  note        TEXT,
  created_at  TEXT NOT NULL, created_by TEXT NOT NULL,
  UNIQUE (a_id, b_id)
);
```

`match_candidate` rows with status `different` are tombstones: the pair is never
proposed again, even if either entity is edited later.

### 4.2.5 Search

```sql
CREATE VIRTUAL TABLE entity_fts USING fts5(
  entity_id UNINDEXED,
  case_id   UNINDEXED,
  caption,
  props,                                  -- all searchable values concatenated
  tokenize = 'unicode61 remove_diacritics 2'
);
```

Kept in sync by triggers on `entity` and `entity_prop`. Fuzzy search layers a
trigram similarity pass over the FTS shortlist rather than scanning everything.

### 4.2.6 Bookkeeping

```sql
CREATE TABLE tag (
  id TEXT PRIMARY KEY, case_id TEXT REFERENCES kase(id) ON DELETE CASCADE,
  name TEXT NOT NULL, color TEXT, UNIQUE (case_id, name)
);
CREATE TABLE entity_tag (
  entity_id TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  tag_id    TEXT NOT NULL REFERENCES tag(id)    ON DELETE CASCADE,
  PRIMARY KEY (entity_id, tag_id)
);

CREATE TABLE audit_log (
  id         INTEGER PRIMARY KEY,
  at         TEXT NOT NULL,
  actor      TEXT NOT NULL,               -- 'human' | agent name
  action     TEXT NOT NULL,               -- create|update|delete|merge|link|import
  case_id    TEXT,
  target_kind TEXT NOT NULL, target_id TEXT NOT NULL,
  before     TEXT, after TEXT,            -- JSON
  batch_id   TEXT                         -- groups an import/merge for undo
);
CREATE INDEX ix_audit_target ON audit_log(target_kind, target_id, at DESC);
CREATE INDEX ix_audit_batch  ON audit_log(batch_id);

CREATE TABLE schema_def (                 -- custom schemas, layered on built-ins
  id TEXT PRIMARY KEY, kind TEXT NOT NULL,  -- entity | relation
  definition TEXT NOT NULL,               -- YAML/JSON
  created_at TEXT NOT NULL
);

CREATE TABLE saved_view (
  id TEXT PRIMARY KEY, case_id TEXT NOT NULL REFERENCES kase(id) ON DELETE CASCADE,
  name TEXT NOT NULL, spec TEXT NOT NULL, created_at TEXT NOT NULL
);
```

---

## 4.3 Normalisation {#normalisation}

The raw value is sacred; the normalised value is derived and disposable. Both
are stored. When a normalisation rule changes, `vesica rescan-matches`
recomputes `value_norm` and `block_key` from the raw values — no data is lost.

Normalisation happens in exactly one place, `vesica-core::normalize`, and both
the write path and the matcher call it. This is why a phone number entered as
`0601 234 567` in a case whose `default_region` is `PL` matches
`+48601234567` entered in another case.

## 4.4 Graph traversal in SQL

Neighbourhood expansion and shortest path are recursive CTEs over `relation`,
optionally hopping `entity_link` to cross case boundaries:

```sql
WITH RECURSIVE reach(id, depth) AS (
  SELECT :start, 0
  UNION
  SELECT CASE WHEN r.source_id_ = reach.id THEN r.target_id ELSE r.source_id_ END,
         reach.depth + 1
  FROM reach
  JOIN relation r ON (r.source_id_ = reach.id OR r.target_id = reach.id)
                 AND r.deleted_at IS NULL
  WHERE reach.depth < :max_depth
)
SELECT id, MIN(depth) AS depth FROM reach GROUP BY id LIMIT :budget;
```

Every traversal is bounded by both a depth limit and a node budget, and returns
a `truncated` flag so the UI and agents know the answer is partial rather than
complete.

## 4.5 What we borrow from FollowTheMoney, and what we don't

**Borrowed**: the schema/property/type vocabulary, the property type list, the
idea that relations are entities with intervals, the built-in schema names, and
export compatibility so data can move into Aleph or be checked against
OpenSanctions.

**Not borrowed**: FtM's full 70-schema catalogue (we ship ~30 and let people add
more), its RDF/OWL projection, and its document-first orientation. FtM models
edges as entities with `Interval` semantics; we model them as first-class rows
with their own property table, which is cheaper to query and maps to the
export on the way out.
