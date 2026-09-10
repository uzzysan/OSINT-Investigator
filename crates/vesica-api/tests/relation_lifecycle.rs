//! Connecting two findings, and everything that happens to the edge
//! afterwards, proven at the service-layer seam against a real SQLite
//! database in a temporary directory.
//!
//! Every assertion goes through a service method, including the ones about
//! what is true afterwards — there is no reaching into the store to check a
//! column, because a rule the service layer does not enforce is not a rule.

use vesica_api::services::{
    Actor, AuditAction, Confidence, CreateEntity, CreateRelation, EditProperties, Endpoint,
    EndpointRole, Patch, RelationQuery, RelationWarningCode, ServiceError, SetInterval,
    UpdateRelation,
};
use vesica_api::testing::{
    SmallGraph, TestServices, property_values, seed_case, seed_entity, seed_person, seed_relation,
    seed_small_graph, test_services,
};
use vesica_core::DatePrecision;

async fn fixture() -> TestServices {
    test_services().await.expect("fixture")
}

async fn graph(fx: &TestServices) -> SmallGraph {
    seed_small_graph(fx).await.expect("a small graph")
}

fn ids(list: &[vesica_api::services::RelationSummary]) -> Vec<&str> {
    list.iter().map(|r| r.id.as_str()).collect()
}

fn in_case(case_id: &str) -> RelationQuery {
    RelationQuery {
        case_id: case_id.to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Create and read
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_typed_directed_relation_connects_two_entities_in_one_case() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    let created = &g.directorship;
    assert_eq!(created.id.len(), 26, "ids are 26-character ULIDs");
    assert_eq!(created.case_id, g.case.id);
    assert_eq!(created.schema_id, "Directorship");
    assert_eq!(created.source_id, g.jan.id);
    assert_eq!(created.target_id, g.acme.id);
    assert!(created.directed, "a directorship reads one way");
    assert_eq!(created.confidence, Confidence::Probable, "the default");
    assert_eq!(created.created_by, "human");
    assert!(created.endpoint_warnings.is_empty());

    assert_eq!(&fx.get_relation(&created.id).await.unwrap(), created);
}

#[tokio::test]
async fn the_caption_is_the_schema_label_until_somebody_names_the_edge() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    assert_eq!(g.directorship.caption, "Directorship");

    let named = fx
        .update_relation(
            &g.directorship.id,
            UpdateRelation {
                caption: Patch::Set("chair, on paper only".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(named.caption, "chair, on paper only");

    let back = fx
        .update_relation(
            &g.directorship.id,
            UpdateRelation {
                caption: Patch::Clear,
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(back.caption, "Directorship", "clearing re-derives it");
}

#[tokio::test]
async fn an_unknown_relation_schema_is_refused_with_the_catalogue_the_registry_has() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    let e = fx
        .create_relation(
            CreateRelation {
                case_id: g.case.id.clone(),
                schema_id: "Shareholding".into(),
                source_id: g.jan.id.clone(),
                target_id: g.acme.id.clone(),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "schema_id"),
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(e.fix().unwrap().contains("Ownership"));
}

#[tokio::test]
async fn a_relation_to_an_entity_that_is_not_there_names_the_end_that_is_wrong() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    let e = fx
        .create_relation(
            CreateRelation {
                case_id: g.case.id.clone(),
                schema_id: "Directorship".into(),
                source_id: g.jan.id.clone(),
                target_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "target_id"),
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(e.fix().is_some());
}

// ---------------------------------------------------------------------------
// Properties on the edge
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_relation_carries_its_own_properties_normalised_like_an_entitys() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    let ownership = seed_relation(
        &fx,
        "Ownership",
        &g.jan,
        &g.acme,
        &[("percentage", &[" 51.5 "]), ("role", &["beneficial owner"])],
    )
    .await
    .unwrap();

    let percentage = ownership
        .properties
        .iter()
        .find(|p| p.prop == "percentage")
        .expect("the shareholding records its percentage");
    assert_eq!(
        percentage.value, " 51.5 ",
        "the raw value is kept exactly as entered"
    );
    assert_eq!(percentage.value_norm.as_deref(), Some("51.5"));
    assert_eq!(percentage.numeric, Some(51.5));
    assert_eq!(percentage.seq, 0);

    // Schema-declared order, not the order the caller happened to send.
    let order: Vec<&str> = ownership
        .properties
        .iter()
        .map(|p| p.prop.as_str())
        .collect();
    assert_eq!(order, vec!["percentage", "role"]);

    assert_eq!(fx.get_relation(&ownership.id).await.unwrap(), ownership);
}

#[tokio::test]
async fn a_property_the_relation_schema_does_not_declare_is_refused_with_the_ones_it_does() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    let e = fx
        .create_relation(
            CreateRelation {
                case_id: g.case.id.clone(),
                schema_id: "Ownership".into(),
                source_id: g.jan.id.clone(),
                target_id: g.acme.id.clone(),
                properties: property_values(&[("shares", &["100"])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, message, .. } => {
            assert_eq!(field, "shares");
            assert!(message.contains("Ownership"));
        }
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(e.fix().unwrap().contains("sharesCount"));
}

#[tokio::test]
async fn editing_a_relations_properties_adds_removes_and_replaces_in_one_act() {
    let fx = fixture().await;
    let g = graph(&fx).await;
    let ownership = seed_relation(&fx, "Ownership", &g.jan, &g.acme, &[("role", &["owner"])])
        .await
        .unwrap();

    let edited = fx
        .edit_relation_properties(
            &ownership.id,
            EditProperties {
                replace: property_values(&[("role", &["beneficial owner"])]),
                add: property_values(&[("percentage", &["51"])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();

    assert_eq!(
        edited
            .properties
            .iter()
            .filter(|p| p.prop == "role")
            .count(),
        1,
        "replace clears what was there"
    );
    assert_eq!(
        edited
            .properties
            .iter()
            .find(|p| p.prop == "percentage")
            .and_then(|p| p.numeric),
        Some(51.0)
    );

    let removed = fx
        .edit_relation_properties(
            &ownership.id,
            EditProperties {
                remove: property_values(&[("percentage", &["51"])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert!(removed.properties.iter().all(|p| p.prop != "percentage"));

    // An edit that asks for nothing is not an error and records nothing.
    let untouched = fx
        .edit_relation_properties(&ownership.id, EditProperties::default(), &Actor::analyst())
        .await
        .unwrap();
    assert_eq!(untouched, removed);
}

// ---------------------------------------------------------------------------
// Intervals
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_validity_interval_keeps_the_precision_it_was_written_to() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    let interval = g
        .directorship
        .interval
        .as_ref()
        .expect("the directorship is dated");
    assert_eq!(
        interval.precision,
        DatePrecision::Year,
        "the coarsest bound governs: `2021` is only known to the year"
    );
    assert_eq!(
        interval.start.as_deref(),
        Some("2015-03-01T00:00:00Z"),
        "`2015-03` starts at the first instant of March"
    );
    assert_eq!(
        interval.end.as_deref(),
        Some("2021-12-31T23:59:59.999999999Z"),
        "`2021` is the whole of 2021 and is never narrowed to 1 January"
    );

    // And it survives the round trip, which is what a timeline will read.
    assert_eq!(
        fx.get_relation(&g.directorship.id)
            .await
            .unwrap()
            .interval
            .as_ref(),
        Some(interval)
    );
}

#[tokio::test]
async fn a_relation_known_only_to_have_existed_in_one_year_says_exactly_that() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    let seen = fx
        .create_relation(
            CreateRelation {
                case_id: g.case.id.clone(),
                schema_id: "Family".into(),
                source_id: g.jan.id.clone(),
                target_id: g.maria.id.clone(),
                interval: Some(SetInterval {
                    start: Some("2019".into()),
                    end: Some("2019".into()),
                    precision: None,
                }),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();

    let interval = seen.interval.unwrap();
    assert_eq!(interval.precision, DatePrecision::Year);
    assert_eq!(interval.start.as_deref(), Some("2019-01-01T00:00:00Z"));
    assert_eq!(
        interval.end.as_deref(),
        Some("2019-12-31T23:59:59.999999999Z")
    );
}

#[tokio::test]
async fn an_interval_can_be_set_changed_and_cleared_and_an_open_end_means_still_current() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    let still_running = fx
        .update_relation(
            &g.directorship.id,
            UpdateRelation {
                interval: Patch::Set(SetInterval {
                    start: Some("2015-03-14".into()),
                    end: None,
                    precision: None,
                }),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    let interval = still_running.interval.unwrap();
    assert_eq!(interval.precision, DatePrecision::Day);
    assert!(
        interval.end.is_none(),
        "no end means it has not ended, as far as is known"
    );

    let undated = fx
        .update_relation(
            &g.directorship.id,
            UpdateRelation {
                interval: Patch::Clear,
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(undated.interval, None);
}

#[tokio::test]
async fn an_interval_on_a_schema_that_does_not_carry_one_is_refused_with_a_fix() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    let e = fx
        .create_relation(
            CreateRelation {
                case_id: g.case.id.clone(),
                schema_id: "Associate".into(),
                source_id: g.jan.id.clone(),
                target_id: g.maria.id.clone(),
                interval: Some(SetInterval {
                    start: Some("2019".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "interval"),
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(e.fix().is_some());
}

// ---------------------------------------------------------------------------
// Endpoint constraints guide, they do not forbid
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_endpoint_the_schema_did_not_expect_is_recorded_with_a_warning_not_refused() {
    let fx = fixture().await;
    let g = graph(&fx).await;
    let holding = seed_entity(
        &fx,
        &g.case.id,
        "Company",
        &[("name", &["Holding sp. z o.o."])],
    )
    .await
    .unwrap();

    // A directorship expects a Person at the source end. A company that
    // sits on another company's board is a real thing the ontology did not
    // anticipate, so it is recorded and flagged.
    let odd = seed_relation(&fx, "Directorship", &holding, &g.acme, &[])
        .await
        .expect("an unexpected endpoint must not be refused");

    assert_eq!(odd.endpoint_warnings.len(), 1);
    let warning = &odd.endpoint_warnings[0];
    assert_eq!(warning.endpoint, Endpoint::Source);
    assert_eq!(warning.code, RelationWarningCode::EndpointSchemaUnexpected);
    assert_eq!(warning.entity_id, holding.id);
    assert_eq!(warning.schema_id, "Company");
    assert!(warning.expected.contains(&"Person".to_string()));

    // The warning is not a one-off on the create response: it comes back on
    // every read, because it is derived from what is stored rather than
    // frozen into a column.
    assert_eq!(
        fx.get_relation(&odd.id).await.unwrap().endpoint_warnings,
        odd.endpoint_warnings
    );

    // An endpoint the schema does expect — via an ancestor — is silent.
    assert!(
        seed_relation(&fx, "Ownership", &g.jan, &g.acme, &[])
            .await
            .unwrap()
            .endpoint_warnings
            .is_empty(),
        "`Person` inherits `LegalEntity`, which the owner end lists"
    );
}

// ---------------------------------------------------------------------------
// Undirected relations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_undirected_relation_is_distinguishable_and_answers_from_either_end() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    assert!(!g.family.directed, "a family tie asserts no direction");
    assert!(g.directorship.directed);

    let symmetric = fx
        .list_relations(RelationQuery {
            directed: Some(false),
            ..in_case(&g.case.id)
        })
        .await
        .unwrap();
    assert_eq!(ids(&symmetric.relations), vec![g.family.id.as_str()]);

    // Maria is stored at the target end of the family tie. Asking for the
    // relations where she is the *source* still finds it, because the edge
    // makes no claim about which end she is.
    let as_source = fx
        .list_relations(RelationQuery {
            endpoint: Some(g.maria.id.clone()),
            role: EndpointRole::Source,
            ..in_case(&g.case.id)
        })
        .await
        .unwrap();
    assert_eq!(ids(&as_source.relations), vec![g.family.id.as_str()]);

    // The directed edge does not answer to a role it does not have.
    let acme_as_source = fx
        .list_relations(RelationQuery {
            endpoint: Some(g.acme.id.clone()),
            role: EndpointRole::Source,
            ..in_case(&g.case.id)
        })
        .await
        .unwrap();
    assert!(
        acme_as_source.relations.is_empty(),
        "Acme is the target of the directorship, not its source"
    );
    let acme_either = fx
        .list_relations(RelationQuery {
            endpoint: Some(g.acme.id.clone()),
            role: EndpointRole::Either,
            ..in_case(&g.case.id)
        })
        .await
        .unwrap();
    assert_eq!(
        ids(&acme_either.relations),
        vec![g.directorship.id.as_str()]
    );
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn relations_list_by_case_by_schema_and_by_endpoint_and_say_when_they_were_cut_short() {
    let fx = fixture().await;
    let g = graph(&fx).await;
    let other = seed_case(&fx, "Operation Beacon", None).await.unwrap();
    let stranger = seed_person(&fx, &other.id, &[("name", &["Somebody Else"])])
        .await
        .unwrap();
    let friend = seed_person(&fx, &other.id, &[("name", &["Another"])])
        .await
        .unwrap();
    seed_relation(&fx, "Associate", &stranger, &friend, &[])
        .await
        .unwrap();

    let all = fx.list_relations(in_case(&g.case.id)).await.unwrap();
    assert_eq!(all.relations.len(), 2, "another case's edges are not ours");
    assert!(!all.truncated);

    let by_schema = fx
        .list_relations(RelationQuery {
            schema_id: Some("Directorship".into()),
            ..in_case(&g.case.id)
        })
        .await
        .unwrap();
    assert_eq!(ids(&by_schema.relations), vec![g.directorship.id.as_str()]);

    let jans = fx
        .list_relations(RelationQuery {
            endpoint: Some(g.jan.id.clone()),
            ..in_case(&g.case.id)
        })
        .await
        .unwrap();
    assert_eq!(jans.relations.len(), 2, "Jan is on both edges");

    let page = fx
        .list_relations(RelationQuery {
            limit: Some(1),
            ..in_case(&g.case.id)
        })
        .await
        .unwrap();
    assert_eq!(page.relations.len(), 1);
    assert!(page.truncated, "a bounded list says it was cut short");
    assert_eq!(page.limit, 1);

    let second = fx
        .list_relations(RelationQuery {
            limit: Some(1),
            offset: 1,
            ..in_case(&g.case.id)
        })
        .await
        .unwrap();
    assert!(!second.truncated);
    assert_ne!(ids(&second.relations), ids(&page.relations));
}

// ---------------------------------------------------------------------------
// Update, delete, restore
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_deleted_relation_disappears_from_every_read_and_comes_back_whole() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    let confident = fx
        .update_relation(
            &g.directorship.id,
            UpdateRelation {
                confidence: Some(Confidence::Confirmed),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(confident.confidence, Confidence::Confirmed);

    fx.delete_relation(&g.directorship.id, &Actor::analyst())
        .await
        .unwrap();

    assert!(matches!(
        fx.get_relation(&g.directorship.id).await,
        Err(ServiceError::NotFound { .. })
    ));
    let left = fx.list_relations(in_case(&g.case.id)).await.unwrap();
    assert_eq!(ids(&left.relations), vec![g.family.id.as_str()]);

    // Deleting it again is not an error and changes nothing.
    fx.delete_relation(&g.directorship.id, &Actor::analyst())
        .await
        .unwrap();

    let restored = fx
        .restore_relation(&g.directorship.id, &Actor::analyst())
        .await
        .unwrap();
    assert_eq!(restored.confidence, Confidence::Confirmed);
    assert_eq!(restored.interval, g.directorship.interval);
    assert_eq!(
        restored
            .properties
            .iter()
            .map(|p| p.value.as_str())
            .collect::<Vec<_>>(),
        vec!["chair"],
        "its properties came back with it"
    );

    assert!(matches!(
        fx.delete_relation("01ARZ3NDEKTSV4RRFFQ69G5FAV", &Actor::analyst())
            .await,
        Err(ServiceError::NotFound { .. })
    ));
}

#[tokio::test]
async fn a_deleted_relation_cannot_be_edited_or_updated() {
    let fx = fixture().await;
    let g = graph(&fx).await;
    fx.delete_relation(&g.family.id, &Actor::analyst())
        .await
        .unwrap();

    assert!(matches!(
        fx.update_relation(&g.family.id, UpdateRelation::default(), &Actor::analyst())
            .await,
        Err(ServiceError::NotFound { .. })
    ));
    assert!(matches!(
        fx.edit_relation_properties(
            &g.family.id,
            EditProperties {
                add: property_values(&[("relationType", &["cousin"])]),
                ..Default::default()
            },
            &Actor::analyst()
        )
        .await,
        Err(ServiceError::NotFound { .. })
    ));
}

// ---------------------------------------------------------------------------
// The cascade
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_an_entity_takes_its_relations_and_restoring_it_brings_them_back() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    fx.delete_entity(&g.jan.id, &Actor::analyst())
        .await
        .unwrap();

    assert!(
        fx.list_relations(in_case(&g.case.id))
            .await
            .unwrap()
            .relations
            .is_empty(),
        "the graph never contains an edge to a node that is not there"
    );
    assert!(matches!(
        fx.get_relation(&g.directorship.id).await,
        Err(ServiceError::NotFound { .. })
    ));

    fx.restore_entity(&g.jan.id, &Actor::analyst())
        .await
        .unwrap();

    let back = fx.list_relations(in_case(&g.case.id)).await.unwrap();
    assert_eq!(back.relations.len(), 2, "both edges came back");
    let directorship = fx.get_relation(&g.directorship.id).await.unwrap();
    assert_eq!(directorship.interval, g.directorship.interval);
    assert_eq!(directorship.properties, g.directorship.properties);
}

#[tokio::test]
async fn restoring_an_entity_leaves_alone_a_relation_that_was_deleted_before_it() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    // The analyst decided this tie was wrong, on its own terms.
    fx.delete_relation(&g.family.id, &Actor::analyst())
        .await
        .unwrap();
    // Later, and for unrelated reasons, Jan goes.
    fx.delete_entity(&g.jan.id, &Actor::analyst())
        .await
        .unwrap();
    fx.restore_entity(&g.jan.id, &Actor::analyst())
        .await
        .unwrap();

    let back = fx.list_relations(in_case(&g.case.id)).await.unwrap();
    assert_eq!(
        ids(&back.relations),
        vec![g.directorship.id.as_str()],
        "only the edges this deletion took down come back"
    );
    assert!(matches!(
        fx.get_relation(&g.family.id).await,
        Err(ServiceError::NotFound { .. })
    ));

    // It is still restorable deliberately, now that both its ends are back.
    fx.restore_relation(&g.family.id, &Actor::analyst())
        .await
        .unwrap();
    assert_eq!(
        fx.list_relations(in_case(&g.case.id))
            .await
            .unwrap()
            .relations
            .len(),
        2
    );
}

#[tokio::test]
async fn a_relation_cannot_be_restored_while_one_of_its_ends_is_deleted() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    fx.delete_entity(&g.acme.id, &Actor::analyst())
        .await
        .unwrap();
    let e = fx
        .restore_relation(&g.directorship.id, &Actor::analyst())
        .await
        .unwrap_err();
    match &e {
        ServiceError::Conflict { .. } => {}
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert!(e.fix().unwrap().contains(&g.acme.id));
}

#[tokio::test]
async fn a_relation_cannot_be_created_onto_a_deleted_entity() {
    let fx = fixture().await;
    let g = graph(&fx).await;
    fx.delete_entity(&g.maria.id, &Actor::analyst())
        .await
        .unwrap();

    let e = seed_relation(&fx, "Family", &g.jan, &g.maria, &[])
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "target_id"),
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[tokio::test]
async fn an_entity_may_be_related_to_itself_and_twice_to_the_same_other_entity() {
    // Neither is forbidden anywhere in the spec, and both occur: a company
    // that owns its own shares, or a director appointed, removed and
    // appointed again. Two directorships over different intervals are two
    // facts, not a duplicate.
    let fx = fixture().await;
    let g = graph(&fx).await;

    let itself = seed_relation(
        &fx,
        "Ownership",
        &g.acme,
        &g.acme,
        &[("percentage", &["5"])],
    )
    .await
    .unwrap();
    assert_eq!(itself.source_id, itself.target_id);

    let second = seed_relation(
        &fx,
        "Directorship",
        &g.jan,
        &g.acme,
        &[("role", &["chair"])],
    )
    .await
    .unwrap();
    assert_ne!(second.id, g.directorship.id);
    assert_eq!(
        fx.list_relations(RelationQuery {
            schema_id: Some("Directorship".into()),
            ..in_case(&g.case.id)
        })
        .await
        .unwrap()
        .relations
        .len(),
        2
    );
}

// ---------------------------------------------------------------------------
// One case per relation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_relation_across_two_cases_is_refused_with_an_explanation() {
    let fx = fixture().await;
    let g = graph(&fx).await;
    let other = seed_case(&fx, "Operation Beacon", None).await.unwrap();
    let stranger = seed_person(&fx, &other.id, &[("name", &["Somebody Else"])])
        .await
        .unwrap();

    let e = fx
        .create_relation(
            CreateRelation {
                case_id: g.case.id.clone(),
                schema_id: "Family".into(),
                source_id: g.jan.id.clone(),
                target_id: stranger.id.clone(),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, message, .. } => {
            assert_eq!(field, "target_id");
            assert!(message.contains(&other.id), "the message names both cases");
            assert!(message.contains(&g.case.id));
        }
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(e.fix().unwrap().contains("case"));

    // And the same refusal from the other end.
    let e = fx
        .create_relation(
            CreateRelation {
                case_id: other.id.clone(),
                schema_id: "Family".into(),
                source_id: g.jan.id.clone(),
                target_id: stranger.id.clone(),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "source_id"),
        other => panic!("expected a validation error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_mutation_is_audited_with_who_did_it() {
    let fx = fixture().await;
    let g = graph(&fx).await;
    let agent = Actor::named("agent:researcher").unwrap();

    fx.update_relation(
        &g.directorship.id,
        UpdateRelation {
            confidence: Some(Confidence::Confirmed),
            ..Default::default()
        },
        &agent,
    )
    .await
    .unwrap();
    fx.delete_relation(&g.directorship.id, &agent)
        .await
        .unwrap();
    fx.restore_relation(&g.directorship.id, &Actor::analyst())
        .await
        .unwrap();

    let trail = fx.relation_audit(&g.directorship.id, None).await.unwrap();
    let actions: Vec<AuditAction> = trail.entries.iter().map(|e| e.action).collect();
    assert_eq!(
        actions,
        vec![
            AuditAction::Restore,
            AuditAction::Delete,
            AuditAction::Update,
            AuditAction::Create,
        ],
        "newest first"
    );
    assert!(!trail.truncated);
    assert!(
        trail
            .entries
            .iter()
            .all(|e| e.target_kind == "relation" && e.case_id.as_deref() == Some(&g.case.id))
    );
    assert_eq!(trail.entries[1].actor, "agent:researcher");
    assert_eq!(trail.entries[0].actor, "human");

    let update = &trail.entries[2];
    assert!(
        update.before.is_some() && update.after.is_some(),
        "an update records both sides"
    );
}

#[tokio::test]
async fn a_refused_relation_leaves_nothing_behind_not_even_an_audit_entry() {
    let fx = fixture().await;
    let g = graph(&fx).await;
    let id = vesica_store::new_id();

    let e = fx
        .create_relation(
            CreateRelation {
                id: Some(id.clone()),
                case_id: g.case.id.clone(),
                schema_id: "Family".into(),
                source_id: g.jan.id.clone(),
                target_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await;
    assert!(e.is_err());

    assert!(
        fx.relation_audit(&id, None)
            .await
            .unwrap()
            .entries
            .is_empty(),
        "the audit entry and the change share one transaction, so both go"
    );
    assert!(matches!(
        fx.get_relation(&id).await,
        Err(ServiceError::NotFound { .. })
    ));
}

#[tokio::test]
async fn a_cascaded_delete_is_audited_against_each_relation_it_took_down() {
    let fx = fixture().await;
    let g = graph(&fx).await;
    let agent = Actor::named("agent:cleanup").unwrap();

    fx.delete_entity(&g.jan.id, &agent).await.unwrap();

    for relation in [&g.directorship, &g.family] {
        let trail = fx.relation_audit(&relation.id, None).await.unwrap();
        assert_eq!(trail.entries[0].action, AuditAction::Delete);
        assert_eq!(trail.entries[0].actor, "agent:cleanup");
        assert!(
            trail.entries[0].batch_id.is_some(),
            "a cascade groups its changes for undo"
        );
    }

    // Every edge the one deletion took down shares the entity's batch.
    let entity_trail = fx.entity_audit(&g.jan.id, None).await.unwrap();
    let batch = entity_trail.entries[0].batch_id.clone();
    assert!(batch.is_some());
    assert_eq!(
        fx.relation_audit(&g.family.id, None).await.unwrap().entries[0].batch_id,
        batch,
        "the entity and its edges went down together"
    );
}

#[tokio::test]
async fn an_id_a_caller_chose_is_used_and_cannot_be_taken_twice() {
    let fx = fixture().await;
    let g = graph(&fx).await;
    let id = vesica_store::new_id();

    let created = fx
        .create_relation(
            CreateRelation {
                id: Some(id.clone()),
                case_id: g.case.id.clone(),
                schema_id: "Associate".into(),
                source_id: g.jan.id.clone(),
                target_id: g.acme.id.clone(),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(created.id, id);

    let e = fx
        .create_relation(
            CreateRelation {
                id: Some(id.clone()),
                case_id: g.case.id.clone(),
                schema_id: "Associate".into(),
                source_id: g.jan.id.clone(),
                target_id: g.maria.id.clone(),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    assert!(matches!(e, ServiceError::Conflict { .. }));
    assert!(e.fix().is_some());

    let e = fx
        .create_relation(
            CreateRelation {
                id: Some("relation-1".into()),
                case_id: g.case.id.clone(),
                schema_id: "Associate".into(),
                source_id: g.jan.id.clone(),
                target_id: g.maria.id.clone(),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "id"),
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[tokio::test]
async fn purging_an_entity_destroys_the_relations_that_touched_it() {
    let fx = fixture().await;
    let g = graph(&fx).await;

    fx.purge_entity(
        vesica_api::services::PurgeEntity {
            entity_id: g.jan.id.clone(),
            confirm: true,
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    assert!(
        fx.list_relations(in_case(&g.case.id))
            .await
            .unwrap()
            .relations
            .is_empty()
    );
    assert!(matches!(
        fx.get_relation(&g.directorship.id).await,
        Err(ServiceError::NotFound { .. })
    ));
    // Restoring the entity is not possible, so nothing can bring the edges
    // back either: a purge is irreversible by design.
    assert!(matches!(
        fx.restore_relation(&g.directorship.id, &Actor::analyst())
            .await,
        Err(ServiceError::NotFound { .. })
    ));
}

#[tokio::test]
async fn an_entity_created_here_can_be_related_in_the_same_breath_by_an_agent() {
    // The subgraph-in-one-piece story from the spec: an agent mints both
    // ids itself, records two entities and the edge between them, and never
    // has to read anything back to do it.
    let fx = fixture().await;
    let case = seed_case(&fx, "Operation Lighthouse", None).await.unwrap();
    let agent = Actor::named("agent:importer").unwrap();
    let jan = vesica_store::new_id();
    let acme = vesica_store::new_id();

    for (id, schema_id, name) in [
        (&jan, "Person", "Jan Kowalski"),
        (&acme, "Company", "Acme sp. z o.o."),
    ] {
        fx.create_entity(
            CreateEntity {
                id: Some(id.clone()),
                case_id: case.id.clone(),
                schema_id: schema_id.into(),
                properties: property_values(&[("name", &[name])]),
                ..Default::default()
            },
            &agent,
        )
        .await
        .unwrap();
    }

    let edge = fx
        .create_relation(
            CreateRelation {
                case_id: case.id.clone(),
                schema_id: "Directorship".into(),
                source_id: jan.clone(),
                target_id: acme.clone(),
                properties: property_values(&[("role", &["chair"])]),
                interval: Some(SetInterval {
                    start: Some("2015".into()),
                    end: None,
                    precision: None,
                }),
                ..Default::default()
            },
            &agent,
        )
        .await
        .unwrap();

    assert_eq!(edge.created_by, "agent:importer");
    assert_eq!(edge.source_id, jan);
    assert_eq!(edge.target_id, acme);
}
