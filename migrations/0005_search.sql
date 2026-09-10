-- 0005 search: the FTS5 index over entity captions and property values,
-- kept in sync by triggers on `entity` and `entity_prop`.
-- Mirrors 04-data-model.md section 4.2.5.
--
-- The index row for an entity is stored at the entity's own rowid, so both the
-- delete and the rebuild are O(1) lookups rather than a scan of the whole
-- index. `props` is the concatenation of every property value of the entity,
-- so a rebuild is the only correct response to any property change; each
-- trigger therefore deletes and re-inserts the whole row.
--
-- The rebuild is written as INSERT ... SELECT FROM entity so it is a no-op
-- when the entity is already gone. That matters: deleting an entity cascades
-- into entity_prop, and those cascaded deletes fire the property triggers
-- below after the parent row has disappeared.

CREATE VIRTUAL TABLE entity_fts USING fts5(
  entity_id UNINDEXED,
  case_id   UNINDEXED,
  caption,
  props,                                  -- all searchable values concatenated
  tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER trg_entity_fts_insert AFTER INSERT ON entity BEGIN
  INSERT INTO entity_fts(rowid, entity_id, case_id, caption, props)
  VALUES (
    new.rowid, new.id, new.case_id, new.caption,
    coalesce((SELECT group_concat(p.value, ' ')
                FROM entity_prop p WHERE p.entity_id = new.id), '')
  );
END;

CREATE TRIGGER trg_entity_fts_update AFTER UPDATE ON entity BEGIN
  DELETE FROM entity_fts WHERE rowid = old.rowid;
  INSERT INTO entity_fts(rowid, entity_id, case_id, caption, props)
  VALUES (
    new.rowid, new.id, new.case_id, new.caption,
    coalesce((SELECT group_concat(p.value, ' ')
                FROM entity_prop p WHERE p.entity_id = new.id), '')
  );
END;

CREATE TRIGGER trg_entity_fts_delete AFTER DELETE ON entity BEGIN
  DELETE FROM entity_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER trg_prop_fts_insert AFTER INSERT ON entity_prop BEGIN
  DELETE FROM entity_fts
    WHERE rowid = (SELECT e.rowid FROM entity e WHERE e.id = new.entity_id);
  INSERT INTO entity_fts(rowid, entity_id, case_id, caption, props)
  SELECT e.rowid, e.id, e.case_id, e.caption,
         coalesce((SELECT group_concat(p.value, ' ')
                     FROM entity_prop p WHERE p.entity_id = e.id), '')
    FROM entity e WHERE e.id = new.entity_id;
END;

CREATE TRIGGER trg_prop_fts_update AFTER UPDATE ON entity_prop BEGIN
  DELETE FROM entity_fts
    WHERE rowid = (SELECT e.rowid FROM entity e WHERE e.id = old.entity_id);
  DELETE FROM entity_fts
    WHERE rowid = (SELECT e.rowid FROM entity e WHERE e.id = new.entity_id);
  INSERT INTO entity_fts(rowid, entity_id, case_id, caption, props)
  SELECT e.rowid, e.id, e.case_id, e.caption,
         coalesce((SELECT group_concat(p.value, ' ')
                     FROM entity_prop p WHERE p.entity_id = e.id), '')
    FROM entity e WHERE e.id = old.entity_id;
  INSERT INTO entity_fts(rowid, entity_id, case_id, caption, props)
  SELECT e.rowid, e.id, e.case_id, e.caption,
         coalesce((SELECT group_concat(p.value, ' ')
                     FROM entity_prop p WHERE p.entity_id = e.id), '')
    FROM entity e WHERE e.id = new.entity_id AND new.entity_id <> old.entity_id;
END;

CREATE TRIGGER trg_prop_fts_delete AFTER DELETE ON entity_prop BEGIN
  DELETE FROM entity_fts
    WHERE rowid = (SELECT e.rowid FROM entity e WHERE e.id = old.entity_id);
  INSERT INTO entity_fts(rowid, entity_id, case_id, caption, props)
  SELECT e.rowid, e.id, e.case_id, e.caption,
         coalesce((SELECT group_concat(p.value, ' ')
                     FROM entity_prop p WHERE p.entity_id = e.id), '')
    FROM entity e WHERE e.id = old.entity_id;
END;
