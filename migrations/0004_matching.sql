-- 0004 matching and linking: the durable resolver queue, the blocking index,
-- proposed pairs and confirmed same-as links.
-- Mirrors 04-data-model.md section 4.2.4.

CREATE TABLE match_job (                  -- durable work queue
  entity_id   TEXT PRIMARY KEY REFERENCES entity(id) ON DELETE CASCADE,
  enqueued_at TEXT NOT NULL,
  attempts    INTEGER NOT NULL DEFAULT 0
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
  UNIQUE (left_id, right_id),
  -- The data model states "left_id < right_id enforced on write". The pair is
  -- unordered, so without this the same pair can be stored twice under two
  -- orderings and the UNIQUE above stops catching it. Enforced here as well as
  -- on the write path, because a tombstone that silently stops working is the
  -- kind of bug nobody notices.
  CHECK (left_id < right_id)
);
CREATE INDEX ix_match_open  ON match_candidate(status, score DESC);
CREATE INDEX ix_match_left  ON match_candidate(left_id);
CREATE INDEX ix_match_right ON match_candidate(right_id);

CREATE TABLE entity_link (                -- confirmed same-as, cross-case
  id          TEXT PRIMARY KEY,
  a_id        TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  b_id        TEXT NOT NULL REFERENCES entity(id) ON DELETE CASCADE,
  kind        TEXT NOT NULL DEFAULT 'same_as',
  note        TEXT,
  created_at  TEXT NOT NULL, created_by TEXT NOT NULL,
  UNIQUE (a_id, b_id)
);
CREATE INDEX ix_entity_link_b ON entity_link(b_id);
