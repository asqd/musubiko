//! The journal keeps every operation it cannot read and never overwrites a broken file.

mod common;

use common::with_origin;
use musubiko_core::{Journal, OpStatus, Operation, Step};
use tempfile::TempDir;

fn op(f: &common::Fixture, title: &str) -> Operation {
    Operation::new(title, &f.repo(), vec![Step::new("x", &["status"])])
}

#[test]
fn keeps_operations_newest_first_and_replaces_by_id() {
    let f = with_origin();
    let dir = TempDir::new().unwrap();
    let j = Journal::new(dir.path().join("journal.json"));
    let a = op(&f, "a");
    let mut b = op(&f, "b");
    assert_eq!(j.upsert(&a).unwrap(), None);
    j.upsert(&b).unwrap();
    b.status = OpStatus::Done;
    j.upsert(&b).unwrap();
    let titles: Vec<String> = j
        .list(&f.repo().root().to_path_buf())
        .into_iter()
        .map(|o| o.title)
        .collect();
    assert_eq!(titles, ["b", "a"]);
    assert_eq!(j.get(&b.id).unwrap().status, OpStatus::Done);
}

#[test]
fn keeps_entries_it_cannot_read() {
    let f = with_origin();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("journal.json");
    // An operation from a newer version: an effect this one does not know.
    let mut newer = serde_json::to_value(op(&f, "newer")).unwrap();
    newer["steps"][0]["effect"] = serde_json::json!({ "kind": "somethingNew", "x": 1 });
    newer["id"] = "newer-1".into();
    std::fs::write(&path, serde_json::to_vec(&vec![newer]).unwrap()).unwrap();

    let j = Journal::new(&path);
    assert!(j.list(f.repo().root()).is_empty(), "not readable here");
    j.upsert(&op(&f, "mine")).unwrap();
    let raw: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let ids: Vec<&str> = raw.iter().filter_map(|e| e["id"].as_str()).collect();
    assert_eq!(ids.len(), 2);
    assert!(
        ids.contains(&"newer-1"),
        "the newer entry is still in the file"
    );
    assert_eq!(j.list(f.repo().root()).len(), 1);
}

#[test]
fn sets_a_broken_file_aside_instead_of_overwriting_it() {
    let f = with_origin();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("journal.json");
    std::fs::write(&path, b"{ this is not a journal").unwrap();

    let j = Journal::new(&path);
    assert!(j.list(f.repo().root()).is_empty());
    assert!(path.exists(), "reading leaves it alone");
    let aside = j.upsert(&op(&f, "after")).unwrap().expect("set aside");
    assert_eq!(std::fs::read(&aside).unwrap(), b"{ this is not a journal");
    assert!(
        aside
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("journal.broken-")
    );
    assert_eq!(j.list(f.repo().root()).len(), 1);
}
