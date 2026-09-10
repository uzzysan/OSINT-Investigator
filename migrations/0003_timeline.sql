-- 0003 timeline: dated observations about a case, an entity or a relation.
-- Mirrors 04-data-model.md section 4.2.3.
--
-- The timeline itself is a view over three producers (observation rows,
-- relation intervals, date-typed entity_prop rows). Only the `observation`
-- producer is a table; the materialised `timeline_event` table the data model
-- mentions in passing is not part of this ticket's table list and is not
-- created here.

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
CREATE INDEX ix_obs_case    ON observation(case_id, date_start);
CREATE INDEX ix_obs_subject ON observation(subject_kind, subject_id);
