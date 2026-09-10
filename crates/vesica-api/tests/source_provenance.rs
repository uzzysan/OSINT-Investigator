//! Provenance, through the service layer: recording where a fact came
//! from, citing it at the granularity the claim was made at, and finding
//! the facts that have no source behind them at all.
//!
//! Every assertion here is something a caller can observe — a service call
//! in, a domain type out. Nothing reaches into the store, and nothing
//! asserts on SQL, per spec 0001's Testing Decisions.

use vesica_api::services::{
    Actor, CitationTarget, CiteSource, CiteTarget, CreateSource, ServiceError, SourceKind,
    SourceQuery, UnsourcedQuery,
};
use vesica_api::testing::{
    cite_entity, cite_property, seed_case, seed_person, seed_small_graph, seed_source,
    test_services,
};

// ---------------------------------------------------------------------------
// Recording a source
// ---------------------------------------------------------------------------

#[tokio::test]
async fn each_kind_of_source_can_be_recorded() {
    let fx = test_services().await.unwrap();
    let case = seed_case(&fx, "Operation Lighthouse", Some("PL"))
        .await
        .unwrap();
    let actor = Actor::analyst();

    let link = fx
        .create_source(
            CreateSource {
                case_id: case.id.clone(),
                kind: Some(SourceKind::Link),
                title: Some("KRS extract".into()),
                url: Some("https://rejestr.test/krs/12345".into()),
                ..Default::default()
            },
            &actor,
        )
        .await
        .unwrap();
    assert_eq!(link.kind, SourceKind::Link);

    let file = fx
        .create_source(
            CreateSource {
                case_id: case.id.clone(),
                kind: Some(SourceKind::File),
                title: Some("Scanned filing".into()),
                reference: Some("bag-17/scan-3.pdf".into()),
                ..Default::default()
            },
            &actor,
        )
        .await
        .unwrap();
    assert_eq!(file.reference.as_deref(), Some("bag-17/scan-3.pdf"));

    let manual = fx
        .create_source(
            CreateSource {
                case_id: case.id.clone(),
                kind: Some(SourceKind::Manual),
                title: Some("Interview with the neighbour".into()),
                excerpt: Some("Said the car was there every Thursday.".into()),
                ..Default::default()
            },
            &actor,
        )
        .await
        .unwrap();
    assert_eq!(manual.kind, SourceKind::Manual);

    let agent = fx
        .create_source(
            CreateSource {
                case_id: case.id.clone(),
                kind: Some(SourceKind::Agent),
                title: Some("Company-register sweep".into()),
                ..Default::default()
            },
            &Actor::named("agent:researcher").unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(agent.created_by, "agent:researcher");

    let listed = fx
        .list_sources(SourceQuery {
            case_id: case.id.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(listed.sources.len(), 4);
    assert!(!listed.truncated);

    let only_files = fx
        .list_sources(SourceQuery {
            case_id: case.id,
            kind: Some(SourceKind::File),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(only_files.sources.len(), 1);
    assert_eq!(only_files.sources[0].source.id, file.id);
}

#[tokio::test]
async fn a_kind_without_the_field_that_makes_it_that_kind_is_refused_with_a_fix() {
    let fx = test_services().await.unwrap();
    let case = seed_case(&fx, "Operation Lighthouse", None).await.unwrap();

    for (kind, field) in [(SourceKind::Link, "url"), (SourceKind::File, "reference")] {
        let e = fx
            .create_source(
                CreateSource {
                    case_id: case.id.clone(),
                    kind: Some(kind),
                    title: Some("Something".into()),
                    ..Default::default()
                },
                &Actor::analyst(),
            )
            .await
            .unwrap_err();
        match &e {
            ServiceError::Validation { field: at, .. } => assert_eq!(at, field),
            other => panic!("expected a validation error, got {other:?}"),
        }
        assert!(
            e.fix().is_some(),
            "an agent has to be able to correct itself"
        );
    }

    // Nothing identifying at all.
    let e = fx
        .create_source(
            CreateSource {
                case_id: case.id,
                kind: Some(SourceKind::Manual),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    assert!(e.fix().unwrap().contains("title"));
}

#[tokio::test]
async fn a_source_records_when_it_was_retrieved_not_only_when_it_was_typed() {
    let fx = test_services().await.unwrap();
    let case = seed_case(&fx, "Operation Lighthouse", None).await.unwrap();

    let source = fx
        .create_source(
            CreateSource {
                case_id: case.id,
                kind: Some(SourceKind::Link),
                url: Some("https://example.test/page".into()),
                retrieved_at: Some("2019-03-04T11:20:00Z".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();

    assert_eq!(source.retrieved_at.as_deref(), Some("2019-03-04T11:20:00Z"));
    assert_ne!(
        source.retrieved_at.as_deref(),
        Some(source.created_at.as_str()),
        "a fact is dated to when it was observed, not when it was typed"
    );
    assert!(
        source.created_at.as_str() > "2020",
        "the row was written now, long after the page was read"
    );

    let e = fx
        .create_source(
            CreateSource {
                case_id: source.case_id,
                kind: Some(SourceKind::Manual),
                title: Some("A note".into()),
                retrieved_at: Some("last tuesday".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    assert!(e.fix().unwrap().contains("observed"));
}

#[tokio::test]
async fn the_same_page_recorded_twice_is_one_source() {
    let fx = test_services().await.unwrap();
    let case = seed_case(&fx, "Operation Lighthouse", None).await.unwrap();

    let first = seed_source(
        &fx,
        &case.id,
        "https://Example.test/Filing?utm_source=newsletter&id=7",
    )
    .await
    .unwrap();
    assert_eq!(
        first.url.as_deref(),
        Some("https://Example.test/Filing?utm_source=newsletter&id=7"),
        "the locator is kept exactly as it was given"
    );
    assert_eq!(
        first.url_norm.as_deref(),
        Some("https://example.test/Filing?id=7"),
        "and normalised beside it, without the campaign parameters"
    );

    let again = seed_source(&fx, &case.id, "https://example.test/Filing?id=7#section-2")
        .await
        .unwrap();
    assert_eq!(
        again.id, first.id,
        "the same page cited twice is one source, not two"
    );

    let listed = fx
        .list_sources(SourceQuery {
            case_id: case.id,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(listed.sources.len(), 1);
}

#[tokio::test]
async fn the_same_page_in_another_case_is_another_source() {
    let fx = test_services().await.unwrap();
    let one = seed_case(&fx, "Operation Lighthouse", None).await.unwrap();
    let two = seed_case(&fx, "Operation Beacon", None).await.unwrap();

    let a = seed_source(&fx, &one.id, "https://example.test/page")
        .await
        .unwrap();
    let b = seed_source(&fx, &two.id, "https://example.test/page")
        .await
        .unwrap();
    assert_ne!(a.id, b.id, "a case is a boundary, evidence included");
}

// ---------------------------------------------------------------------------
// Citing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_source_can_be_cited_on_an_entity_a_relation_and_one_property_value() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let source = seed_source(&fx, &graph.case.id, "https://rejestr.test/krs/1")
        .await
        .unwrap();

    cite_entity(&fx, &source, &graph.jan).await.unwrap();
    cite_property(&fx, &source, &graph.jan, "name", 0)
        .await
        .unwrap();
    fx.cite_source(
        CiteSource {
            source_id: source.id.clone(),
            target: CiteTarget::Relation {
                relation_id: graph.directorship.id.clone(),
            },
            note: Some("the register lists him as chair".into()),
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    // Reading the Entity returns its citations — its own and its values'.
    let jan = fx.get_entity(&graph.jan.id).await.unwrap();
    assert_eq!(jan.citations.items.len(), 2);
    assert!(!jan.citations.truncated);
    assert!(jan.citations.items.iter().any(|c| matches!(
        &c.target,
        CitationTarget::Entity { entity_id } if *entity_id == graph.jan.id
    )));
    let value = jan
        .citations
        .items
        .iter()
        .find_map(|c| match &c.target {
            CitationTarget::Property { value, prop, .. } => Some((prop.clone(), value.clone())),
            _ => None,
        })
        .expect("the property value's citation comes back too");
    assert_eq!(value, ("name".to_string(), "Jan Kowalski".to_string()));

    // The value itself says which Source it rests on, and carries the
    // handle that cites it.
    let name = &jan.properties[0];
    assert_eq!(name.source_id.as_deref(), Some(source.id.as_str()));
    assert!(name.value_ref.is_some());

    // Reading the Relation returns its citations.
    let directorship = fx.get_relation(&graph.directorship.id).await.unwrap();
    assert_eq!(directorship.citations.items.len(), 1);
    assert_eq!(
        directorship.citations.items[0].note.as_deref(),
        Some("the register lists him as chair")
    );

    // And Maria, whom nothing cites, is unsourced — permitted, not refused.
    let maria = fx.get_entity(&graph.maria.id).await.unwrap();
    assert!(maria.citations.is_empty());
    assert!(maria.properties[0].source_id.is_none());
}

#[tokio::test]
async fn a_property_value_can_be_cited_by_its_handle_and_by_its_position() {
    let fx = test_services().await.unwrap();
    let case = seed_case(&fx, "Operation Lighthouse", Some("PL"))
        .await
        .unwrap();
    let jan = seed_person(
        &fx,
        &case.id,
        &[
            ("name", &["Jan Kowalski"]),
            ("phone", &["601 234 567", "602 000 111"]),
        ],
    )
    .await
    .unwrap();
    let source = seed_source(&fx, &case.id, "https://example.test/directory")
        .await
        .unwrap();

    // By position, as `PropertyValue::seq` reports it.
    cite_property(&fx, &source, &jan, "phone", 1).await.unwrap();

    // By the handle the read reported.
    let stored = fx.get_entity(&jan.id).await.unwrap();
    let first_phone = stored
        .properties
        .iter()
        .find(|p| p.prop == "phone" && p.seq == 0)
        .unwrap();
    fx.cite_source(
        CiteSource {
            source_id: source.id.clone(),
            target: CiteTarget::PropertyValue {
                value_ref: first_phone.value_ref.clone().unwrap(),
            },
            note: None,
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    let stored = fx.get_entity(&jan.id).await.unwrap();
    let cited: Vec<&str> = stored
        .citations
        .items
        .iter()
        .filter_map(|c| match &c.target {
            CitationTarget::Property { value, .. } => Some(value.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(cited.len(), 2);
    assert!(cited.contains(&"601 234 567"));
    assert!(cited.contains(&"602 000 111"));

    // Which of a repeating property's values is meant has to be said.
    let e = fx
        .cite_source(
            CiteSource {
                source_id: source.id,
                target: CiteTarget::EntityProperty {
                    entity_id: jan.id,
                    prop: "phone".into(),
                    seq: None,
                },
                note: None,
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    assert!(e.fix().unwrap().contains("seq"));
}

#[tokio::test]
async fn one_source_is_cited_many_times_and_the_listing_says_what_cites_it() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let source = seed_source(&fx, &graph.case.id, "https://rejestr.test/krs/1")
        .await
        .unwrap();

    cite_entity(&fx, &source, &graph.jan).await.unwrap();
    cite_entity(&fx, &source, &graph.acme).await.unwrap();
    let noted = fx
        .cite_source(
            CiteSource {
                source_id: source.id.clone(),
                target: CiteTarget::EntityProperty {
                    entity_id: graph.acme.id.clone(),
                    prop: "name".into(),
                    seq: None,
                },
                note: Some("as spelled in the register".into()),
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(noted.note.as_deref(), Some("as spelled in the register"));

    let listed = fx
        .list_sources(SourceQuery {
            case_id: graph.case.id,
            ..Default::default()
        })
        .await
        .unwrap();
    let usage = listed
        .sources
        .iter()
        .find(|u| u.source.id == source.id)
        .unwrap();
    assert_eq!(usage.citation_count, 3);
    assert_eq!(usage.citations.items.len(), 3);
    assert!(!usage.citations.truncated);
    assert!(
        usage.citations.items.iter().any(|c| matches!(
            &c.target,
            CitationTarget::Property { prop, .. } if prop == "name"
        )),
        "a listing has to say which value it was cited for, not just that it was"
    );
}

#[tokio::test]
async fn citing_the_same_source_for_the_same_fact_twice_is_one_claim() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let source = seed_source(&fx, &graph.case.id, "https://rejestr.test/krs/1")
        .await
        .unwrap();

    let first = cite_entity(&fx, &source, &graph.jan).await.unwrap();
    let again = cite_entity(&fx, &source, &graph.jan).await.unwrap();
    assert_eq!(first.id, again.id);

    let jan = fx.get_entity(&graph.jan.id).await.unwrap();
    assert_eq!(jan.citations.items.len(), 1);
}

#[tokio::test]
async fn evidence_does_not_cross_a_case_boundary() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let elsewhere = seed_case(&fx, "Operation Beacon", None).await.unwrap();
    let source = seed_source(&fx, &elsewhere.id, "https://example.test/page")
        .await
        .unwrap();

    let e = cite_entity(&fx, &source, &graph.jan).await.unwrap_err();
    match &e {
        ServiceError::Conflict { .. } => {}
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert!(e.fix().unwrap().contains("same case"));
}

#[tokio::test]
async fn a_citation_can_be_withdrawn_and_leaves_the_fact_alone() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let source = seed_source(&fx, &graph.case.id, "https://rejestr.test/krs/1")
        .await
        .unwrap();
    let citation = cite_property(&fx, &source, &graph.jan, "name", 0)
        .await
        .unwrap();

    fx.remove_citation(citation.id, &Actor::analyst())
        .await
        .unwrap();

    let jan = fx.get_entity(&graph.jan.id).await.unwrap();
    assert!(jan.citations.is_empty());
    assert!(jan.properties[0].source_id.is_none());
    assert_eq!(jan.properties[0].value, "Jan Kowalski");

    let e = fx
        .remove_citation(citation.id, &Actor::analyst())
        .await
        .unwrap_err();
    assert!(matches!(e, ServiceError::NotFound { .. }));
}

// ---------------------------------------------------------------------------
// Deleting a source
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_a_source_leaves_the_facts_intact_and_marks_them_unsourced() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let source = seed_source(&fx, &graph.case.id, "https://rejestr.test/krs/1")
        .await
        .unwrap();

    cite_entity(&fx, &source, &graph.jan).await.unwrap();
    cite_property(&fx, &source, &graph.jan, "name", 0)
        .await
        .unwrap();
    fx.cite_source(
        CiteSource {
            source_id: source.id.clone(),
            target: CiteTarget::Relation {
                relation_id: graph.directorship.id.clone(),
            },
            note: None,
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    fx.delete_source(&source.id, &Actor::analyst())
        .await
        .unwrap();

    let jan = fx.get_entity(&graph.jan.id).await.unwrap();
    assert_eq!(
        jan.caption, "Jan Kowalski",
        "the finding survives its source"
    );
    assert_eq!(jan.properties[0].value, "Jan Kowalski");
    assert!(
        jan.properties[0].source_id.is_none(),
        "and is unsourced again"
    );
    assert!(jan.citations.is_empty());

    let directorship = fx.get_relation(&graph.directorship.id).await.unwrap();
    assert!(directorship.citations.is_empty());

    assert!(matches!(
        fx.get_source(&source.id).await,
        Err(ServiceError::NotFound { .. })
    ));

    let unsourced = fx
        .unsourced_facts(UnsourcedQuery {
            case_id: graph.case.id,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(unsourced.entities.contains(&graph.jan.id));
    assert!(unsourced.relations.contains(&graph.directorship.id));
}

#[tokio::test]
async fn withdrawing_one_of_two_sources_falls_back_to_the_other() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let first = seed_source(&fx, &graph.case.id, "https://rejestr.test/krs/1")
        .await
        .unwrap();
    let second = seed_source(&fx, &graph.case.id, "https://gazeta.test/article/9")
        .await
        .unwrap();

    cite_property(&fx, &first, &graph.jan, "name", 0)
        .await
        .unwrap();
    cite_property(&fx, &second, &graph.jan, "name", 0)
        .await
        .unwrap();

    let jan = fx.get_entity(&graph.jan.id).await.unwrap();
    assert_eq!(
        jan.properties[0].source_id.as_deref(),
        Some(first.id.as_str()),
        "the value rests on the first claim made about it"
    );
    assert_eq!(jan.citations.items.len(), 2);

    fx.delete_source(&first.id, &Actor::analyst())
        .await
        .unwrap();

    let jan = fx.get_entity(&graph.jan.id).await.unwrap();
    assert_eq!(
        jan.properties[0].source_id.as_deref(),
        Some(second.id.as_str()),
        "withdrawing one source must not make a doubly-sourced value look unsourced"
    );
    assert_eq!(jan.citations.items.len(), 1);
}

#[tokio::test]
async fn a_citation_does_not_survive_the_value_it_cites() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let source = seed_source(&fx, &graph.case.id, "https://rejestr.test/krs/1")
        .await
        .unwrap();
    cite_property(&fx, &source, &graph.jan, "name", 0)
        .await
        .unwrap();

    fx.edit_entity_properties(
        &graph.jan.id,
        vesica_api::services::EditProperties {
            replace: vesica_api::testing::property_values(&[("name", &["Jan Kowalsky"])]),
            ..Default::default()
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    let jan = fx.get_entity(&graph.jan.id).await.unwrap();
    assert_eq!(jan.properties[0].value, "Jan Kowalsky");
    assert!(
        jan.citations.is_empty(),
        "a changed value is a different claim and carries no provenance across"
    );
    assert!(jan.properties[0].source_id.is_none());

    // The Source itself is untouched — it is simply no longer cited.
    let source = fx.get_source(&source.id).await.unwrap();
    let listed = fx
        .list_sources(SourceQuery {
            case_id: graph.case.id,
            ..Default::default()
        })
        .await
        .unwrap();
    let usage = listed
        .sources
        .iter()
        .find(|u| u.source.id == source.id)
        .unwrap();
    assert_eq!(usage.citation_count, 0);
}

#[tokio::test]
async fn purging_an_entity_takes_its_citations_with_it() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let source = seed_source(&fx, &graph.case.id, "https://rejestr.test/krs/1")
        .await
        .unwrap();
    cite_entity(&fx, &source, &graph.jan).await.unwrap();
    cite_property(&fx, &source, &graph.jan, "name", 0)
        .await
        .unwrap();
    fx.cite_source(
        CiteSource {
            source_id: source.id.clone(),
            target: CiteTarget::Relation {
                relation_id: graph.directorship.id.clone(),
            },
            note: None,
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    fx.purge_entity(
        vesica_api::services::PurgeEntity {
            entity_id: graph.jan.id.clone(),
            confirm: true,
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    let listed = fx
        .list_sources(SourceQuery {
            case_id: graph.case.id,
            ..Default::default()
        })
        .await
        .unwrap();
    let usage = listed
        .sources
        .iter()
        .find(|u| u.source.id == source.id)
        .unwrap();
    assert_eq!(
        usage.citation_count, 0,
        "the entity, its values and the relations that went with it took their citations too"
    );
}

// ---------------------------------------------------------------------------
// Unsourced facts and coverage
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_unsourced_report_shrinks_as_the_gaps_are_closed() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let source = seed_source(&fx, &graph.case.id, "https://rejestr.test/krs/1")
        .await
        .unwrap();

    let before = fx
        .unsourced_facts(UnsourcedQuery {
            case_id: graph.case.id.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(before.entities.len(), 3, "Jan, Maria and Acme");
    assert_eq!(before.relations.len(), 2);
    assert!(
        before
            .property_values
            .iter()
            .any(|v| v.value == "Jan Kowalski"),
        "a property value with no source is a gap of its own"
    );
    assert!(
        before.property_values.iter().any(|v| v.value == "chair"),
        "a relation's own property values count too"
    );
    assert!(!before.entities_truncated);

    cite_entity(&fx, &source, &graph.jan).await.unwrap();
    cite_property(&fx, &source, &graph.jan, "name", 0)
        .await
        .unwrap();

    let after = fx
        .unsourced_facts(UnsourcedQuery {
            case_id: graph.case.id,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(!after.entities.contains(&graph.jan.id));
    assert_eq!(after.entities.len(), 2);
    assert!(
        !after
            .property_values
            .iter()
            .any(|v| v.value == "Jan Kowalski")
    );
}

#[tokio::test]
async fn an_unsourced_value_reports_the_handle_that_cites_it() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let source = seed_source(&fx, &graph.case.id, "https://rejestr.test/krs/1")
        .await
        .unwrap();

    let unsourced = fx
        .unsourced_facts(UnsourcedQuery {
            case_id: graph.case.id.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    let gap = unsourced
        .property_values
        .iter()
        .find(|v| v.value == "Jan Kowalski")
        .unwrap();
    assert_eq!(gap.host_id, graph.jan.id);
    assert_eq!(gap.prop, "name");

    // The report hands back exactly what closes the gap.
    fx.cite_source(
        CiteSource {
            source_id: source.id,
            target: CiteTarget::PropertyValue {
                value_ref: gap.value_ref.clone(),
            },
            note: None,
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    let after = fx
        .unsourced_facts(UnsourcedQuery {
            case_id: graph.case.id,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        !after
            .property_values
            .iter()
            .any(|v| v.value == "Jan Kowalski")
    );
}

#[tokio::test]
async fn the_unsourced_report_is_bounded_and_says_when_it_was_cut_short() {
    let fx = test_services().await.unwrap();
    let case = seed_case(&fx, "Operation Lighthouse", None).await.unwrap();
    for i in 0..4 {
        seed_person(&fx, &case.id, &[("name", &[&format!("Person {i}")])])
            .await
            .unwrap();
    }

    let page = fx
        .unsourced_facts(UnsourcedQuery {
            case_id: case.id,
            limit: Some(2),
        })
        .await
        .unwrap();
    assert_eq!(page.entities.len(), 2);
    assert!(page.entities_truncated);
    assert_eq!(page.limit, 2);
    assert!(page.property_values_truncated);
}

#[tokio::test]
async fn case_level_counts_report_sourced_against_unsourced() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let source = seed_source(&fx, &graph.case.id, "https://rejestr.test/krs/1")
        .await
        .unwrap();

    let before = fx.source_coverage(&graph.case.id).await.unwrap();
    assert_eq!(before.entities.total, 3);
    assert_eq!(before.entities.sourced, 0);
    assert_eq!(before.entities.unsourced, 3);
    assert_eq!(before.relations.total, 2);
    assert!(!before.entities.capped, "a small case is counted exactly");
    assert_eq!(
        before.property_values.total, 4,
        "three names and the directorship's role"
    );
    assert_eq!(before.property_values.sourced, 0);

    cite_entity(&fx, &source, &graph.jan).await.unwrap();
    cite_property(&fx, &source, &graph.jan, "name", 0)
        .await
        .unwrap();
    fx.cite_source(
        CiteSource {
            source_id: source.id.clone(),
            target: CiteTarget::RelationProperty {
                relation_id: graph.directorship.id.clone(),
                prop: "role".into(),
                seq: None,
            },
            note: None,
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    let after = fx.source_coverage(&graph.case.id).await.unwrap();
    assert_eq!(after.entities.sourced, 1);
    assert_eq!(after.entities.unsourced, 2);
    assert_eq!(
        after.property_values.sourced, 2,
        "one entity value and one relation value"
    );
    assert_eq!(after.property_values.unsourced, 2);
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_provenance_mutation_is_audited_with_its_actor() {
    let fx = test_services().await.unwrap();
    let graph = seed_small_graph(&fx).await.unwrap();
    let agent = Actor::named("agent:researcher").unwrap();

    let source = fx
        .create_source(
            CreateSource {
                case_id: graph.case.id.clone(),
                kind: Some(SourceKind::Link),
                url: Some("https://rejestr.test/krs/1".into()),
                ..Default::default()
            },
            &agent,
        )
        .await
        .unwrap();

    let citation = fx
        .cite_source(
            CiteSource {
                source_id: source.id.clone(),
                target: CiteTarget::EntityProperty {
                    entity_id: graph.jan.id.clone(),
                    prop: "name".into(),
                    seq: None,
                },
                note: None,
            },
            &agent,
        )
        .await
        .unwrap();

    // A citation of a property value is filed against the Entity it is
    // about, because that is where a reader will look for it.
    let trail = fx.entity_audit(&graph.jan.id, None).await.unwrap();
    let cite = trail
        .entries
        .iter()
        .find(|e| e.action == vesica_api::services::AuditAction::Cite)
        .expect("citing a value shows up in its entity's trail");
    assert_eq!(cite.actor, "agent:researcher");
    assert_eq!(cite.case_id.as_deref(), Some(graph.case.id.as_str()));
    assert_eq!(
        cite.after.as_ref().unwrap()["source_id"].as_str(),
        Some(source.id.as_str())
    );

    fx.remove_citation(citation.id, &agent).await.unwrap();
    let trail = fx.entity_audit(&graph.jan.id, None).await.unwrap();
    assert!(
        trail
            .entries
            .iter()
            .any(|e| e.action == vesica_api::services::AuditAction::Uncite)
    );

    // The Source's own life is recorded against the Source.
    let trail = fx.source_audit(&source.id, None).await.unwrap();
    assert_eq!(trail.entries.len(), 1);
    assert_eq!(trail.entries[0].actor, "agent:researcher");
    assert_eq!(
        trail.entries[0].action,
        vesica_api::services::AuditAction::Create
    );

    fx.delete_source(&source.id, &agent).await.unwrap();
    let trail = fx.source_audit(&source.id, None).await.unwrap();
    assert_eq!(trail.entries.len(), 2);
    assert_eq!(
        trail.entries[0].action,
        vesica_api::services::AuditAction::Delete,
        "the trail outlives what it describes"
    );
}

#[tokio::test]
async fn a_source_for_a_case_that_is_not_there_is_refused() {
    let fx = test_services().await.unwrap();
    let e = fx
        .create_source(
            CreateSource {
                case_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                kind: Some(SourceKind::Manual),
                title: Some("A note".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    assert!(matches!(e, ServiceError::NotFound { kind: "case", .. }));
}
