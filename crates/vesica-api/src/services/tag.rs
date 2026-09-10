//! Tagging: the labels an analyst puts on Entities to group things the
//! ontology has no opinion about.
//!
//! A tag is a name, scoped to a Case. It is created by being used and it
//! disappears when nothing carries it any more, so a Case's tag list is
//! always the labels actually in use rather than an accumulating vocabulary
//! nobody pruned. Lookup is case-insensitive: `Suspect` and `suspect` are
//! one tag, spelled the way whoever first used it spelled it, because two
//! tags that differ only in capitalisation are a filing error waiting to
//! happen.
//!
//! Attaching and detaching are mutations like any other: each writes an
//! `audit_log` row, in the same transaction, saying who did it.

use serde::{Deserialize, Serialize};
use vesica_store::repo::{TagRow, audit, tag};

use super::case::AuditAction;
use super::entity::{Entity, TARGET_ENTITY, entry, snapshot};
use super::error::{Actor, ServiceError};
use super::{AuditPage, Services};

/// Longest a tag name may be. A tag is a label, not a sentence.
const MAX_TAG_LEN: usize = 64;

/// One tag in use in a Case, and how much of the Case carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagSummary {
    /// ULID, 26 characters.
    pub id: String,
    pub name: String,
    /// A `#rrggbb` colour, if one was ever set. Nothing sets one yet.
    pub color: Option<String>,
    /// Live Entities carrying this tag. Deleted ones do not count, and are
    /// not lost either: restoring one brings its tags back with it.
    pub entity_count: usize,
}

/// The tags in use in one Case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagList {
    /// In name order.
    pub tags: Vec<TagSummary>,
    /// Whether there were more than the budget allowed.
    pub truncated: bool,
}

impl Services {
    /// Attaches a tag to an Entity, creating the tag in that Entity's Case
    /// if this is the first time it has been used there.
    ///
    /// Attaching a tag the Entity already carries changes nothing and
    /// records nothing, so an agent that cannot remember what it has
    /// already done can say it twice safely.
    pub async fn tag_entity(
        &self,
        entity_id: &str,
        name: &str,
        actor: &Actor,
    ) -> Result<Entity, ServiceError> {
        let name = validate_tag(name)?;
        let entity_id = entity_id.to_string();
        let actor = actor.as_str().to_string();
        let schemas = self.schemas_handle();

        self.tx(move |conn| {
            Box::pin(async move {
                let row = super::entity::live_entity(conn, &entity_id).await?;
                let before = super::entity::load_row(conn, row.clone(), &schemas).await?;
                if before
                    .tags
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&name))
                {
                    return Ok(before);
                }

                let tag_id = match tag::find_by_name(&mut *conn, &row.case_id, &name).await? {
                    Some(existing) => existing.id,
                    None => {
                        let created = TagRow {
                            id: vesica_store::new_id(),
                            case_id: Some(row.case_id.clone()),
                            name: name.clone(),
                            color: None,
                        };
                        tag::insert(conn, &created).await?;
                        created.id
                    }
                };

                let mut after = before.clone();
                after.tags.push(name.clone());
                after.tags.sort();

                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Tag,
                        &row.case_id,
                        &row.id,
                        snapshot(&before),
                        snapshot(&after),
                    ),
                )
                .await?;
                tag::attach(conn, &row.id, &tag_id).await?;

                super::entity::load_row(conn, row, &schemas).await
            })
        })
        .await
    }

    /// Detaches a tag from an Entity. Detaching one it does not carry
    /// changes nothing and records nothing.
    ///
    /// A tag that nothing carries any more goes with it: the Case's tag
    /// list is a list of labels in use, not a graveyard.
    pub async fn untag_entity(
        &self,
        entity_id: &str,
        name: &str,
        actor: &Actor,
    ) -> Result<Entity, ServiceError> {
        let name = validate_tag(name)?;
        let entity_id = entity_id.to_string();
        let actor = actor.as_str().to_string();
        let schemas = self.schemas_handle();

        self.tx(move |conn| {
            Box::pin(async move {
                let row = super::entity::live_entity(conn, &entity_id).await?;
                let before = super::entity::load_row(conn, row.clone(), &schemas).await?;

                let Some(existing) = tag::find_by_name(&mut *conn, &row.case_id, &name).await?
                else {
                    return Ok(before);
                };
                if !before
                    .tags
                    .iter()
                    .any(|carried| carried.eq_ignore_ascii_case(&existing.name))
                {
                    return Ok(before);
                }

                let mut after = before.clone();
                after.tags.retain(|carried| carried != &existing.name);

                audit::record(
                    conn,
                    entry(
                        &actor,
                        AuditAction::Untag,
                        &row.case_id,
                        &row.id,
                        snapshot(&before),
                        snapshot(&after),
                    ),
                )
                .await?;
                tag::detach(conn, &row.id, &existing.id).await?;
                tag::delete_if_unused(conn, &existing.id).await?;

                super::entity::load_row(conn, row, &schemas).await
            })
        })
        .await
    }

    /// The tags in use in one Case, with how many live Entities carry each.
    /// Bounded like every other list.
    pub async fn list_tags(
        &self,
        case_id: &str,
        limit: Option<usize>,
    ) -> Result<TagList, ServiceError> {
        let limit = self.page_size(limit);
        let mut rows = tag::list_for_case(
            self.db().read_pool(),
            case_id,
            (limit as i64).saturating_add(1),
        )
        .await?;
        let truncated = rows.len() > limit;
        rows.truncate(limit);

        Ok(TagList {
            tags: rows
                .into_iter()
                .map(|row| TagSummary {
                    id: row.id,
                    name: row.name,
                    color: row.color,
                    entity_count: row.entity_count.max(0) as usize,
                })
                .collect(),
            truncated,
        })
    }

    /// The audit trail for one Entity's tagging is part of its own trail;
    /// this is here so a caller that only cares about tags does not have to
    /// filter the whole history itself.
    pub async fn entity_tag_history(
        &self,
        entity_id: &str,
        limit: Option<usize>,
    ) -> Result<AuditPage, ServiceError> {
        let mut page = self
            .audit_for_target(TARGET_ENTITY, entity_id, limit)
            .await?;
        page.entries
            .retain(|record| matches!(record.action, AuditAction::Tag | AuditAction::Untag));
        Ok(page)
    }
}

/// Checks a tag name: trimmed, whitespace collapsed, not empty, bounded.
///
/// The name is otherwise left exactly as given — capitalisation included,
/// because `PESEL` and `pesel` should display the way whoever typed them
/// meant, even though they are the same tag.
fn validate_tag(name: &str) -> Result<String, ServiceError> {
    let trimmed = vesica_core::folding::collapse_whitespace(name);
    if trimmed.is_empty() {
        return Err(ServiceError::validation(
            "tag",
            "a tag needs a name",
            "send a short label, e.g. `suspect`",
        ));
    }
    if trimmed.chars().count() > MAX_TAG_LEN {
        return Err(ServiceError::validation(
            "tag",
            format!("a tag name may be at most {MAX_TAG_LEN} characters"),
            format!("shorten the tag to {MAX_TAG_LEN} characters or fewer"),
        ));
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_name_is_collapsed_bounded_and_never_empty() {
        assert_eq!(validate_tag("  key   witness ").unwrap(), "key witness");
        let e = validate_tag("   ").unwrap_err();
        assert!(e.fix().unwrap().contains("suspect"));
        assert!(validate_tag(&"x".repeat(MAX_TAG_LEN + 1)).is_err());
    }
}
