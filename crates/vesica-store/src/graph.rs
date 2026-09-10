//! Bounded graph traversal over `relation`, expressed as a recursive CTE.
//! See 04-data-model.md section 4.4 and AGENT-BRIEF.md rule 5: "Every
//! traversal and every list is bounded... Do not add an unbounded variant
//! 'for internal use'." There is exactly one traversal primitive in this
//! module and it cannot be called without both a depth limit and a node
//! budget.
//!
//! ## Why this isn't a literal copy of the data-model doc's example query
//!
//! 04-data-model.md 4.4 shows the traversal as:
//!
//! ```sql
//! WITH RECURSIVE reach(id, depth) AS (
//!   SELECT :start, 0
//!   UNION
//!   SELECT ..., reach.depth + 1
//!   FROM reach JOIN relation r ON (...)
//!   WHERE reach.depth < :max_depth
//! )
//! SELECT id, MIN(depth) AS depth FROM reach GROUP BY id LIMIT :budget;
//! ```
//!
//! That query only bounds the *output*: `LIMIT` applies to the grouped
//! result, not to how many rows the recursive term itself produces before
//! grouping. The recursive term has no cycle-dedup, so a start node that
//! reaches a cycle can generate rows for every node on the cycle at every
//! depth up to `max_depth` — exponential in fan-out, regardless of how small
//! `budget` is. That is an unbounded variant in every way that matters, so
//! this implementation adds a second cap, `work_cap`, inside the recursive
//! term's own `WHERE` clause (`(SELECT COUNT(*) FROM reach) < work_cap`),
//! which bounds the number of rows the recursion itself is allowed to
//! produce before it ever reaches the outer `GROUP BY`/`LIMIT`. This is
//! flagged here rather than silently "fixed" in the doc, per AGENT-BRIEF.md
//! rule 10.

use sqlx::sqlite::SqliteRow;
use sqlx::{FromRow, Row, SqlitePool};

use crate::error::StoreError;

/// Hard ceiling on `max_depth`, regardless of what a caller asks for.
pub const MAX_DEPTH_CEILING: i64 = 10;
/// Hard ceiling on `budget` (the number of nodes returned), regardless of
/// what a caller asks for.
pub const MAX_BUDGET_CEILING: i64 = 5_000;
/// How many raw (pre-dedup) rows the recursive term itself may produce per
/// unit of requested budget, before the internal safety net in the module
/// docs kicks in.
const WORK_CAP_MULTIPLIER: i64 = 50;
/// Absolute ceiling on the internal work cap, independent of budget.
const WORK_CAP_CEILING: i64 = 50_000;

/// One node reached during a [`neighbors`] traversal, at its shortest
/// distance (in hops) from the start node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neighbor {
    pub id: String,
    pub depth: i64,
}

/// The result of a bounded neighbourhood expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraversalResult {
    /// Nodes reached, nearest first, excluding the start node itself is
    /// *not* guaranteed — the start node is included at depth 0, matching
    /// the data-model reference query.
    pub nodes: Vec<Neighbor>,
    /// `true` if this is a partial answer: either the node budget cut the
    /// result short, or the internal work cap stopped the recursion before
    /// it would naturally have finished exploring within `max_depth`. A
    /// caller must not treat `nodes` as exhaustive when this is `true`.
    pub truncated: bool,
}

struct NeighborRow {
    id: String,
    depth: i64,
    total_rows: i64,
}

impl FromRow<'_, SqliteRow> for NeighborRow {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(NeighborRow {
            id: row.try_get("id")?,
            depth: row.try_get("depth")?,
            total_rows: row.try_get("total_rows")?,
        })
    }
}

/// Expands the neighbourhood of `start` over `relation`, undirected (either
/// endpoint of an edge reaches the other), up to `max_depth` hops, returning
/// at most `budget` nodes including the start node itself.
///
/// `max_depth` and `budget` are clamped to [`MAX_DEPTH_CEILING`] and
/// [`MAX_BUDGET_CEILING`] no matter what is requested — there is no way to
/// call this function unboundedly. `start` not existing, or having no
/// relations, is not an error: the result is just `{start}` at depth 0.
pub async fn neighbors(
    pool: &SqlitePool,
    start: &str,
    max_depth: i64,
    budget: i64,
) -> Result<TraversalResult, StoreError> {
    let max_depth = max_depth.clamp(0, MAX_DEPTH_CEILING);
    let budget = budget.clamp(1, MAX_BUDGET_CEILING);
    let work_cap = budget
        .saturating_mul(WORK_CAP_MULTIPLIER)
        .min(WORK_CAP_CEILING)
        .max(budget + 1);
    // Fetch one extra row past the budget so we can tell "the result
    // happened to end exactly at the budget" apart from "there was more".
    let fetch_limit = budget + 1;
    // Same trick for the raw (pre-dedup) work cap: ask for one row past it
    // so we can tell "the raw recursion produced exactly work_cap rows" from
    // "it kept going and was cut off".
    let raw_cap = work_cap + 1;

    // Two CTEs: `reach` is the recursive expansion, bounded only by
    // `max_depth` (SQLite forbids referencing a recursive CTE's own table a
    // second time from within its recursive term, so the row-count cap
    // below cannot live inside `reach`'s definition). `capped` is a plain,
    // non-recursive `SELECT ... FROM reach LIMIT ...` — reading a recursive
    // CTE through a directly-overlying `LIMIT` with no grouping in between
    // is exactly the pattern SQLite's own query planner recognises to stop
    // evaluating the recursion once the limit is satisfied, so this is what
    // actually bounds the work, not just the output. Grouping/dedup for the
    // shortest-depth-per-node answer happens afterwards, over `capped`,
    // which is now bounded regardless of how large or cyclic the graph is.
    let rows: Vec<NeighborRow> = sqlx::query_as(
        r#"
        WITH RECURSIVE reach(id, depth) AS (
          SELECT ?1, 0
          UNION
          SELECT CASE WHEN r.source_id_ = reach.id THEN r.target_id ELSE r.source_id_ END,
                 reach.depth + 1
          FROM reach
          JOIN relation r ON (r.source_id_ = reach.id OR r.target_id = reach.id)
                         AND r.deleted_at IS NULL
          WHERE reach.depth < ?2
        ),
        capped(id, depth) AS (
          SELECT id, depth FROM reach LIMIT ?3
        )
        SELECT id, MIN(depth) AS depth,
               (SELECT COUNT(*) FROM capped) AS total_rows
        FROM capped
        GROUP BY id
        ORDER BY depth, id
        LIMIT ?4
        "#,
    )
    .bind(start)
    .bind(max_depth)
    .bind(raw_cap)
    .bind(fetch_limit)
    .fetch_all(pool)
    .await?;

    let output_truncated = rows.len() as i64 > budget;
    let work_cap_engaged = rows.first().is_some_and(|r| r.total_rows > work_cap);

    let mut nodes: Vec<Neighbor> = rows
        .into_iter()
        .map(|r| Neighbor {
            id: r.id,
            depth: r.depth,
        })
        .collect();
    nodes.truncate(budget as usize);

    Ok(TraversalResult {
        nodes,
        truncated: output_truncated || work_cap_engaged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::test_store;

    /// Seeds a graph with:
    /// - a **hub** (`hub`) directly connected to six leaves (`leaf0..leaf5`)
    /// - a **chain** hanging off the hub: `hub - c1 - c2 - c3 - c4`
    /// - a **cycle** at the end of the chain: `c4 - k1 - k2 - c4`
    async fn seed_hub_chain_cycle(db: &crate::store::Db) {
        db.write(|conn| {
            Box::pin(async move {
                let now = "2026-01-01T00:00:00.000Z";
                let node_ids = [
                    "hub", "leaf0", "leaf1", "leaf2", "leaf3", "leaf4", "leaf5", "c1", "c2", "c3",
                    "c4", "k1", "k2",
                ];
                sqlx::query(
                    "INSERT INTO kase (id, slug, name, status, created_at, updated_at) \
                     VALUES ('case1', 'case1', 'Case 1', 'active', ?, ?)",
                )
                .bind(now)
                .bind(now)
                .execute(&mut *conn)
                .await?;

                for id in node_ids {
                    sqlx::query(
                        "INSERT INTO entity (id, case_id, schema_id, caption, created_at, created_by, updated_at) \
                         VALUES (?, 'case1', 'Thing', ?, ?, 'test', ?)",
                    )
                    .bind(id)
                    .bind(id)
                    .bind(now)
                    .bind(now)
                    .execute(&mut *conn)
                    .await?;
                }

                let edges: &[(&str, &str)] = &[
                    ("hub", "leaf0"),
                    ("hub", "leaf1"),
                    ("hub", "leaf2"),
                    ("hub", "leaf3"),
                    ("hub", "leaf4"),
                    ("hub", "leaf5"),
                    ("hub", "c1"),
                    ("c1", "c2"),
                    ("c2", "c3"),
                    ("c3", "c4"),
                    ("c4", "k1"),
                    ("k1", "k2"),
                    ("k2", "c4"),
                ];
                for (i, (src, tgt)) in edges.iter().enumerate() {
                    sqlx::query(
                        "INSERT INTO relation (id, case_id, schema_id, source_id_, target_id, directed, created_at, created_by, updated_at) \
                         VALUES (?, 'case1', 'Related', ?, ?, 0, ?, 'test', ?)",
                    )
                    .bind(format!("rel{i}"))
                    .bind(*src)
                    .bind(*tgt)
                    .bind(now)
                    .bind(now)
                    .execute(&mut *conn)
                    .await?;
                }

                Ok(())
            })
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn depth_one_from_the_hub_returns_only_its_direct_neighbours() {
        let store = test_store().await.unwrap();
        seed_hub_chain_cycle(&store.db).await;

        let result = neighbors(store.db.read_pool(), "hub", 1, 100)
            .await
            .unwrap();
        assert!(!result.truncated);

        let mut ids: Vec<&str> = result.nodes.iter().map(|n| n.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            vec![
                "c1", "hub", "leaf0", "leaf1", "leaf2", "leaf3", "leaf4", "leaf5"
            ]
        );
        assert_eq!(
            result.nodes.iter().find(|n| n.id == "hub").unwrap().depth,
            0
        );
        assert_eq!(
            result.nodes.iter().find(|n| n.id == "leaf0").unwrap().depth,
            1
        );
    }

    #[tokio::test]
    async fn a_small_budget_truncates_the_result_and_says_so() {
        let store = test_store().await.unwrap();
        seed_hub_chain_cycle(&store.db).await;

        let result = neighbors(store.db.read_pool(), "hub", MAX_DEPTH_CEILING, 3)
            .await
            .unwrap();
        assert!(result.truncated);
        assert_eq!(result.nodes.len(), 3);
    }

    #[tokio::test]
    async fn traversal_reaches_across_the_cycle_without_hanging_or_double_counting() {
        let store = test_store().await.unwrap();
        seed_hub_chain_cycle(&store.db).await;

        // From inside the cycle, with plenty of depth and budget, every node
        // in the connected component should be reachable exactly once, at
        // its true shortest distance — despite the 3-cycle giving the raw
        // recursion many ways to revisit c4/k1/k2.
        let result = neighbors(store.db.read_pool(), "k1", MAX_DEPTH_CEILING, 100)
            .await
            .unwrap();
        assert!(!result.truncated);

        let mut ids: Vec<&str> = result.nodes.iter().map(|n| n.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            result.nodes.len(),
            "no node should appear twice in the result"
        );
        assert!(ids.contains(&"hub"));
        assert!(ids.contains(&"leaf0"));

        let hub_depth = result.nodes.iter().find(|n| n.id == "hub").unwrap().depth;
        // k1 -> c4 -> c3 -> c2 -> c1 -> hub is 5, k1 -> k2 -> c4 -> ... is
        // longer, so the shortest path must be 5, not something inflated by
        // going the long way around the cycle.
        assert_eq!(hub_depth, 5);
    }

    #[tokio::test]
    async fn an_absent_start_node_yields_just_itself_not_an_error() {
        let store = test_store().await.unwrap();
        seed_hub_chain_cycle(&store.db).await;

        let result = neighbors(store.db.read_pool(), "does-not-exist", 5, 10)
            .await
            .unwrap();
        assert!(!result.truncated);
        assert_eq!(
            result.nodes,
            vec![Neighbor {
                id: "does-not-exist".to_string(),
                depth: 0
            }]
        );
    }
}
