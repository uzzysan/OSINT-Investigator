-- 0002 evidence: where a fact came from, and the bytes behind it.
-- Mirrors 04-data-model.md section 4.2.2.

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
CREATE INDEX ix_attachment_source ON attachment(source_id);
CREATE INDEX ix_attachment_blob   ON attachment(sha256);

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
CREATE INDEX ix_citation_source ON citation(source_id);
