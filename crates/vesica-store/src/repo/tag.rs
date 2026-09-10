//! Persistence for `tag` and `entity_tag` — 04-data-model.md 4.2.6.
//!
//! Free functions over a `sqlx` executor, for the reason [`super::case`]
//! gives: attaching a tag is a mutation, a mutation writes an `audit_log`
//! row, and the two have to commit together.
//!
//! A tag is scoped to a case (`UNIQUE (case_id, name)`). Lookup by name is
//! `COLLATE NOCASE` so that `Suspect` and `suspect` are one tag rather than
//! two — the name is stored exactly as the first caller spelled it, and a
//! later caller asking for a different casing gets the existing tag rather
//! than a near-duplicate. The column's own `UNIQUE` index is binary, so
//! that behaviour lives in [`find_by_name`] and every write path goes
//! through it.

use sqlx::FromRow;
use sqlx::SqliteConnection;
use sqlx::sqlite::SqliteExecutor;

use crate::error::StoreError;

/// Largest number of rows [`list_for_case`] and [`for_entity`] return,
/// whatever they are asked for. AGENT-BRIEF.md rule 5: every list is
/// bounded.
pub const MAX_LIST_LIMIT: i64 = 1_000;

/// A `tag` row exactly as stored.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct TagRow {
    pub id: String,
    /// The case the tag belongs to. Null is possible in the schema (a
    /// deployment-wide tag); nothing writes one today.
    pub case_id: Option<String>,
    pub name: String,
    pub color: Option<String>,
}

/// A tag and how many live entities in the case carry it.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct TagUsage {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    /// Entities carrying this tag that are neither deleted nor merged away.
    pub entity_count: i64,
}

/// The tag with this name in this case, matched case-insensitively.
pub async fn find_by_name<'e, E>(
    exec: E,
    case_id: &str,
    name: &str,
) -> Result<Option<TagRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, TagRow>(
        "SELECT id, case_id, name, color FROM tag \
         WHERE case_id = ? AND name = ? COLLATE NOCASE LIMIT 1",
    )
    .bind(case_id)
    .bind(name)
    .fetch_optional(exec)
    .await?;
    Ok(row)
}

/// Inserts a tag exactly as given.
pub async fn insert(conn: &mut SqliteConnection, row: &TagRow) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO tag (id, case_id, name, color) VALUES (?, ?, ?, ?)")
        .bind(&row.id)
        .bind(&row.case_id)
        .bind(&row.name)
        .bind(&row.color)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Every tag in one case that is carried by at least one live entity, by
/// name. Bounded by [`MAX_LIST_LIMIT`].
pub async fn list_for_case<'e, E>(
    exec: E,
    case_id: &str,
    limit: i64,
) -> Result<Vec<TagUsage>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, TagUsage>(
        "SELECT t.id AS id, t.name AS name, t.color AS color, count(e.id) AS entity_count \
         FROM tag t \
         JOIN entity_tag et ON et.tag_id = t.id \
         JOIN entity e ON e.id = et.entity_id \
              AND e.deleted_at IS NULL AND e.merged_into IS NULL \
         WHERE t.case_id = ? \
         GROUP BY t.id, t.name, t.color \
         ORDER BY t.name ASC LIMIT ?",
    )
    .bind(case_id)
    .bind(limit.clamp(0, MAX_LIST_LIMIT))
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// The tags one entity carries, by name. Bounded by [`MAX_LIST_LIMIT`].
pub async fn for_entity<'e, E>(exec: E, entity_id: &str) -> Result<Vec<TagRow>, StoreError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, TagRow>(
        "SELECT t.id AS id, t.case_id AS case_id, t.name AS name, t.color AS color \
         FROM tag t JOIN entity_tag et ON et.tag_id = t.id \
         WHERE et.entity_id = ? ORDER BY t.name ASC LIMIT ?",
    )
    .bind(entity_id)
    .bind(MAX_LIST_LIMIT)
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

/// Attaches a tag to an entity. Returns whether this changed anything —
/// attaching a tag the entity already carries is a no-op, not an error.
pub async fn attach(
    conn: &mut SqliteConnection,
    entity_id: &str,
    tag_id: &str,
) -> Result<bool, StoreError> {
    let result = sqlx::query("INSERT OR IGNORE INTO entity_tag (entity_id, tag_id) VALUES (?, ?)")
        .bind(entity_id)
        .bind(tag_id)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Detaches a tag from an entity. Returns whether it was attached.
pub async fn detach(
    conn: &mut SqliteConnection,
    entity_id: &str,
    tag_id: &str,
) -> Result<bool, StoreError> {
    let result = sqlx::query("DELETE FROM entity_tag WHERE entity_id = ? AND tag_id = ?")
        .bind(entity_id)
        .bind(tag_id)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Deletes a tag that nothing carries any more, so that a case's tag list
/// stays a list of the labels actually in use. Returns whether the row went.
/// A tag still attached to anything — including a soft-deleted entity, whose
/// tags have to survive a restore — is left alone.
pub async fn delete_if_unused(
    conn: &mut SqliteConnection,
    tag_id: &str,
) -> Result<bool, StoreError> {
    let result = sqlx::query(
        "DELETE FROM tag WHERE id = ? \
           AND NOT EXISTS (SELECT 1 FROM entity_tag et WHERE et.tag_id = tag.id)",
    )
    .bind(tag_id)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids;
    use crate::repo::entity::{self, EntityRow};
    use crate::testing::{seed_case, test_store};

    async fn seed_entity(db: &crate::Db, case_id: &str, id: &str) {
        let row = EntityRow {
            id: id.into(),
            case_id: case_id.into(),
            schema_id: "Person".into(),
            caption: id.into(),
            confidence: "probable".into(),
            extra: None,
            created_at: ids::now(),
            created_by: "human".into(),
            updated_at: ids::now(),
            deleted_at: None,
            merged_into: None,
        };
        db.transaction(move |conn| Box::pin(async move { entity::insert(conn, &row).await }))
            .await
            .unwrap();
    }

    async fn seed_tag(db: &crate::Db, case_id: &str, name: &str) -> TagRow {
        let row = TagRow {
            id: ids::new_id(),
            case_id: Some(case_id.into()),
            name: name.into(),
            color: None,
        };
        let stored = row.clone();
        db.transaction(move |conn| Box::pin(async move { insert(conn, &stored).await }))
            .await
            .unwrap();
        row
    }

    #[tokio::test]
    async fn a_tag_is_found_whatever_case_the_caller_spells_it_in() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        let tag = seed_tag(&store.db, &case.id, "Suspect").await;

        for spelling in ["Suspect", "suspect", "SUSPECT"] {
            let found = find_by_name(store.db.read_pool(), &case.id, spelling)
                .await
                .unwrap();
            assert_eq!(found.as_ref().map(|t| t.id.as_str()), Some(tag.id.as_str()));
        }
        assert!(
            find_by_name(store.db.read_pool(), &case.id, "witness")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn the_same_name_in_another_case_is_another_tag() {
        let store = test_store().await.unwrap();
        let one = seed_case(&store.db, "one").await.unwrap();
        let two = seed_case(&store.db, "two").await.unwrap();
        let a = seed_tag(&store.db, &one.id, "suspect").await;
        let b = seed_tag(&store.db, &two.id, "suspect").await;
        assert_ne!(a.id, b.id, "tags are scoped to their case");
    }

    #[tokio::test]
    async fn attach_is_idempotent_and_detach_reports_what_it_did() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        seed_entity(&store.db, &case.id, "id-a").await;
        let tag = seed_tag(&store.db, &case.id, "suspect").await;

        let id = tag.id.clone();
        let first = store
            .db
            .transaction(move |conn| Box::pin(async move { attach(conn, "id-a", &id).await }))
            .await
            .unwrap();
        assert!(first);

        let id = tag.id.clone();
        let again = store
            .db
            .transaction(move |conn| Box::pin(async move { attach(conn, "id-a", &id).await }))
            .await
            .unwrap();
        assert!(!again, "attaching twice changes nothing");

        let names: Vec<String> = for_entity(store.db.read_pool(), "id-a")
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, vec!["suspect".to_string()]);

        let id = tag.id.clone();
        assert!(
            store
                .db
                .transaction(move |conn| Box::pin(async move { detach(conn, "id-a", &id).await }))
                .await
                .unwrap()
        );
        let id = tag.id.clone();
        assert!(
            !store
                .db
                .transaction(move |conn| Box::pin(async move { detach(conn, "id-a", &id).await }))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn the_case_listing_counts_live_entities_only_and_an_unused_tag_can_be_swept() {
        let store = test_store().await.unwrap();
        let case = seed_case(&store.db, "op-lighthouse").await.unwrap();
        seed_entity(&store.db, &case.id, "id-a").await;
        seed_entity(&store.db, &case.id, "id-b").await;
        let tag = seed_tag(&store.db, &case.id, "suspect").await;

        for entity_id in ["id-a", "id-b"] {
            let id = tag.id.clone();
            store
                .db
                .transaction(move |conn| {
                    Box::pin(async move { attach(conn, entity_id, &id).await })
                })
                .await
                .unwrap();
        }

        let listed = list_for_case(store.db.read_pool(), &case.id, MAX_LIST_LIMIT)
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].entity_count, 2);

        // A soft-deleted entity stops counting towards the tag.
        let mut row = entity::find(store.db.read_pool(), "id-b")
            .await
            .unwrap()
            .unwrap();
        row.deleted_at = Some(ids::now());
        store
            .db
            .transaction(move |conn| Box::pin(async move { entity::save(conn, &row).await }))
            .await
            .unwrap();
        let listed = list_for_case(store.db.read_pool(), &case.id, MAX_LIST_LIMIT)
            .await
            .unwrap();
        assert_eq!(listed[0].entity_count, 1);

        // ... but the tag row survives, because a restore has to bring the
        // deleted entity's tags back with it.
        let id = tag.id.clone();
        assert!(
            !store
                .db
                .transaction(move |conn| Box::pin(async move { delete_if_unused(conn, &id).await }))
                .await
                .unwrap()
        );

        for entity_id in ["id-a", "id-b"] {
            let id = tag.id.clone();
            store
                .db
                .transaction(move |conn| {
                    Box::pin(async move { detach(conn, entity_id, &id).await })
                })
                .await
                .unwrap();
        }
        let id = tag.id.clone();
        assert!(
            store
                .db
                .transaction(move |conn| Box::pin(async move { delete_if_unused(conn, &id).await }))
                .await
                .unwrap()
        );
        assert!(
            find_by_name(store.db.read_pool(), &case.id, "suspect")
                .await
                .unwrap()
                .is_none()
        );
    }
}
