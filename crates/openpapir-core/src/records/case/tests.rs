//! Unit tests for the case record, its filters, and its one rewrite.

use super::*;
use crate::archive;
use crate::error::codes;
use crate::records::submission;
use std::fs;

fn archive_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    archive::init(root.path()).unwrap();
    root
}

fn tags(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn stored_document(root: &Path, id: &str) -> String {
    fs::read_to_string(root.join(CASES_DIR).join(format!("{id}.json"))).unwrap()
}

#[test]
fn a_created_case_is_listed_and_shown_again() {
    let root = archive_root();
    let created = create(root.path(), "Tax matter", Some("First contact.")).unwrap();
    let case = created.data.case;
    assert_eq!(case.title, "Tax matter");
    assert_eq!(case.notes.as_deref(), Some("First contact."));
    assert_eq!(case.record_kind, KIND);
    assert_eq!(case.archive_schema_version, 1);
    assert_eq!(case.id.len(), 32);
    assert!(case.created_at.ends_with('Z'));
    assert_eq!(case.status, Status::Open, "a new case is open");
    assert!(case.tags.is_empty());
    assert_eq!(case.updated_at, None, "nothing has been updated yet");

    let listed = list(root.path(), &Filter::default()).unwrap().data;
    assert_eq!(listed.count, 1);
    assert_eq!(listed.cases[0], case);

    let shown = show(root.path(), &case.id).unwrap().data;
    assert_eq!(shown.case, case);
    assert_eq!(shown.submission_count, 0);
    assert!(shown.submissions.is_empty());
}

#[test]
fn a_case_without_notes_omits_the_field_entirely() {
    let root = archive_root();
    let case = create(root.path(), "Plain title", None).unwrap().data.case;
    assert_eq!(case.notes, None);
    let stored = stored_document(root.path(), &case.id);
    assert!(!stored.contains("notes"));
    assert!(!stored.contains("updated_at"), "no update has happened");
    assert!(stored.contains("\"status\":\"open\""));
    assert!(stored.contains("\"tags\":[]"));
    assert!(stored.ends_with("}\n"), "one LF-terminated document");
    let keys: Vec<String> = serde_json::from_str::<serde_json::Value>(&stored)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "keys are stored sorted");
}

#[test]
fn a_record_written_before_the_new_fields_reads_as_open_and_untagged() {
    let root = archive_root();
    let case = create(root.path(), "Older", None).unwrap().data.case;
    let earlier = serde_json::json!({
        "archive_schema_version": 1,
        "created_at": case.created_at,
        "id": case.id,
        "record_kind": "case",
        "title": "Older",
    });
    fs::write(
        root.path()
            .join(CASES_DIR)
            .join(format!("{}.json", case.id)),
        format!("{earlier}\n"),
    )
    .unwrap();
    let read = show(root.path(), &case.id).unwrap().data.case;
    assert_eq!(read.status, Status::Open);
    assert!(read.tags.is_empty());
    assert_eq!(read.updated_at, None);
}

#[test]
fn an_empty_archive_lists_no_case() {
    let root = archive_root();
    let listed = list(root.path(), &Filter::default()).unwrap().data;
    assert_eq!(listed.count, 0);
    assert!(listed.cases.is_empty());
}

#[test]
fn an_unknown_case_is_refused_without_echoing_the_reference() {
    let root = archive_root();
    let refusal = show(root.path(), "0123456789abcdef0123456789abcdef")
        .unwrap_err()
        .error;
    assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
    assert_eq!(refusal.exit_code(), 4);
    let json = serde_json::to_value(&refusal).unwrap();
    assert_eq!(json["details"]["record_kind"], "case");
    assert_eq!(json["details"]["reference_kind"], "case_id");
    assert_eq!(json["details"].as_object().unwrap().len(), 3);
}

#[test]
fn a_field_that_breaks_its_cap_is_refused_before_any_write() {
    let root = archive_root();
    let refusal = create(root.path(), &"t".repeat(201), None)
        .unwrap_err()
        .error;
    assert_eq!(refusal.code, codes::INPUT_CAP_FIELD_LENGTH);
    let refusal = create(root.path(), "Title", Some(&"n".repeat(4097)))
        .unwrap_err()
        .error;
    assert_eq!(refusal.code, codes::INPUT_CAP_FIELD_LENGTH);
    assert_eq!(
        fs::read_dir(root.path().join(CASES_DIR)).unwrap().count(),
        0,
        "no record survives a refused field"
    );
}

#[test]
fn creating_a_case_needs_the_writer_lock() {
    let root = archive_root();
    let _held = WriterLock::acquire(root.path()).unwrap();
    let refusal = create(root.path(), "Held", None).unwrap_err().error;
    assert_eq!(refusal.code, codes::LOCK_HELD);
    assert!(refusal.is_retryable());
    assert!(
        list(root.path(), &Filter::default()).is_ok(),
        "listing needs no lock at all"
    );
}

#[test]
fn a_malformed_case_document_is_reported_rather_than_skipped() {
    let root = archive_root();
    let case = create(root.path(), "Readable", None).unwrap().data.case;
    fs::write(
        root.path()
            .join(CASES_DIR)
            .join("ffffffffffffffffffffffffffffffff.json"),
        b"{ not a record",
    )
    .unwrap();
    let refusal = list(root.path(), &Filter::default()).unwrap_err().error;
    assert_eq!(refusal.code, codes::RECORD_MALFORMED);
    assert_eq!(
        serde_json::to_value(&refusal).unwrap()["details"]["path_count"],
        1
    );
    assert_eq!(
        show(root.path(), &case.id).unwrap().data.case.title,
        "Readable",
        "one unreadable document does not stop reading another by name"
    );
}

#[test]
fn a_case_shows_its_own_submissions_and_no_others() {
    let root = archive_root();
    let first = create(root.path(), "First", None).unwrap().data.case;
    let second = create(root.path(), "Second", None).unwrap().data.case;
    submission::add(root.path(), &first.id, "One", None, &[]).unwrap();
    submission::add(root.path(), &second.id, "Two", None, &[]).unwrap();
    let shown = show(root.path(), &first.id).unwrap().data;
    assert_eq!(shown.submission_count, 1);
    assert_eq!(shown.submissions[0].description, "One");
    assert_eq!(shown.submissions[0].case_id, first.id);
}

#[test]
fn tags_are_stored_sorted_and_deduplicated() {
    let root = archive_root();
    let case = create_with(
        root.path(),
        "Tagged",
        None,
        Status::Closed,
        &tags(&["tax", "appeal", "tax"]),
    )
    .unwrap()
    .data
    .case;
    assert_eq!(case.tags, tags(&["appeal", "tax"]));
    assert_eq!(case.status, Status::Closed);
    let stored = stored_document(root.path(), &case.id);
    assert!(stored.contains("\"status\":\"closed\""));
    assert!(stored.contains("\"tags\":[\"appeal\",\"tax\"]"));
}

#[test]
fn a_tag_that_breaks_a_cap_or_its_shape_is_refused_and_never_echoed() {
    let root = archive_root();
    let long = "t".repeat(65);
    let refusal = create_with(root.path(), "T", None, Status::Open, &tags(&[&long]))
        .unwrap_err()
        .error;
    assert_eq!(refusal.code, codes::INPUT_CAP_TAG_LENGTH);
    assert_eq!(refusal.exit_code(), 3);
    let json = serde_json::to_value(&refusal).unwrap();
    assert_eq!(json["details"]["cap_bytes"], 64);
    assert_eq!(json["details"]["input_index"], 0);
    assert!(!serde_json::to_string(&refusal).unwrap().contains(&long));

    let many: Vec<String> = (0..33).map(|index| format!("tag{index}")).collect();
    let refusal = create_with(root.path(), "T", None, Status::Open, &many)
        .unwrap_err()
        .error;
    assert_eq!(refusal.code, codes::INPUT_CAP_TAG_COUNT);
    assert_eq!(
        serde_json::to_value(&refusal).unwrap()["details"]["cap_count"],
        32
    );

    for bad in [" ", "with\tcontrol"] {
        let refusal = create_with(root.path(), "T", None, Status::Open, &tags(&[bad]))
            .unwrap_err()
            .error;
        assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
        assert_eq!(
            serde_json::to_value(&refusal).unwrap()["details"]["argument"],
            "tag"
        );
    }

    let repeated = tags(&["same"; 40]);
    assert!(
        create_with(root.path(), "T", None, Status::Open, &repeated).is_ok(),
        "a repeated tag never spends part of the count cap"
    );
}

#[test]
fn a_status_is_one_of_two_stable_names() {
    assert_eq!(Status::parse("open").unwrap(), Status::Open);
    assert_eq!(Status::parse("closed").unwrap(), Status::Closed);
    assert_eq!(Status::Open.as_str(), "open");
    assert_eq!(Status::Closed.as_str(), "closed");
    let refusal = Status::parse("OPEN").unwrap_err();
    assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
    assert_eq!(refusal.exit_code(), 2);
    assert_eq!(
        serde_json::to_value(&refusal).unwrap()["details"]["argument"],
        "status"
    );
}

#[test]
fn an_update_rewrites_the_one_record_keeping_its_identity() {
    let root = archive_root();
    let case = create_with(
        root.path(),
        "Tax matter",
        Some("First contact."),
        Status::Open,
        &tags(&["tax"]),
    )
    .unwrap()
    .data
    .case;
    let updated = update(
        root.path(),
        &case.id,
        &Change {
            title: Some("Tax appeal"),
            status: Some(Status::Closed),
            add_tags: &tags(&["appeal"]),
            remove_tags: &tags(&["tax"]),
            notes: NotesChange::Clear,
        },
    )
    .unwrap()
    .data;
    assert_eq!(updated.case.id, case.id, "the identifier never moves");
    assert_eq!(
        updated.case.created_at, case.created_at,
        "the creation time never moves"
    );
    assert_eq!(updated.case.title, "Tax appeal");
    assert_eq!(updated.case.notes, None);
    assert_eq!(updated.case.status, Status::Closed);
    assert_eq!(updated.case.tags, tags(&["appeal"]));
    assert!(updated.case.updated_at.is_some());
    assert_eq!(
        updated.changed,
        vec![
            "notes".to_owned(),
            "status".to_owned(),
            "tags".to_owned(),
            "title".to_owned()
        ],
        "the field names only, sorted"
    );

    let listed = list(root.path(), &Filter::default()).unwrap().data;
    assert_eq!(listed.count, 1, "the rewrite left one record, not two");
    assert_eq!(listed.cases[0], updated.case);
}

#[test]
fn an_update_that_changes_nothing_is_refused_without_a_write() {
    let root = archive_root();
    let case = create_with(root.path(), "Same", None, Status::Open, &tags(&["tax"]))
        .unwrap()
        .data
        .case;
    let before = stored_document(root.path(), &case.id);

    let refusal = update(root.path(), &case.id, &Change::default())
        .unwrap_err()
        .error;
    assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
    assert_eq!(refusal.exit_code(), 2);
    assert_eq!(
        serde_json::to_value(&refusal).unwrap()["details"]["argument"],
        "update"
    );

    let refusal = update(
        root.path(),
        &case.id,
        &Change {
            title: Some("Same"),
            status: Some(Status::Open),
            add_tags: &tags(&["tax"]),
            remove_tags: &tags(&["absent"]),
            notes: NotesChange::Clear,
        },
    )
    .unwrap_err()
    .error;
    assert_eq!(refusal.code, codes::USAGE_ARGUMENTS);
    assert_eq!(
        stored_document(root.path(), &case.id),
        before,
        "a refused update leaves the document byte for byte as it was"
    );
}

#[test]
fn an_update_refuses_a_broken_field_before_it_takes_the_lock() {
    let root = archive_root();
    let case = create(root.path(), "Held", None).unwrap().data.case;
    let _held = WriterLock::acquire(root.path()).unwrap();
    let long = "t".repeat(201);
    let refusal = update(
        root.path(),
        &case.id,
        &Change {
            title: Some(&long),
            ..Change::default()
        },
    )
    .unwrap_err()
    .error;
    assert_eq!(
        refusal.code,
        codes::INPUT_CAP_FIELD_LENGTH,
        "the cap is checked before the lock is even asked for"
    );
}

#[test]
fn an_update_of_an_unknown_case_is_refused_without_echoing_it() {
    let root = archive_root();
    let refusal = update(
        root.path(),
        "0123456789abcdef0123456789abcdef",
        &Change {
            status: Some(Status::Closed),
            ..Change::default()
        },
    )
    .unwrap_err()
    .error;
    assert_eq!(refusal.code, codes::RECORD_NOT_FOUND);
    assert_eq!(refusal.exit_code(), 4);
}

#[test]
fn an_update_needs_the_writer_lock() {
    let root = archive_root();
    let case = create(root.path(), "Held", None).unwrap().data.case;
    let _held = WriterLock::acquire(root.path()).unwrap();
    let refusal = update(
        root.path(),
        &case.id,
        &Change {
            status: Some(Status::Closed),
            ..Change::default()
        },
    )
    .unwrap_err()
    .error;
    assert_eq!(refusal.code, codes::LOCK_HELD);
    assert!(refusal.is_retryable());
}

#[test]
fn an_update_may_not_push_the_tags_past_the_count_cap() {
    let root = archive_root();
    let first: Vec<String> = (0..32).map(|index| format!("tag{index}")).collect();
    let case = create_with(root.path(), "Full", None, Status::Open, &first)
        .unwrap()
        .data
        .case;
    let refusal = update(
        root.path(),
        &case.id,
        &Change {
            add_tags: &tags(&["one-too-many"]),
            ..Change::default()
        },
    )
    .unwrap_err()
    .error;
    assert_eq!(refusal.code, codes::INPUT_CAP_TAG_COUNT);
    assert_eq!(
        show(root.path(), &case.id).unwrap().data.case.tags.len(),
        32,
        "the refused update wrote nothing"
    );
}

#[test]
fn a_filter_keeps_only_the_cases_that_match_every_supplied_part() {
    let root = archive_root();
    create_with(
        root.path(),
        "Tax matter",
        Some("The office wrote back."),
        Status::Open,
        &tags(&["tax", "urgent"]),
    )
    .unwrap();
    create_with(
        root.path(),
        "Parking appeal",
        None,
        Status::Closed,
        &tags(&["tax"]),
    )
    .unwrap();

    let all = list(root.path(), &Filter::default()).unwrap().data;
    assert_eq!(all.count, 2);

    let open = list(
        root.path(),
        &Filter {
            status: Some(Status::Open),
            ..Filter::default()
        },
    )
    .unwrap()
    .data;
    assert_eq!(open.count, 1);
    assert_eq!(open.cases[0].title, "Tax matter");

    let both_tags = tags(&["tax", "urgent"]);
    let tagged = list(
        root.path(),
        &Filter {
            tags: &both_tags,
            ..Filter::default()
        },
    )
    .unwrap()
    .data;
    assert_eq!(tagged.count, 1, "every supplied tag must match");

    let one_tag = tags(&["tax"]);
    let tagged = list(
        root.path(),
        &Filter {
            tags: &one_tag,
            ..Filter::default()
        },
    )
    .unwrap()
    .data;
    assert_eq!(tagged.count, 2);

    let queried = list(
        root.path(),
        &Filter {
            query: Some("OFFICE"),
            ..Filter::default()
        },
    )
    .unwrap()
    .data;
    assert_eq!(queried.count, 1, "the query reads the notes as well");
    assert_eq!(queried.cases[0].title, "Tax matter");

    let queried = list(
        root.path(),
        &Filter {
            query: Some("parking"),
            ..Filter::default()
        },
    )
    .unwrap()
    .data;
    assert_eq!(queried.count, 1, "and the title, without regard to case");

    let together = list(
        root.path(),
        &Filter {
            status: Some(Status::Closed),
            tags: &one_tag,
            query: Some("appeal"),
        },
    )
    .unwrap()
    .data;
    assert_eq!(together.count, 1);

    let none = list(
        root.path(),
        &Filter {
            status: Some(Status::Closed),
            query: Some("tax matter"),
            ..Filter::default()
        },
    )
    .unwrap()
    .data;
    assert_eq!(none.count, 0, "the parts are combined, not alternated");
}

#[test]
fn a_listing_keeps_its_order_under_a_filter() {
    let root = archive_root();
    for index in 0..5 {
        create_with(
            root.path(),
            &format!("Matter {index}"),
            None,
            Status::Open,
            &tags(&["keep"]),
        )
        .unwrap();
    }
    let keep = tags(&["keep"]);
    let filtered = list(
        root.path(),
        &Filter {
            tags: &keep,
            ..Filter::default()
        },
    )
    .unwrap()
    .data;
    let identifiers: Vec<&str> = filtered.cases.iter().map(|case| case.id.as_str()).collect();
    let mut sorted = identifiers.clone();
    sorted.sort_unstable();
    assert_eq!(identifiers, sorted, "the order is the identifier order");
    assert_eq!(filtered.count, 5);
}
