//! The apply pass's own tests, one directory down so that the pass and the
//! evidence for it each stay well inside the repository's file-length check.

use super::*;

const DIGEST: &str = "a002fd0595c559505437ce754971d911b703373addf2b59e425ec057d631614f";

#[test]
fn a_retained_object_is_reported_as_a_count_and_nothing_else() {
    let refusal = objects_retained(2);
    assert_eq!(refusal.code, codes::DELETE_OBJECTS_RETAINED);
    assert_eq!(refusal.exit_code(), 4);
    assert!(!refusal.is_retryable());
    let json = serde_json::to_value(&refusal).unwrap();
    assert_eq!(json["details"]["retained_count"], 2);
    assert_eq!(json["details"]["bucket"], "delete");
    assert_eq!(json["details"]["reason"], "unremovable");
    assert_eq!(json["details"].as_object().unwrap().len(), 3);
}

#[test]
fn an_object_that_is_not_a_regular_file_is_left_exactly_as_it_is() {
    let root = tempfile::tempdir().unwrap();
    let path = objects::absolute_path(root.path(), DIGEST);
    let mut warnings = Vec::new();
    assert!(
        !unlink_object(root.path(), DIGEST, &mut warnings),
        "an absent object is never counted as removed"
    );
    fs::create_dir_all(&path).unwrap();
    assert!(!unlink_object(root.path(), DIGEST, &mut warnings));
    assert!(path.is_dir(), "a directory is never removed");
}

#[test]
fn only_the_documents_the_plan_named_are_unlinked() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join(Case::DIRECTORY);
    fs::create_dir_all(&directory).unwrap();
    let present = "0123456789abcdef0123456789abcdef".to_owned();
    let absent = "fedcba9876543210fedcba9876543210".to_owned();
    let path = document_path(&directory, &present);
    fs::write(&path, "{}\n").unwrap();
    fs::write(directory.join("keep.json"), "{}\n").unwrap();
    assert!(
        unlinkable(&path),
        "the probe and the pass look at the one path this helper builds"
    );
    assert_eq!(
        unlink_records::<Case>(root.path(), &[present.clone(), absent]),
        Unlinked {
            removed: 1,
            retained: 0
        },
        "a document that was not there is neither removed nor retained"
    );
    assert!(!path.exists());
    assert!(directory.join("keep.json").exists(), "nothing else goes");
}

/// The merge rule is platform-independent, so the warnings are built
/// here rather than produced by an unlink: only Windows defers one, and
/// the rule that decides which of two survives has to hold everywhere.
#[test]
fn a_repeated_warning_keeps_the_worst_outcome_whatever_the_order() {
    let deferred = |restored: bool| replace_while_open(Some(restored));

    for order in [[true, false], [false, true]] {
        let mut warnings = Vec::new();
        for restored in order {
            note(&mut warnings, deferred(restored));
        }
        assert_eq!(warnings.len(), 1, "one code is reported once");
        assert_eq!(
            warnings[0].details.flag_value(READ_ONLY_RESTORED),
            Some(false),
            "an object left writable is reported whichever object saw it first"
        );
    }

    let mut warnings = Vec::new();
    note(&mut warnings, deferred(true));
    note(&mut warnings, deferred(true));
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].details.flag_value(READ_ONLY_RESTORED),
        Some(true),
        "nothing is worsened by repetition alone"
    );

    let mut warnings = Vec::new();
    note(&mut warnings, deferred(false));
    note(&mut warnings, paths::no_directory_fsync_warning("purge"));
    assert_eq!(warnings.len(), 2, "a different code is its own entry");
    assert_eq!(
        warnings[0].details.flag_value(READ_ONLY_RESTORED),
        Some(false),
        "a flagless warning of another code never displaces one"
    );
    assert_eq!(
        warnings[1].details.flag_value(READ_ONLY_RESTORED),
        None,
        "a warning with no such flag reads as none"
    );
}

/// The flag answers a question that only exists once the attribute has
/// been cleared, so the path that never cleared it must not answer it.
#[test]
fn the_restored_flag_is_absent_where_the_attribute_was_never_cleared() {
    let never = replace_while_open(None);
    assert_eq!(never.code, codes::PLATFORM_REPLACE_WHILE_OPEN);
    assert!(never.is_retryable());
    let json = serde_json::to_value(&never).unwrap();
    assert_eq!(json["details"]["stage"], "purge");
    assert_eq!(
        json["details"].get(READ_ONLY_RESTORED),
        None,
        "no restore is claimed where nothing was cleared"
    );
    assert_eq!(json["details"].as_object().unwrap().len(), 2);
    assert_eq!(never.details.flag_value(READ_ONLY_RESTORED), None);

    let cleared = replace_while_open(Some(true));
    assert_eq!(
        serde_json::to_value(&cleared).unwrap()["details"][READ_ONLY_RESTORED],
        true,
        "the clearing path still answers it"
    );

    // A warning that never cleared the attribute widened nothing, so it
    // is not the weaker outcome and never displaces one that did.
    let mut warnings = Vec::new();
    note(&mut warnings, replace_while_open(Some(false)));
    note(&mut warnings, replace_while_open(None));
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].details.flag_value(READ_ONLY_RESTORED),
        Some(false),
        "the object left writable is still the one reported"
    );

    // A flagless copy answers less than one that cleared the attribute
    // and put it back, so a later copy that did replaces it. It is still
    // never allowed to displace the object that was left writable.
    for later in [Some(true), Some(false)] {
        let mut warnings = Vec::new();
        note(&mut warnings, replace_while_open(None));
        note(&mut warnings, replace_while_open(later));
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].details.flag_value(READ_ONLY_RESTORED),
            later,
            "the copy that answers the question is the one kept"
        );
    }
    assert_eq!(fidelity(&paths::no_directory_fsync_warning("purge")), 0);
}

/// The probe is the whole of the all-or-nothing rule, so each of its
/// answers is asserted directly: a directory that cannot have an entry
/// removed from it, a path holding something openPapir did not write,
/// and a document that is simply not there any more.
#[test]
fn a_record_that_cannot_be_unlinked_refuses_the_pass_before_it_starts() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join(Case::DIRECTORY);
    fs::create_dir_all(&directory).unwrap();
    let case = "0123456789abcdef0123456789abcdef".to_owned();
    fs::write(directory.join(format!("{case}.json")), "{}\n").unwrap();
    let plan = Plan {
        case: case.clone(),
        ..Plan::default()
    };
    assert!(removable(root.path(), &plan), "an ordinary case may go");

    assert!(
        unlinkable(&directory.join("absent.json")),
        "a document that is already gone is not a refusal"
    );
    assert!(
        !unlinkable(&directory),
        "a directory at a record's path is never unlinked"
    );
    assert!(
        !directory_writable(&root.path().join("records/nowhere")),
        "a directory that is not there cannot have an entry removed"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        let mut warnings = Vec::new();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
        assert!(
            !directory_writable(&directory),
            "search alone is not enough"
        );
        assert!(!removable(root.path(), &plan));
        let removed = run(root.path(), &plan, &mut warnings);
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(removed.records_retained, 1);
        assert_eq!(removed.records(), 0);
        assert!(
            warnings.is_empty(),
            "nothing was touched, so nothing synced"
        );
        assert!(
            directory.join(format!("{case}.json")).is_file(),
            "the document the deletion could not finish is still there"
        );
    }
}

/// An import event is a document the plan would remove, so a deletion
/// that never reached the object pass has to count it as retained.
#[test]
fn a_planned_import_event_counts_as_a_document_the_deletion_planned() {
    let mut plan = Plan {
        case: "0123456789abcdef0123456789abcdef".to_owned(),
        submissions: vec!["a".to_owned(), "b".to_owned()],
        ..Plan::default()
    };
    assert_eq!(planned_documents(&plan), 3, "the case and its submissions");
    plan.import_events
        .insert(DIGEST.to_owned(), vec!["c".to_owned(), "d".to_owned()]);
    assert_eq!(planned_documents(&plan), 5, "and the events going with it");
}

/// The count `delete.records_retained` carries is every document the
/// deletion planned to remove and did not, so a refusal that names the
/// import directory still reports the events it never reached.
#[cfg(unix)]
#[test]
fn a_refused_import_directory_is_reported_in_the_retained_count() {
    use std::os::unix::fs::PermissionsExt as _;

    let root = tempfile::tempdir().unwrap();
    let cases = root.path().join(Case::DIRECTORY);
    let imports = root.path().join(ImportEvent::DIRECTORY);
    fs::create_dir_all(&cases).unwrap();
    fs::create_dir_all(&imports).unwrap();
    let case = "0123456789abcdef0123456789abcdef".to_owned();
    let event = "fedcba9876543210fedcba9876543210".to_owned();
    fs::write(document_path(&cases, &case), "{}\n").unwrap();
    fs::write(document_path(&imports, &event), "{}\n").unwrap();
    let mut plan = Plan {
        case,
        objects: vec![DIGEST.to_owned()],
        ..Plan::default()
    };
    plan.import_events
        .insert(DIGEST.to_owned(), vec![event.clone()]);

    let mut warnings = Vec::new();
    fs::set_permissions(&imports, fs::Permissions::from_mode(0o500)).unwrap();
    let removed = run(root.path(), &plan, &mut warnings);
    fs::set_permissions(&imports, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(removed.records(), 0, "the probe refused before the pass");
    assert_eq!(
        removed.records_retained, 2,
        "the case and the import event it never reached"
    );
    assert_eq!(removed.objects, 0, "and no object was touched");
    assert!(document_path(&imports, &event).is_file());
}

#[test]
fn a_removed_count_totals_every_record_kind() {
    let removed = Removed {
        associations: 1,
        cases: 1,
        import_events: 2,
        receipts: 1,
        submissions: 3,
        objects: 4,
        records_retained: 0,
        unremovable: 0,
    };
    assert_eq!(removed.records(), 8);
    assert_eq!(Removed::default().records(), 0);
}
