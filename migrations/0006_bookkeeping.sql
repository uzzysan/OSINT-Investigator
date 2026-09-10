-- 0006 bookkeeping: tags, the audit trail, custom schema definitions and
-- saved views.
-- Mirrors 04-data-model.md section 4.2.6.

CREATE TABLE tag (
  id TEXT PRIMARY KEY, case_id TEXT REFERENCES kase(id) ON DELETE CASCADE,
  name TEXT NOT NULL, color TEXT, UNIQUE (case_id, name)
);

CREATE TABLE entity_tag (
  entity_id TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  tag_id    TEXT NOT NULL REFERENCES tag(id)    ON DELETE CASCADE,
  PRIMARY KEY (entity_id, tag_id)
);
CREATE INDEX ix_entity_tag_tag ON entity_tag(tag_id);

CREATE TABLE audit_log (
  id          INTEGER PRIMARY KEY,
  at          TEXT NOT NULL,
  actor       TEXT NOT NULL,              -- 'human' | agent name
  action      TEXT NOT NULL,              -- create|update|delete|merge|link|import
  case_id     TEXT,
  target_kind TEXT NOT NULL, target_id TEXT NOT NULL,
  before      TEXT, after TEXT,           -- JSON
  batch_id    TEXT                        -- groups an import/merge for undo
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
CREATE INDEX ix_saved_view_case ON saved_view(case_id);
