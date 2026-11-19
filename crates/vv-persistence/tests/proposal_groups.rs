//! Reviewing tag suggestions across the library (STATE.md F7.3 follow-up,
//! ADR-0038): pending suggestions grouped by value, against a real
//! database file (migration 0016).

use rusqlite::Connection;
use vv_persistence::library::organize;
use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::*;
use vv_persistence::proposals;
use vv_persistence::{Database, OpenOptions};

fn item(c: &Connection, title: &str, n: i64) -> String {
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, n);
    it.canonical_url = Some(format!("https://example.com/v/{n}"));
    it.title = Some(title.into());
    repo::insert_item(c, &it).unwrap();
    it.id
}

fn run(c: &Connection, id: &str, values: &[&str]) -> Vec<proposals::Proposal> {
    let v: Vec<String> = values.iter().map(|s| (*s).to_owned()).collect();
    proposals::save_run(
        c,
        id,
        "qwen3.5:4b",
        "http://localhost:11434/v1",
        &"0".repeat(32),
        &v,
        1_794_000_000_000,
    )
    .unwrap()
}

#[test]
fn groups_join_spellings_count_items_and_skip_decided_and_removed() {
    let d = tempfile::tempdir().unwrap();
    let mut db = Database::open(&d.path().join("l.sqlite"), &OpenOptions::default()).unwrap();
    let c = db.writer();
    let a = item(c, "Harrier", 1);
    let b = item(c, "Kadıköy vapuru", 2);
    let x = item(c, "F-16", 3);
    let gone = item(c, "Removed", 4);
    run(c, &a, &["Fighter jet", "Marine Corps"]);
    run(c, &b, &["İstanbul", "vapur"]);
    let px = run(c, &x, &["fighter jet", "istanbul", "uçak"]);
    run(c, &gone, &["fighter jet"]);
    c.execute(
        "UPDATE library_item SET removed_at = 5 WHERE id = ?1",
        [&gone],
    )
    .unwrap();
    // A decided suggestion is not in the review any more.
    let rejected = px.iter().find(|p| p.value == "uçak").unwrap();
    proposals::decide(c, &rejected.id, false, 6).unwrap();
    // The library already has "Vapur" as a tag (another item has it).
    let (tag, _) = organize::get_or_create_tag(c, "Vapur", 7).unwrap();
    organize::tag_items(c, &tag.id, std::slice::from_ref(&a), 7).unwrap();

    let g = proposals::pending_groups(c).unwrap();
    let names: Vec<(&str, usize)> = g
        .iter()
        .map(|g| (g.value.as_str(), g.items.len()))
        .collect();
    // Most items first, then by name; "İstanbul" and "istanbul" are one value,
    // and on a tie the newest item's spelling names the group.
    assert_eq!(
        names,
        vec![
            ("fighter jet", 2),
            ("istanbul", 2),
            ("Marine Corps", 1),
            ("Vapur", 1)
        ],
        "{g:#?}"
    );
    // Newest item first; the removed item is left out.
    assert_eq!(g[0].items[0].item_id, x);
    assert_eq!(g[0].items[1].item_id, a);
    assert_eq!(g[0].items[1].title.as_deref(), Some("Harrier"));
    // An existing tag gives the group its name.
    assert!(g[3].tag_exists && !g[0].tag_exists);
}

#[test]
fn an_empty_library_has_no_groups() {
    let d = tempfile::tempdir().unwrap();
    let mut db = Database::open(&d.path().join("l.sqlite"), &OpenOptions::default()).unwrap();
    assert!(proposals::pending_groups(db.writer()).unwrap().is_empty());
}
