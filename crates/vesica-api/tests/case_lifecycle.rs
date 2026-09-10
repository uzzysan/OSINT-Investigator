//! The Case lifecycle, proven at the service-layer seam against a real
//! SQLite database in a temporary directory.
//!
//! Every assertion here goes through a service method, including the ones
//! about what is true *afterwards*: nothing reads a row, inspects a private
//! field or asserts on SQL. That is deliberate (Spec 0001, Testing
//! Decisions) and it is what makes this suite survive a refactor — and what
//! will let Spec 0002's HTTP handlers and MCP tools inherit it, since both
//! are adapters over the same struct.

use vesica_api::services::{
    Actor, AuditAction, CaseQuery, CaseStatus, CaseStatusFilter, CreateCase, Patch, PurgeCase,
    REDACTED_BY_PURGE, REDACTED_FIELD, ServiceConfig, ServiceError, UpdateCase,
};
use vesica_api::testing::{TestServices, test_services, test_services_with};

/// A create request with just a name — the common case.
fn named(name: &str) -> CreateCase {
    CreateCase {
        name: name.to_string(),
        ..Default::default()
    }
}

async fn fixture() -> TestServices {
    test_services().await.expect("fixture")
}

// ---------------------------------------------------------------------------
// Create, read, list, update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_created_case_comes_back_by_id_and_by_slug() {
    let fx = fixture().await;

    let created = fx
        .create_case(
            CreateCase {
                name: "Operation Lighthouse".into(),
                description: Some("Missing shipment, Gdańsk".into()),
                color: Some("#3B82F6".into()),
                default_region: Some("PL".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();

    assert_eq!(created.id.len(), 26, "ids are 26-character ULIDs");
    assert_eq!(created.slug, "operation-lighthouse");
    assert_eq!(created.name, "Operation Lighthouse");
    assert_eq!(created.status, CaseStatus::Active);
    assert_eq!(created.color.as_deref(), Some("#3b82f6"));

    assert_eq!(fx.get_case(&created.id).await.unwrap(), created);
    assert_eq!(fx.get_case_by_slug(&created.slug).await.unwrap(), created);
}

#[tokio::test]
async fn reading_something_that_is_not_there_says_so() {
    let fx = fixture().await;
    assert!(matches!(
        fx.get_case("01ARZ3NDEKTSV4RRFFQ69G5FAV").await,
        Err(ServiceError::NotFound { kind: "case", .. })
    ));
    assert!(matches!(
        fx.get_case_by_slug("no-such-case").await,
        Err(ServiceError::NotFound { .. })
    ));
}

#[tokio::test]
async fn a_case_needs_a_name_and_the_refusal_says_what_to_send() {
    let fx = fixture().await;
    let e = fx
        .create_case(named("   "), &Actor::analyst())
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "name"),
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(
        e.fix().is_some_and(|f| !f.is_empty()),
        "an agent has to be told what a valid name looks like"
    );
}

#[tokio::test]
async fn the_list_is_newest_first_and_reports_when_it_was_cut_short() {
    let fx = fixture().await;
    for name in ["First", "Second", "Third"] {
        fx.create_case(named(name), &Actor::analyst())
            .await
            .unwrap();
    }

    let all = fx.list_cases(CaseQuery::default()).await.unwrap();
    assert_eq!(all.cases.len(), 3);
    assert_eq!(all.cases[0].name, "Third", "newest first");
    assert!(!all.truncated);

    let page = fx
        .list_cases(CaseQuery {
            limit: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.cases.len(), 2);
    assert!(
        page.truncated,
        "there was a third case the page did not show"
    );

    let rest = fx
        .list_cases(CaseQuery {
            limit: Some(2),
            offset: 2,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(rest.cases.len(), 1);
    assert!(!rest.truncated);
}

#[tokio::test]
async fn a_caller_cannot_ask_for_more_rows_than_the_configuration_allows() {
    let fx = test_services_with(ServiceConfig {
        max_page_size: 2,
        ..Default::default()
    })
    .await
    .unwrap();
    for name in ["A", "B", "C"] {
        fx.create_case(named(name), &Actor::analyst())
            .await
            .unwrap();
    }

    let page = fx
        .list_cases(CaseQuery {
            limit: Some(1_000),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.limit, 2, "clamped to the configured ceiling");
    assert_eq!(page.cases.len(), 2);
    assert!(page.truncated);
}

#[tokio::test]
async fn update_changes_what_it_is_given_and_leaves_the_rest_alone() {
    let fx = fixture().await;
    let case = fx
        .create_case(
            CreateCase {
                name: "Operation Lighthouse".into(),
                description: Some("original".into()),
                color: Some("#3b82f6".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();

    let updated = fx
        .update_case(
            &case.id,
            UpdateCase {
                description: Patch::Set("revised".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();

    assert_eq!(updated.description.as_deref(), Some("revised"));
    assert_eq!(updated.name, case.name, "an untouched field is untouched");
    assert_eq!(updated.color, case.color);

    let cleared = fx
        .update_case(
            &case.id,
            UpdateCase {
                description: Patch::Clear,
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(cleared.description, None, "Clear means clear");
    assert_eq!(cleared.color, case.color, "and clears nothing else");

    assert_eq!(fx.get_case(&case.id).await.unwrap(), cleared);
}

// ---------------------------------------------------------------------------
// Slugs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn slugs_are_derived_from_the_name_and_made_unique() {
    let fx = fixture().await;

    let first = fx
        .create_case(named("Operation Lighthouse"), &Actor::analyst())
        .await
        .unwrap();
    let second = fx
        .create_case(named("Operation Lighthouse"), &Actor::analyst())
        .await
        .unwrap();
    let third = fx
        .create_case(named("operation lighthouse"), &Actor::analyst())
        .await
        .unwrap();

    assert_eq!(first.slug, "operation-lighthouse");
    assert_eq!(second.slug, "operation-lighthouse-2");
    assert_eq!(third.slug, "operation-lighthouse-3");

    // Each still resolves to its own case.
    assert_eq!(
        fx.get_case_by_slug(&second.slug).await.unwrap().id,
        second.id
    );
}

#[tokio::test]
async fn a_slug_does_not_move_when_the_case_is_renamed() {
    // The point of a slug is that a URL, a report or an agent can quote it
    // and still be right later. A rename that silently re-slugs breaks
    // every one of those at once.
    let fx = fixture().await;
    let case = fx
        .create_case(named("Operation Lighthouse"), &Actor::analyst())
        .await
        .unwrap();

    let renamed = fx
        .update_case(
            &case.id,
            UpdateCase {
                name: Some("Operation Northern Star".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();

    assert_eq!(renamed.name, "Operation Northern Star");
    assert_eq!(renamed.slug, "operation-lighthouse", "the slug is stable");
    assert_eq!(
        fx.get_case_by_slug("operation-lighthouse")
            .await
            .unwrap()
            .id,
        case.id,
        "a reference taken before the rename still resolves"
    );
    assert!(
        fx.get_case_by_slug("operation-northern-star")
            .await
            .is_err(),
        "and the new name did not quietly claim a second handle"
    );
}

#[tokio::test]
async fn a_name_with_diacritics_becomes_an_ascii_handle() {
    let fx = fixture().await;
    let case = fx
        .create_case(named("Operacja Żółć — Łódź"), &Actor::analyst())
        .await
        .unwrap();
    assert_eq!(case.slug, "operacja-zolc-lodz");
    assert_eq!(fx.get_case_by_slug(&case.slug).await.unwrap().id, case.id);
}

// ---------------------------------------------------------------------------
// Default region
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_default_region_is_stored_normalised_and_usable_for_normalisation() {
    let fx = fixture().await;
    let case = fx
        .create_case(
            CreateCase {
                name: "Polish case".into(),
                default_region: Some("Poland".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();

    assert_eq!(
        case.default_region.as_deref(),
        Some("PL"),
        "however it was spelled, it is stored as ISO-3166-1 alpha-2"
    );
    assert_eq!(
        fx.get_case(&case.id)
            .await
            .unwrap()
            .normalize_context()
            .region(),
        Some("PL"),
        "and it is retrievable as the context later normalisation needs"
    );

    let e = fx
        .create_case(
            CreateCase {
                name: "Nowhere".into(),
                default_region: Some("Atlantis".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    assert!(e.fix().is_some(), "a rejected region says what to send");
}

#[tokio::test]
async fn the_region_can_be_changed_and_cleared_later() {
    let fx = fixture().await;
    let case = fx
        .create_case(named("Case"), &Actor::analyst())
        .await
        .unwrap();
    assert_eq!(case.default_region, None);
    assert_eq!(case.normalize_context().region(), None);

    let set = fx
        .update_case(
            &case.id,
            UpdateCase {
                default_region: Patch::Set("de".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(set.default_region.as_deref(), Some("DE"));

    let cleared = fx
        .update_case(
            &case.id,
            UpdateCase {
                default_region: Patch::Clear,
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(cleared.default_region, None);
}

#[tokio::test]
async fn a_deployment_wide_default_region_applies_to_a_case_that_does_not_name_one() {
    let fx = test_services_with(ServiceConfig {
        default_region: Some("PL".into()),
        ..Default::default()
    })
    .await
    .unwrap();

    let inherited = fx
        .create_case(named("Case"), &Actor::analyst())
        .await
        .unwrap();
    assert_eq!(inherited.default_region.as_deref(), Some("PL"));

    let explicit = fx
        .create_case(
            CreateCase {
                name: "German case".into(),
                default_region: Some("DE".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(
        explicit.default_region.as_deref(),
        Some("DE"),
        "an explicit region beats the deployment default"
    );
}

// ---------------------------------------------------------------------------
// Archive
// ---------------------------------------------------------------------------

#[tokio::test]
async fn archiving_takes_a_case_out_of_the_active_list_without_destroying_it() {
    let fx = fixture().await;
    let case = fx
        .create_case(
            CreateCase {
                name: "Concluded".into(),
                description: Some("everything it knew".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();

    let archived = fx.archive_case(&case.id, &Actor::analyst()).await.unwrap();
    assert_eq!(archived.status, CaseStatus::Archived);

    let active = fx.list_cases(CaseQuery::default()).await.unwrap();
    assert!(active.cases.is_empty(), "gone from the active list");

    // Nothing was destroyed: it reads back whole, by id and by slug, and
    // appears when asked for.
    let read_back = fx.get_case(&case.id).await.unwrap();
    assert_eq!(read_back.description.as_deref(), Some("everything it knew"));
    assert_eq!(fx.get_case_by_slug(&case.slug).await.unwrap().id, case.id);

    for filter in [CaseStatusFilter::Archived, CaseStatusFilter::All] {
        let listed = fx
            .list_cases(CaseQuery {
                status: filter,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(listed.cases.len(), 1, "visible under {filter:?}");
    }

    let reopened = fx
        .unarchive_case(&case.id, &Actor::analyst())
        .await
        .unwrap();
    assert_eq!(reopened.status, CaseStatus::Active);
    assert_eq!(
        fx.list_cases(CaseQuery::default())
            .await
            .unwrap()
            .cases
            .len(),
        1
    );
}

// ---------------------------------------------------------------------------
// Soft delete and restore
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_deleted_case_is_hidden_from_every_read_and_can_be_brought_back() {
    let fx = fixture().await;
    let case = fx
        .create_case(named("Operation Lighthouse"), &Actor::analyst())
        .await
        .unwrap();

    fx.delete_case(&case.id, &Actor::analyst()).await.unwrap();

    assert!(matches!(
        fx.get_case(&case.id).await,
        Err(ServiceError::NotFound { .. })
    ));
    assert!(matches!(
        fx.get_case_by_slug(&case.slug).await,
        Err(ServiceError::NotFound { .. })
    ));
    for filter in [
        CaseStatusFilter::Active,
        CaseStatusFilter::Archived,
        CaseStatusFilter::All,
    ] {
        let listed = fx
            .list_cases(CaseQuery {
                status: filter,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(listed.cases.is_empty(), "hidden under {filter:?} too");
    }

    let restored = fx.restore_case(&case.id, &Actor::analyst()).await.unwrap();
    assert_eq!(restored.id, case.id);
    assert_eq!(
        restored.slug, case.slug,
        "and it keeps the handle people quoted"
    );
    assert_eq!(restored.name, case.name);
    assert_eq!(fx.get_case(&case.id).await.unwrap(), restored);
}

#[tokio::test]
async fn a_deleted_case_keeps_its_slug_reserved() {
    // Restore has to be able to give the slug back, so a deleted case is
    // still holding it — the next case of the same name gets the suffix.
    let fx = fixture().await;
    let first = fx
        .create_case(named("Operation Lighthouse"), &Actor::analyst())
        .await
        .unwrap();
    fx.delete_case(&first.id, &Actor::analyst()).await.unwrap();

    let second = fx
        .create_case(named("Operation Lighthouse"), &Actor::analyst())
        .await
        .unwrap();
    assert_eq!(second.slug, "operation-lighthouse-2");

    let restored = fx.restore_case(&first.id, &Actor::analyst()).await.unwrap();
    assert_eq!(restored.slug, "operation-lighthouse");
}

#[tokio::test]
async fn deleting_and_restoring_are_idempotent_and_a_missing_case_is_still_missing() {
    let fx = fixture().await;
    let case = fx
        .create_case(named("Case"), &Actor::analyst())
        .await
        .unwrap();

    fx.delete_case(&case.id, &Actor::analyst()).await.unwrap();
    fx.delete_case(&case.id, &Actor::analyst())
        .await
        .expect("deleting an already-deleted case is not an error");
    fx.restore_case(&case.id, &Actor::analyst()).await.unwrap();
    fx.restore_case(&case.id, &Actor::analyst())
        .await
        .expect("restoring a live case is not an error");

    assert!(matches!(
        fx.delete_case("01ARZ3NDEKTSV4RRFFQ69G5FAV", &Actor::analyst())
            .await,
        Err(ServiceError::NotFound { .. })
    ));

    // The no-ops recorded nothing: one create, one delete, one restore.
    let trail = fx.case_audit(&case.id, None).await.unwrap();
    let actions: Vec<_> = trail.entries.iter().map(|e| e.action).collect();
    assert_eq!(
        actions,
        vec![
            AuditAction::Restore,
            AuditAction::Delete,
            AuditAction::Create
        ]
    );
}

// ---------------------------------------------------------------------------
// Purge
// ---------------------------------------------------------------------------

#[tokio::test]
async fn purging_without_confirmation_is_refused_and_changes_nothing() {
    let fx = fixture().await;
    let case = fx
        .create_case(named("Case"), &Actor::analyst())
        .await
        .unwrap();

    let e = fx
        .purge_case(
            PurgeCase {
                case_id: case.id.clone(),
                confirm: false,
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();

    assert!(matches!(e, ServiceError::ConfirmationRequired { .. }));
    assert!(
        e.fix().is_some_and(|f| f.contains("confirm")),
        "the refusal says how to confirm"
    );
    assert_eq!(
        fx.get_case(&case.id).await.unwrap(),
        case,
        "and the case is untouched"
    );
    let trail = fx.case_audit(&case.id, None).await.unwrap();
    assert_eq!(trail.entries.len(), 1, "a refusal is not a mutation");
}

#[tokio::test]
async fn a_confirmed_purge_actually_destroys_the_case_and_frees_its_slug() {
    let fx = fixture().await;
    let case = fx
        .create_case(named("Operation Lighthouse"), &Actor::analyst())
        .await
        .unwrap();

    fx.purge_case(
        PurgeCase {
            case_id: case.id.clone(),
            confirm: true,
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    assert!(matches!(
        fx.get_case(&case.id).await,
        Err(ServiceError::NotFound { .. })
    ));
    assert!(
        fx.restore_case(&case.id, &Actor::analyst()).await.is_err(),
        "a purge is not reversible; there is nothing left to restore"
    );

    // The slug is released, so the name is available again — and the new
    // case gets the original handle, not a suffixed one.
    let replacement = fx
        .create_case(named("Operation Lighthouse"), &Actor::analyst())
        .await
        .unwrap();
    assert_eq!(replacement.slug, "operation-lighthouse");
    assert_ne!(replacement.id, case.id);

    // The trail records that a purge happened, and carries no snapshot of
    // what was destroyed.
    let trail = fx.case_audit(&case.id, None).await.unwrap();
    let purge = trail
        .entries
        .iter()
        .find(|e| e.action == AuditAction::Purge)
        .expect("the purge is recorded");
    assert_eq!(purge.actor, "human");
    assert!(purge.before.is_none() && purge.after.is_none());
}

#[tokio::test]
async fn a_purge_erases_the_payloads_in_the_trail_but_not_the_trail() {
    // 09-security-and-legal.md: after a purge the audit log retains "only
    // the fact that a deletion occurred". So the entries stay — a purge
    // must not be able to erase the evidence that it happened — and what
    // goes is the copy of the purged data inside them.
    let fx = fixture().await;
    let case = fx
        .create_case(
            CreateCase {
                name: "Operation Lighthouse".into(),
                description: Some("Jan Kowalski, Gdańsk".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    fx.update_case(
        &case.id,
        UpdateCase {
            name: Some("Operation Northern Star".into()),
            ..Default::default()
        },
        &Actor::named("agent:researcher").unwrap(),
    )
    .await
    .unwrap();

    let before_purge = fx.case_audit(&case.id, None).await.unwrap();
    assert!(
        before_purge
            .entries
            .iter()
            .any(|e| !e.payload_was_redacted() && e.before.is_some()),
        "precondition: the trail holds the case's data"
    );

    fx.purge_case(
        PurgeCase {
            case_id: case.id.clone(),
            confirm: true,
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    let after_purge = fx.case_audit(&case.id, None).await.unwrap();
    assert_eq!(
        after_purge.entries.len(),
        before_purge.entries.len() + 1,
        "every entry survives, plus the purge's own"
    );

    for entry in &after_purge.entries {
        for payload in [entry.before.as_ref(), entry.after.as_ref()]
            .into_iter()
            .flatten()
        {
            assert_eq!(
                payload[REDACTED_FIELD],
                serde_json::json!(REDACTED_BY_PURGE),
                "no payload may still hold the purged case's data: {payload}"
            );
            assert!(
                payload.get("at").is_some(),
                "a marker says when the erasure happened, so a reader can \
                 tell it from a payload that was never recorded"
            );
            assert!(
                !payload.to_string().contains("Kowalski"),
                "and it really is gone"
            );
        }
    }

    // What happened, when, and at whose hand is all still on the record.
    let creation = &before_purge.entries[before_purge.entries.len() - 1];
    let creation_after = after_purge
        .entries
        .iter()
        .find(|e| e.id == creation.id)
        .expect("the creation entry is still there");
    assert_eq!(creation_after.action, AuditAction::Create);
    assert_eq!(creation_after.actor, creation.actor);
    assert_eq!(creation_after.at, creation.at);
    assert_eq!(creation_after.target_id, case.id);
    assert!(creation_after.payload_was_redacted());

    let update = after_purge
        .entries
        .iter()
        .find(|e| e.action == AuditAction::Update)
        .unwrap();
    assert_eq!(
        update.actor, "agent:researcher",
        "who did what is exactly what the trail is for"
    );

    let purge = after_purge
        .entries
        .iter()
        .find(|e| e.action == AuditAction::Purge)
        .unwrap();
    assert!(
        purge.before.is_none() && purge.after.is_none(),
        "the purge's own entry carries no snapshot and needs no marker"
    );
    assert!(!purge.payload_was_redacted());
}

#[tokio::test]
async fn a_purge_leaves_another_cases_trail_alone() {
    let fx = fixture().await;
    let doomed = fx
        .create_case(named("Doomed"), &Actor::analyst())
        .await
        .unwrap();
    let bystander = fx
        .create_case(
            CreateCase {
                name: "Bystander".into(),
                description: Some("nothing to do with it".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();

    let before = fx.case_audit(&bystander.id, None).await.unwrap();

    fx.purge_case(
        PurgeCase {
            case_id: doomed.id.clone(),
            confirm: true,
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    let after = fx.case_audit(&bystander.id, None).await.unwrap();
    assert_eq!(after, before, "an unrelated case's trail is untouched");
    assert!(!after.entries[0].payload_was_redacted());
    assert_eq!(
        after.entries[0].after.as_ref().unwrap()["name"],
        serde_json::json!("Bystander")
    );
    assert_eq!(fx.get_case(&bystander.id).await.unwrap(), bystander);
}

#[tokio::test]
async fn a_purge_that_is_refused_redacts_nothing() {
    let fx = fixture().await;
    let case = fx
        .create_case(named("Operation Lighthouse"), &Actor::analyst())
        .await
        .unwrap();
    let before = fx.case_audit(&case.id, None).await.unwrap();

    assert!(
        fx.purge_case(
            PurgeCase {
                case_id: case.id.clone(),
                confirm: false,
            },
            &Actor::analyst(),
        )
        .await
        .is_err()
    );

    let after = fx.case_audit(&case.id, None).await.unwrap();
    assert_eq!(after, before, "nothing was erased and nothing was recorded");
    assert!(!after.entries[0].payload_was_redacted());
}

#[tokio::test]
async fn purging_a_case_that_is_not_there_says_so() {
    let fx = fixture().await;
    assert!(matches!(
        fx.purge_case(
            PurgeCase {
                case_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                confirm: true,
            },
            &Actor::analyst(),
        )
        .await,
        Err(ServiceError::NotFound { .. })
    ));
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_mutation_is_recorded_with_what_changed() {
    let fx = fixture().await;
    let case = fx
        .create_case(named("Operation Lighthouse"), &Actor::analyst())
        .await
        .unwrap();
    fx.update_case(
        &case.id,
        UpdateCase {
            name: Some("Operation Northern Star".into()),
            ..Default::default()
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();
    fx.archive_case(&case.id, &Actor::analyst()).await.unwrap();
    fx.unarchive_case(&case.id, &Actor::analyst())
        .await
        .unwrap();
    fx.delete_case(&case.id, &Actor::analyst()).await.unwrap();
    fx.restore_case(&case.id, &Actor::analyst()).await.unwrap();

    let trail = fx.case_audit(&case.id, None).await.unwrap();
    let actions: Vec<_> = trail.entries.iter().map(|e| e.action).collect();
    assert_eq!(
        actions,
        vec![
            AuditAction::Restore,
            AuditAction::Delete,
            AuditAction::Unarchive,
            AuditAction::Archive,
            AuditAction::Update,
            AuditAction::Create,
        ],
        "newest first, one entry per mutation and none for anything else"
    );

    for entry in &trail.entries {
        assert_eq!(entry.target_kind, "case");
        assert_eq!(entry.target_id, case.id);
        assert_eq!(entry.case_id.as_deref(), Some(case.id.as_str()));
        assert!(!entry.at.is_empty());
    }

    let update = trail
        .entries
        .iter()
        .find(|e| e.action == AuditAction::Update)
        .unwrap();
    assert_eq!(
        update.before.as_ref().unwrap()["name"],
        serde_json::json!("Operation Lighthouse"),
        "the trail says what it was"
    );
    assert_eq!(
        update.after.as_ref().unwrap()["name"],
        serde_json::json!("Operation Northern Star"),
        "and what it became"
    );
}

#[tokio::test]
async fn the_actor_defaults_to_the_analyst_and_a_caller_can_override_it() {
    let fx = fixture().await;

    let by_analyst = fx
        .create_case(named("By the analyst"), &Actor::default())
        .await
        .unwrap();
    let agent = Actor::named("agent:lighthouse-researcher").unwrap();
    let by_agent = fx.create_case(named("By an agent"), &agent).await.unwrap();
    fx.archive_case(&by_analyst.id, &agent).await.unwrap();

    let analyst_trail = fx.case_audit(&by_analyst.id, None).await.unwrap();
    assert_eq!(
        analyst_trail
            .entries
            .iter()
            .map(|e| (e.action, e.actor.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (AuditAction::Archive, "agent:lighthouse-researcher"),
            (AuditAction::Create, "human"),
        ],
        "each entry records who actually did that one thing"
    );

    let agent_trail = fx.case_audit(&by_agent.id, None).await.unwrap();
    assert_eq!(agent_trail.entries[0].actor, "agent:lighthouse-researcher");
}

#[tokio::test]
async fn the_audit_trail_is_bounded_and_says_when_it_was_cut_short() {
    let fx = fixture().await;
    let case = fx
        .create_case(named("Case"), &Actor::analyst())
        .await
        .unwrap();
    for i in 0..4 {
        fx.update_case(
            &case.id,
            UpdateCase {
                description: Patch::Set(format!("revision {i}")),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    }

    let page = fx.case_audit(&case.id, Some(2)).await.unwrap();
    assert_eq!(page.entries.len(), 2);
    assert!(page.truncated);

    let whole = fx.case_audit(&case.id, None).await.unwrap();
    assert_eq!(whole.entries.len(), 5);
    assert!(!whole.truncated);
}

// ---------------------------------------------------------------------------
// Atomicity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_write_that_fails_takes_its_audit_entry_down_with_it() {
    // The audit entry is written *before* the change inside the same
    // transaction, so this test can only pass if the two really do share
    // one: were they separate transactions, the entry below would have
    // committed before the insert failed and would still be here.
    let fx = fixture().await;
    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    let created = fx
        .create_case(
            CreateCase {
                id: Some(id.into()),
                name: "Operation Lighthouse".into(),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(created.id, id);

    let e = fx
        .create_case(
            CreateCase {
                id: Some(id.into()),
                name: "A different case entirely".into(),
                ..Default::default()
            },
            &Actor::named("agent:clumsy").unwrap(),
        )
        .await
        .unwrap_err();
    assert!(matches!(e, ServiceError::Conflict { .. }));
    assert!(e.fix().is_some());

    let trail = fx.case_audit(id, None).await.unwrap();
    assert_eq!(
        trail.entries.len(),
        1,
        "the failed create must leave no audit entry behind: {trail:?}"
    );
    assert_eq!(
        trail.entries[0].actor, "human",
        "and not the failed actor's"
    );

    // The store is otherwise exactly as it was.
    assert_eq!(fx.get_case(id).await.unwrap(), created);
    assert_eq!(
        fx.list_cases(CaseQuery::default())
            .await
            .unwrap()
            .cases
            .len(),
        1
    );
    assert!(
        fx.get_case_by_slug("a-different-case-entirely")
            .await
            .is_err(),
        "and the slug the failed create was allocating was not consumed"
    );
}

#[tokio::test]
async fn an_id_that_is_not_a_ulid_is_refused_before_anything_is_written() {
    let fx = fixture().await;
    let e = fx
        .create_case(
            CreateCase {
                id: Some("case-1".into()),
                name: "Case".into(),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    assert!(matches!(e, ServiceError::Validation { .. }));
    assert!(e.fix().unwrap().contains("ULID"));
    assert!(
        fx.case_audit("case-1", None)
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(
        fx.list_cases(CaseQuery::default())
            .await
            .unwrap()
            .cases
            .is_empty()
    );
}
