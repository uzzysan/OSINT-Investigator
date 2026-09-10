-- 0001 core: cases, entities, their properties, and relations.
--
-- Mirrors 04-data-model.md section 4.2.1. The table is `kase` because `case`
-- is a reserved word in SQL, and the relation's source column is `source_id_`
-- so it cannot be confused with a reference to the `source` (evidence) table.
--
-- `entity_prop.source_id` references `source(id)`, which migration 0002
-- creates. SQLite resolves foreign-key targets when a row is written, not when
-- the table is declared, so the forward reference is legal here and is
-- satisfied by the time any data exists; `schema_is_referentially_sound` in
-- the migration tests runs `PRAGMA foreign_key_check` to prove it.

CREATE TABLE kase (                       -- "case" is reserved in SQL
  id             TEXT PRIMARY KEY,
  slug           TEXT NOT NULL UNIQUE,
  name           TEXT NOT NULL,
  description    TEXT,
  color          TEXT,
  status         TEXT NOT NULL DEFAULT 'active',  -- active | archived
  default_region TEXT,                    -- ISO country, for phone parsing
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  deleted_at     TEXT
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
CREATE INDEX ix_relprop_relation ON relation_prop(relation_id);
