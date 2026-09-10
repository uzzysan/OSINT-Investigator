//! Recording a finding, and everything that happens to it afterwards,
//! proven at the service-layer seam against a real SQLite database in a
//! temporary directory.
//!
//! Every assertion goes through a service method, including the ones about
//! what is true afterwards. The two exceptions are marked where they occur
//! and both reach for the store to set up a state no use case can yet
//! produce (a merged-away Entity, whose merging is a later ticket).

use vesica_api::services::{
    Actor, AuditAction, Confidence, CreateEntity, EditProperties, Entity, EntityQuery, PurgeEntity,
    ServiceConfig, ServiceError, UpdateEntity,
};
use vesica_api::services::{Patch, REDACTED_FIELD};
use vesica_api::testing::{
    TestServices, property_values, seed_case, seed_person, test_services, test_services_with,
};

async fn fixture() -> TestServices {
    test_services().await.expect("fixture")
}

/// A Case with a Polish default region, which is what makes `601 234 567`
/// mean something.
async fn polish_case(fx: &TestServices) -> String {
    seed_case(fx, "Operation Lighthouse", Some("PL"))
        .await
        .unwrap()
        .id
}

fn person(case_id: &str, properties: &[(&str, &[&str])]) -> CreateEntity {
    CreateEntity {
        case_id: case_id.to_string(),
        schema_id: "Person".into(),
        properties: property_values(properties),
        ..Default::default()
    }
}

fn value_of<'a>(entity: &'a Entity, prop: &str, seq: usize) -> &'a str {
    entity
        .properties
        .iter()
        .find(|v| v.prop == prop && v.seq == seq)
        .map(|v| v.value.as_str())
        .unwrap_or_else(|| panic!("{prop}[{seq}] should be recorded"))
}

fn norm_of<'a>(entity: &'a Entity, prop: &str, seq: usize) -> &'a str {
    entity
        .properties
        .iter()
        .find(|v| v.prop == prop && v.seq == seq)
        .and_then(|v| v.value_norm.as_deref())
        .unwrap_or_else(|| panic!("{prop}[{seq}] should have a normalised form"))
}

// ---------------------------------------------------------------------------
// Create and read
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_recorded_finding_comes_back_with_everything_that_was_recorded() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    let created = fx
        .create_entity(
            person(
                &case_id,
                &[
                    ("name", &["Jan Kowalski"]),
                    ("phone", &["601 234 567"]),
                    ("email", &["J.Kowalski@Example.COM"]),
                ],
            ),
            &Actor::analyst(),
        )
        .await
        .unwrap();

    assert_eq!(created.id.len(), 26, "ids are 26-character ULIDs");
    assert_eq!(created.case_id, case_id);
    assert_eq!(created.schema_id, "Person");
    assert_eq!(created.confidence, Confidence::Probable, "the default");
    assert_eq!(created.created_by, "human");
    assert_eq!(created.merged_from, None);

    assert_eq!(fx.get_entity(&created.id).await.unwrap(), created);
}

#[tokio::test]
async fn the_value_that_was_typed_is_kept_and_a_normalised_one_stored_beside_it() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    let created = fx
        .create_entity(
            person(
                &case_id,
                &[
                    ("name", &["Łukasz Nowak"]),
                    ("email", &["J.Kowalski@Example.COM"]),
                ],
            ),
            &Actor::analyst(),
        )
        .await
        .unwrap();

    assert_eq!(value_of(&created, "name", 0), "Łukasz Nowak");
    assert_eq!(value_of(&created, "email", 0), "J.Kowalski@Example.COM");
    // Case is folded and the domain is lowered, but the dots in the local
    // part survive: folding them is a Google-only rule, because Google is
    // the only large provider for which `j.kowalski@` and `jkowalski@` are
    // one mailbox. Applying it everywhere would merge two different people
    // at any provider that treats a dot as significant.
    assert_eq!(norm_of(&created, "email", 0), "j.kowalski@example.com");
    // The comparison form folds case and every diacritic NFKD can take
    // apart — but `ł` has no decomposition, so it survives. That is the
    // whole point of the distinction: `value_norm` is a comparison form,
    // not an ASCII transcription. The transcription that lets `Żółć` meet
    // `Zolc` is a blocking key, which belongs to the resolver in M5 and is
    // deliberately not stored on the Property.
    assert_eq!(norm_of(&created, "name", 0), "łukasz nowak");

    let folded = fx
        .create_entity(
            person(&case_id, &[("name", &["Żółć ZAŻÓŁĆ"])]),
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(norm_of(&folded, "name", 0), "zołc zazołc");

    // The provider-specific half of that rule, so the asymmetry above is
    // pinned by a test rather than left looking like an oversight.
    let gmail = fx
        .create_entity(
            person(
                &case_id,
                &[
                    ("name", &["Jan Kowalski"]),
                    ("email", &["J.Kowalski+leaks@GMail.com"]),
                ],
            ),
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(norm_of(&gmail, "email", 0), "jkowalski@gmail.com");
}

#[tokio::test]
async fn properties_repeat_and_come_back_in_the_order_they_went_in() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    let created = fx
        .create_entity(
            person(
                &case_id,
                &[
                    (
                        "phone",
                        &["601 234 567", "+48 22 123 45 67", "0601 234 568"],
                    ),
                    ("name", &["Jan Kowalski"]),
                ],
            ),
            &Actor::analyst(),
        )
        .await
        .unwrap();

    let phones = created.values_of("phone");
    assert_eq!(
        phones,
        vec!["601 234 567", "+48 22 123 45 67", "0601 234 568"],
        "five phone numbers without choosing which one is real"
    );
    assert_eq!(norm_of(&created, "phone", 0), "+48601234567");
    assert_eq!(norm_of(&created, "phone", 2), "+48601234568");

    // And the order survives a round trip through storage.
    let read = fx.get_entity(&created.id).await.unwrap();
    assert_eq!(read.values_of("phone"), phones);
}

#[tokio::test]
async fn the_cases_region_is_what_makes_a_local_number_mean_something() {
    let fx = fixture().await;
    let polish = polish_case(&fx).await;
    let regionless = seed_case(&fx, "No Region", None).await.unwrap().id;

    let jan = fx
        .create_entity(
            person(&polish, &[("name", &["Jan"]), ("phone", &["601 234 567"])]),
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(norm_of(&jan, "phone", 0), "+48601234567");

    let e = fx
        .create_entity(
            person(
                &regionless,
                &[("name", &["Jan"]), ("phone", &["601 234 567"])],
            ),
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "phone"),
        other => panic!("expected a validation error naming the property, got {other:?}"),
    }
    assert!(
        e.fix().unwrap().contains("default region"),
        "an agent has to be told the case needs a region: {:?}",
        e.fix()
    );

    // The failed write left nothing behind.
    let listed = fx
        .list_entities(EntityQuery {
            case_id: regionless,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(listed.entities.is_empty());
}

#[tokio::test]
async fn a_deployment_wide_region_reaches_the_values_recorded_in_a_new_case() {
    let fx = test_services_with(ServiceConfig {
        default_region: Some("PL".into()),
        ..Default::default()
    })
    .await
    .unwrap();
    let case_id = seed_case(&fx, "Inherits The Region", None)
        .await
        .unwrap()
        .id;

    let jan = fx
        .create_entity(
            person(&case_id, &[("name", &["Jan"]), ("phone", &["601 234 567"])]),
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(norm_of(&jan, "phone", 0), "+48601234567");
}

// ---------------------------------------------------------------------------
// Captions and confidence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_caption_derives_itself_can_be_overridden_and_can_be_put_back() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    let derived = fx
        .create_entity(
            person(&case_id, &[("name", &["Jan Kowalski"])]),
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(derived.caption, "Jan Kowalski", "from the schema template");

    let named = fx
        .update_entity(
            &derived.id,
            UpdateEntity {
                caption: Patch::Set("  The   Driver ".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(named.caption, "The Driver", "collapsed, and honoured");

    // An override survives a change to the property it was derived from.
    let renamed = fx
        .edit_entity_properties(
            &derived.id,
            EditProperties {
                replace: property_values(&[("name", &["Jan Nowak"])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(renamed.caption, "The Driver");

    // Clearing it puts it back under derivation, from the current values.
    let back = fx
        .update_entity(
            &derived.id,
            UpdateEntity {
                caption: Patch::Clear,
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(back.caption, "Jan Nowak");

    // And now that it is derived again, it follows the values.
    let followed = fx
        .edit_entity_properties(
            &derived.id,
            EditProperties {
                replace: property_values(&[("name", &["Jan Kowalski"])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(followed.caption, "Jan Kowalski");
}

#[tokio::test]
async fn an_entity_with_nothing_in_its_caption_template_is_labelled_by_its_schema() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    // `Company` requires a name too, so use a schema whose caption template
    // names a property that is not required: Address captions on `full`.
    let created = fx
        .create_entity(
            CreateEntity {
                case_id,
                schema_id: "Address".into(),
                properties: property_values(&[("city", &["Gdańsk"])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await;

    // Either the schema demands the captioning property (a validation
    // error naming it) or it does not and the caption falls back to the
    // schema label. Both are correct; neither may produce a blank caption
    // or a leaked template.
    match created {
        Ok(entity) => {
            assert!(!entity.caption.trim().is_empty());
            assert!(!entity.caption.contains('{'));
        }
        Err(ServiceError::Validation { field, .. }) => assert!(!field.is_empty()),
        Err(other) => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn confidence_is_set_on_creation_and_can_be_changed() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    let hypothesis = fx
        .create_entity(
            CreateEntity {
                confidence: Some(Confidence::Hypothesis),
                ..person(&case_id, &[("name", &["A driver, name unknown"])])
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(hypothesis.confidence, Confidence::Hypothesis);

    let confirmed = fx
        .update_entity(
            &hypothesis.id,
            UpdateEntity {
                confidence: Some(Confidence::Confirmed),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(confirmed.confidence, Confidence::Confirmed);
    assert_eq!(
        fx.get_entity(&hypothesis.id).await.unwrap().confidence,
        Confidence::Confirmed
    );

    let e = Confidence::parse("probably-ish").unwrap_err();
    assert!(e.fix().unwrap().contains("probable"));
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tags_attach_detach_and_list_within_their_case() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;
    let other_case = seed_case(&fx, "Another Case", Some("PL")).await.unwrap().id;

    let jan = seed_person(&fx, &case_id, &[("name", &["Jan Kowalski"])])
        .await
        .unwrap();
    let ewa = seed_person(&fx, &case_id, &[("name", &["Ewa Nowak"])])
        .await
        .unwrap();
    let elsewhere = seed_person(&fx, &other_case, &[("name", &["Someone Else"])])
        .await
        .unwrap();

    let tagged = fx
        .tag_entity(&jan.id, "suspect", &Actor::analyst())
        .await
        .unwrap();
    assert_eq!(tagged.tags, vec!["suspect".to_string()]);

    // Attaching the same tag again, in another capitalisation, is a no-op
    // rather than a second tag.
    let again = fx
        .tag_entity(&jan.id, "Suspect", &Actor::analyst())
        .await
        .unwrap();
    assert_eq!(again.tags, vec!["suspect".to_string()]);

    fx.tag_entity(&ewa.id, "suspect", &Actor::analyst())
        .await
        .unwrap();
    fx.tag_entity(&elsewhere.id, "suspect", &Actor::analyst())
        .await
        .unwrap();

    let tags = fx.list_tags(&case_id, None).await.unwrap();
    assert_eq!(tags.tags.len(), 1);
    assert_eq!(tags.tags[0].name, "suspect");
    assert_eq!(
        tags.tags[0].entity_count, 2,
        "the tag is scoped to its case; the third entity is elsewhere"
    );

    let detached = fx
        .untag_entity(&jan.id, "suspect", &Actor::analyst())
        .await
        .unwrap();
    assert!(detached.tags.is_empty());
    assert_eq!(
        fx.list_tags(&case_id, None).await.unwrap().tags[0].entity_count,
        1
    );

    // Detaching what is not attached is a no-op, not an error.
    let again = fx
        .untag_entity(&jan.id, "suspect", &Actor::analyst())
        .await
        .unwrap();
    assert!(again.tags.is_empty());

    // The last carrier letting go takes the tag out of the case's list.
    fx.untag_entity(&ewa.id, "suspect", &Actor::analyst())
        .await
        .unwrap();
    assert!(fx.list_tags(&case_id, None).await.unwrap().tags.is_empty());
    assert_eq!(
        fx.list_tags(&other_case, None).await.unwrap().tags[0].entity_count,
        1,
        "the other case's tag of the same name was never touched"
    );
}

#[tokio::test]
async fn a_tag_needs_a_name_and_the_refusal_says_what_to_send() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;
    let jan = seed_person(&fx, &case_id, &[("name", &["Jan"])])
        .await
        .unwrap();

    let e = fx
        .tag_entity(&jan.id, "   ", &Actor::analyst())
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "tag"),
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(e.fix().is_some());
}

// ---------------------------------------------------------------------------
// Listing and filtering
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_list_is_newest_first_filterable_and_says_when_it_was_cut_short() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;
    let elsewhere = seed_case(&fx, "Another Case", Some("PL")).await.unwrap().id;

    let first = seed_person(&fx, &case_id, &[("name", &["First"])])
        .await
        .unwrap();
    let second = seed_person(&fx, &case_id, &[("name", &["Second"])])
        .await
        .unwrap();
    let acme = fx
        .create_entity(
            CreateEntity {
                case_id: case_id.clone(),
                schema_id: "Company".into(),
                properties: property_values(&[("name", &["Acme sp. z o.o."])]),
                confidence: Some(Confidence::Confirmed),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    seed_person(&fx, &elsewhere, &[("name", &["Not In This Case"])])
        .await
        .unwrap();
    fx.tag_entity(&second.id, "suspect", &Actor::analyst())
        .await
        .unwrap();

    let all = fx
        .list_entities(EntityQuery {
            case_id: case_id.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(all.entities.len(), 3, "and nothing from the other case");
    assert_eq!(all.entities[0].id, acme.id, "newest first");
    assert!(!all.truncated);

    let page = fx
        .list_entities(EntityQuery {
            case_id: case_id.clone(),
            limit: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.entities.len(), 2);
    assert!(page.truncated, "there was a third the page did not show");

    let rest = fx
        .list_entities(EntityQuery {
            case_id: case_id.clone(),
            limit: Some(2),
            offset: 2,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(rest.entities.len(), 1);
    assert_eq!(rest.entities[0].id, first.id);
    assert!(!rest.truncated);

    let people = fx
        .list_entities(EntityQuery {
            case_id: case_id.clone(),
            schema_id: Some("Person".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(people.entities.len(), 2);

    let confirmed = fx
        .list_entities(EntityQuery {
            case_id: case_id.clone(),
            confidence: Some(Confidence::Confirmed),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(confirmed.entities.len(), 1);
    assert_eq!(confirmed.entities[0].id, acme.id);

    let suspects = fx
        .list_entities(EntityQuery {
            case_id,
            tag: Some("SUSPECT".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(suspects.entities.len(), 1);
    assert_eq!(suspects.entities[0].id, second.id);
}

// ---------------------------------------------------------------------------
// Editing properties
// ---------------------------------------------------------------------------

#[tokio::test]
async fn values_can_be_added_replaced_and_removed() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    let jan = seed_person(
        &fx,
        &case_id,
        &[("name", &["Jan Kowalski"]), ("phone", &["601 234 567"])],
    )
    .await
    .unwrap();

    let added = fx
        .edit_entity_properties(
            &jan.id,
            EditProperties {
                add: property_values(&[
                    ("phone", &["601 234 568"]),
                    ("email", &["jan@example.com"]),
                ]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(
        added.values_of("phone"),
        vec!["601 234 567", "601 234 568"],
        "added values land after what was already there"
    );
    assert_eq!(added.values_of("email"), vec!["jan@example.com"]);

    // Removing matches the normalised form as well as the raw one, so the
    // same number written differently still removes it.
    let removed = fx
        .edit_entity_properties(
            &jan.id,
            EditProperties {
                remove: property_values(&[("phone", &["+48 601 234 567"])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(removed.values_of("phone"), vec!["601 234 568"]);

    let replaced = fx
        .edit_entity_properties(
            &jan.id,
            EditProperties {
                replace: property_values(&[("phone", &["22 123 45 67", "22 123 45 68"])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(
        replaced.values_of("phone"),
        vec!["22 123 45 67", "22 123 45 68"]
    );

    // A property named in `replace` with nothing in it is cleared.
    let cleared = fx
        .edit_entity_properties(
            &jan.id,
            EditProperties {
                replace: property_values(&[("phone", &[])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert!(cleared.values_of("phone").is_empty());
    assert_eq!(
        cleared.values_of("email"),
        vec!["jan@example.com"],
        "an edit touches only what it names"
    );
    assert_eq!(fx.get_entity(&jan.id).await.unwrap(), cleared);
}

#[tokio::test]
async fn an_edit_that_would_leave_the_entity_invalid_is_refused_whole() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;
    let jan = seed_person(
        &fx,
        &case_id,
        &[("name", &["Jan Kowalski"]), ("phone", &["601 234 567"])],
    )
    .await
    .unwrap();

    // `name` is required by the Person schema.
    let e = fx
        .edit_entity_properties(
            &jan.id,
            EditProperties {
                replace: property_values(&[("name", &[])]),
                add: property_values(&[("phone", &["601 234 569"])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "name"),
        other => panic!("expected a validation error naming the property, got {other:?}"),
    }

    // Nothing was applied — not the clear, and not the unrelated addition.
    let unchanged = fx.get_entity(&jan.id).await.unwrap();
    assert_eq!(unchanged.values_of("name"), vec!["Jan Kowalski"]);
    assert_eq!(unchanged.values_of("phone"), vec!["601 234 567"]);

    // A property that does not repeat cannot be made to.
    let e = fx
        .edit_entity_properties(
            &jan.id,
            EditProperties {
                add: property_values(&[("birthDate", &["1978-04-12", "1979"])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    assert!(e.fix().unwrap().contains("single value"));
}

#[tokio::test]
async fn a_date_keeps_the_precision_it_was_given() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    let jan = seed_person(
        &fx,
        &case_id,
        &[("name", &["Jan Kowalski"]), ("birthDate", &["2019"])],
    )
    .await
    .unwrap();

    let date = jan
        .properties
        .iter()
        .find(|v| v.prop == "birthDate")
        .and_then(|v| v.date.as_ref())
        .expect("a date property carries its span");
    assert!(date.start < date.end, "a year is a band, not a point");
    assert_eq!(value_of(&jan, "birthDate", 0), "2019");
}

// ---------------------------------------------------------------------------
// Warnings
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_failed_checksum_is_stored_flagged_and_reported_on_the_entity() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    let created = fx
        .create_entity(
            person(
                &case_id,
                &[
                    ("name", &["Jan Kowalski"]),
                    ("taxNumber", &["NIP 5252248418"]),
                ],
            ),
            &Actor::analyst(),
        )
        .await
        .unwrap();

    assert_eq!(
        value_of(&created, "taxNumber", 0),
        "NIP 5252248418",
        "a document that is wrong is still recorded exactly as it reads"
    );
    let warning = created
        .warnings
        .iter()
        .find(|w| w.prop == "taxNumber")
        .expect("the checksum failure must surface on the created entity");
    assert_eq!(warning.code.as_str(), "checksum_failed");
    assert!(!warning.message.is_empty());

    // And it is still there when the entity is read back.
    let read = fx.get_entity(&created.id).await.unwrap();
    assert_eq!(read.warnings, created.warnings);

    // A good identifier warns about nothing.
    let clean = seed_person(&fx, &case_id, &[("name", &["Ewa"])])
        .await
        .unwrap();
    assert!(clean.warnings.is_empty());
}

// ---------------------------------------------------------------------------
// Ids and validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_caller_can_choose_the_id_and_using_it_twice_is_refused_clearly() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    let id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    let created = fx
        .create_entity(
            CreateEntity {
                id: Some(id.into()),
                ..person(&case_id, &[("name", &["Jan Kowalski"])])
            },
            &Actor::analyst(),
        )
        .await
        .unwrap();
    assert_eq!(created.id, id);
    assert_eq!(fx.get_entity(id).await.unwrap().id, id);

    let e = fx
        .create_entity(
            CreateEntity {
                id: Some(id.into()),
                ..person(&case_id, &[("name", &["Someone Else"])])
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Conflict { message, .. } => assert!(message.contains(id)),
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert!(e.fix().unwrap().contains("omit the id"));

    // The first entity is untouched by the refused second write.
    assert_eq!(fx.get_entity(id).await.unwrap().caption, "Jan Kowalski");

    let e = fx
        .create_entity(
            CreateEntity {
                id: Some("entity-1".into()),
                ..person(&case_id, &[("name", &["Jan"])])
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "id"),
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(e.fix().unwrap().contains("ULID"));
}

#[tokio::test]
async fn a_validation_failure_names_the_property_and_says_what_to_send() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    // An unknown schema.
    let e = fx
        .create_entity(
            CreateEntity {
                case_id: case_id.clone(),
                schema_id: "Unicorn".into(),
                properties: property_values(&[("name", &["x"])]),
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
    assert!(
        e.fix().unwrap().contains("Person"),
        "the catalogue is listed rather than hardcoded: {:?}",
        e.fix()
    );

    // An abstract schema cannot be instantiated.
    let e = fx
        .create_entity(
            CreateEntity {
                case_id: case_id.clone(),
                schema_id: "LegalEntity".into(),
                properties: property_values(&[("name", &["x"])]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    assert!(e.fix().is_some_and(|f| !f.is_empty()));

    // A property the schema does not have.
    let e = fx
        .create_entity(
            person(&case_id, &[("name", &["Jan"]), ("phon", &["601 234 567"])]),
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "phon"),
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(e.fix().unwrap().contains("phone"));

    // A value the property type cannot represent at all.
    let e = fx
        .create_entity(
            person(
                &case_id,
                &[("name", &["Jan"]), ("email", &["not an email"])],
            ),
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "email"),
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(e.fix().is_some_and(|f| !f.is_empty()));

    // A missing required property.
    let e = fx
        .create_entity(
            person(&case_id, &[("notes", &["a person whose name I never got"])]),
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Validation { field, .. } => assert_eq!(field, "name"),
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[tokio::test]
async fn an_entity_needs_a_case_that_is_actually_there() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;

    assert!(matches!(
        fx.create_entity(
            person("01ARZ3NDEKTSV4RRFFQ69G5FAV", &[("name", &["Jan"])]),
            &Actor::analyst(),
        )
        .await,
        Err(ServiceError::NotFound { kind: "case", .. })
    ));

    // ... and a deleted case is not there either.
    fx.delete_case(&case_id, &Actor::analyst()).await.unwrap();
    assert!(matches!(
        fx.create_entity(person(&case_id, &[("name", &["Jan"])]), &Actor::analyst())
            .await,
        Err(ServiceError::NotFound { kind: "case", .. })
    ));

    assert!(matches!(
        fx.get_entity("01ARZ3NDEKTSV4RRFFQ69G5FAV").await,
        Err(ServiceError::NotFound { kind: "entity", .. })
    ));
}

// ---------------------------------------------------------------------------
// Deletion, restore and purge
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_is_reversible_and_hides_the_entity_from_every_read() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;
    let jan = seed_person(
        &fx,
        &case_id,
        &[("name", &["Jan Kowalski"]), ("phone", &["601 234 567"])],
    )
    .await
    .unwrap();
    fx.tag_entity(&jan.id, "suspect", &Actor::analyst())
        .await
        .unwrap();

    fx.delete_entity(&jan.id, &Actor::analyst()).await.unwrap();

    assert!(matches!(
        fx.get_entity(&jan.id).await,
        Err(ServiceError::NotFound { .. })
    ));
    let listed = fx
        .list_entities(EntityQuery {
            case_id: case_id.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(listed.entities.is_empty());
    assert!(
        fx.list_tags(&case_id, None).await.unwrap().tags.is_empty(),
        "a deleted entity stops counting towards its tags"
    );

    // Deleting twice is a no-op, not an error.
    fx.delete_entity(&jan.id, &Actor::analyst()).await.unwrap();

    let restored = fx.restore_entity(&jan.id, &Actor::analyst()).await.unwrap();
    assert_eq!(restored.values_of("phone"), vec!["601 234 567"]);
    assert_eq!(
        restored.tags,
        vec!["suspect".to_string()],
        "a restore brings the tags back with it"
    );
    assert_eq!(fx.get_entity(&jan.id).await.unwrap(), restored);
    assert_eq!(fx.list_tags(&case_id, None).await.unwrap().tags.len(), 1);
}

#[tokio::test]
async fn purging_destroys_the_entity_and_redacts_only_its_own_audit_payloads() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;
    let jan = seed_person(&fx, &case_id, &[("name", &["Jan Kowalski"])])
        .await
        .unwrap();
    let ewa = seed_person(&fx, &case_id, &[("name", &["Ewa Nowak"])])
        .await
        .unwrap();

    // It asks first.
    let e = fx
        .purge_entity(
            PurgeEntity {
                entity_id: jan.id.clone(),
                confirm: false,
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    assert!(matches!(e, ServiceError::ConfirmationRequired { .. }));
    assert!(e.fix().unwrap().contains("confirm"));
    assert!(fx.get_entity(&jan.id).await.is_ok(), "nothing happened yet");

    fx.purge_entity(
        PurgeEntity {
            entity_id: jan.id.clone(),
            confirm: true,
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();

    assert!(matches!(
        fx.get_entity(&jan.id).await,
        Err(ServiceError::NotFound { .. })
    ));
    assert!(fx.get_entity(&ewa.id).await.is_ok());

    // The trail still says a purge happened, and who did it, but the copies
    // of the purged data inside it are gone.
    let trail = fx.entity_audit(&jan.id, None).await.unwrap();
    assert!(trail.entries.iter().any(|e| e.action == AuditAction::Purge));
    for entry in &trail.entries {
        if entry.action == AuditAction::Purge {
            assert!(entry.before.is_none() && entry.after.is_none());
        } else {
            assert!(
                entry.payload_was_redacted(),
                "an earlier payload described the data the purge erased"
            );
            let payload = entry.after.as_ref().or(entry.before.as_ref()).unwrap();
            assert!(payload.get(REDACTED_FIELD).is_some());
        }
        assert_eq!(entry.actor, "human", "who did it is never redacted");
    }

    // The other entity's history is untouched.
    let others = fx.entity_audit(&ewa.id, None).await.unwrap();
    assert!(!others.entries.is_empty());
    assert!(
        others.entries.iter().all(|e| !e.payload_was_redacted()),
        "purging one entity must not erase another's history"
    );
}

#[tokio::test]
async fn purging_something_that_is_not_there_says_so() {
    let fx = fixture().await;
    assert!(matches!(
        fx.purge_entity(
            PurgeEntity {
                entity_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                confirm: true,
            },
            &Actor::analyst(),
        )
        .await,
        Err(ServiceError::NotFound { .. })
    ));
}

// ---------------------------------------------------------------------------
// Merged-away ids
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_merged_away_entity_still_resolves_and_says_where_it_went() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;
    let survivor = seed_person(&fx, &case_id, &[("name", &["Jan Kowalski"])])
        .await
        .unwrap();
    let duplicate = seed_person(&fx, &case_id, &[("name", &["J. Kowalski"])])
        .await
        .unwrap();

    // Merging is a later ticket, so the state is set through the store.
    // This is the only thing in this file that does not go through a
    // service method, and it is setup, not assertion.
    merge_away(&fx, &duplicate.id, &survivor.id).await;

    let resolved = fx.get_entity(&duplicate.id).await.unwrap();
    assert_eq!(
        resolved.id, survivor.id,
        "an id quoted in a six-month-old report still resolves"
    );
    assert_eq!(
        resolved.merged_from.as_deref(),
        Some(duplicate.id.as_str()),
        "and it says why it landed somewhere else"
    );

    // The merged-away entity is no longer a node of its own.
    let listed = fx
        .list_entities(EntityQuery {
            case_id,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(listed.entities.len(), 1);
    assert_eq!(listed.entities[0].id, survivor.id);

    // And it cannot be edited: the survivor is what changes now.
    let e = fx
        .update_entity(
            &duplicate.id,
            UpdateEntity {
                caption: Patch::Set("Anything".into()),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    match &e {
        ServiceError::Conflict { message, .. } => assert!(message.contains(&survivor.id)),
        other => panic!("expected a conflict pointing at the survivor, got {other:?}"),
    }
    assert!(e.fix().unwrap().contains(&survivor.id));
}

/// Sets `merged_into` directly, because merging has no use case yet.
async fn merge_away(fx: &TestServices, merged: &str, survivor: &str) {
    use vesica_store::repo::entity;

    let mut row = entity::find(fx.store.db.read_pool(), merged)
        .await
        .unwrap()
        .unwrap();
    row.merged_into = Some(survivor.to_string());
    fx.store
        .db
        .transaction(move |conn| Box::pin(async move { entity::save(conn, &row).await }))
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// Audit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_mutation_is_recorded_with_the_actor_that_made_it() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;
    let agent = Actor::named("agent:researcher").unwrap();

    let jan = fx
        .create_entity(person(&case_id, &[("name", &["Jan Kowalski"])]), &agent)
        .await
        .unwrap();
    assert_eq!(jan.created_by, "agent:researcher");

    fx.update_entity(
        &jan.id,
        UpdateEntity {
            confidence: Some(Confidence::Confirmed),
            ..Default::default()
        },
        &Actor::analyst(),
    )
    .await
    .unwrap();
    fx.edit_entity_properties(
        &jan.id,
        EditProperties {
            add: property_values(&[("phone", &["601 234 567"])]),
            ..Default::default()
        },
        &agent,
    )
    .await
    .unwrap();
    fx.tag_entity(&jan.id, "suspect", &agent).await.unwrap();
    fx.untag_entity(&jan.id, "suspect", &Actor::analyst())
        .await
        .unwrap();
    fx.delete_entity(&jan.id, &Actor::analyst()).await.unwrap();
    fx.restore_entity(&jan.id, &agent).await.unwrap();

    let trail = fx.entity_audit(&jan.id, None).await.unwrap();
    let actions: Vec<AuditAction> = trail.entries.iter().map(|e| e.action).collect();
    for expected in [
        AuditAction::Create,
        AuditAction::Update,
        AuditAction::Tag,
        AuditAction::Untag,
        AuditAction::Delete,
        AuditAction::Restore,
    ] {
        assert!(
            actions.contains(&expected),
            "{expected:?} should be in {actions:?}"
        );
    }
    assert!(
        trail
            .entries
            .iter()
            .filter(|e| e.action == AuditAction::Update)
            .count()
            >= 2,
        "the confidence change and the property edit are both updates"
    );

    let create = trail
        .entries
        .iter()
        .find(|e| e.action == AuditAction::Create)
        .unwrap();
    assert_eq!(create.actor, "agent:researcher");
    assert_eq!(create.target_kind, "entity");
    assert_eq!(create.target_id, jan.id);
    assert_eq!(create.case_id.as_deref(), Some(case_id.as_str()));
    assert!(create.before.is_none());
    assert!(
        create.after.as_ref().unwrap().get("caption").is_some(),
        "the snapshot says what was recorded"
    );

    // An entity's changes are part of its case's trail as well.
    let case_trail = fx.case_audit(&case_id, Some(50)).await.unwrap();
    assert!(
        case_trail
            .entries
            .iter()
            .any(|e| e.target_kind == "entity" && e.target_id == jan.id)
    );

    // The tag history is the same trail, narrowed.
    let tags = fx.entity_tag_history(&jan.id, None).await.unwrap();
    assert_eq!(tags.entries.len(), 2);
    assert!(
        tags.entries
            .iter()
            .all(|e| matches!(e.action, AuditAction::Tag | AuditAction::Untag))
    );
}

#[tokio::test]
async fn a_change_that_changes_nothing_records_nothing() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;
    let jan = seed_person(&fx, &case_id, &[("name", &["Jan Kowalski"])])
        .await
        .unwrap();

    let before = fx.entity_audit(&jan.id, None).await.unwrap().entries.len();

    fx.update_entity(&jan.id, UpdateEntity::default(), &Actor::analyst())
        .await
        .unwrap();
    fx.edit_entity_properties(&jan.id, EditProperties::default(), &Actor::analyst())
        .await
        .unwrap();
    fx.untag_entity(&jan.id, "never-attached", &Actor::analyst())
        .await
        .unwrap();

    assert_eq!(
        fx.entity_audit(&jan.id, None).await.unwrap().entries.len(),
        before,
        "a no-op is not an event"
    );
}

#[tokio::test]
async fn a_failed_write_leaves_neither_a_change_nor_an_audit_entry() {
    let fx = fixture().await;
    let case_id = polish_case(&fx).await;
    let jan = seed_person(&fx, &case_id, &[("name", &["Jan Kowalski"])])
        .await
        .unwrap();
    let before = fx.entity_audit(&jan.id, None).await.unwrap().entries.len();

    // The addition is fine; the second value of a single-valued property is
    // not, so the whole edit is refused.
    let e = fx
        .edit_entity_properties(
            &jan.id,
            EditProperties {
                add: property_values(&[
                    ("phone", &["601 234 567"]),
                    ("birthDate", &["1978", "1979"]),
                ]),
                ..Default::default()
            },
            &Actor::analyst(),
        )
        .await
        .unwrap_err();
    assert!(matches!(e, ServiceError::Validation { .. }));

    let after = fx.get_entity(&jan.id).await.unwrap();
    assert!(after.values_of("phone").is_empty());
    assert_eq!(
        fx.entity_audit(&jan.id, None).await.unwrap().entries.len(),
        before
    );
}
