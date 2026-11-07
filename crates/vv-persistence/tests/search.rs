//! STATE.md F1.6: lexical search (SPEC §7.4 SEA-01…05, §16.1). Covers the
//! Turkish search fixture (§25.2), AC-05 and AC-31.

use serde::Deserialize;
use std::collections::BTreeSet;
use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::search::*;
use vv_persistence::library::*;
use vv_persistence::{Database, OpenOptions};

fn open(dir: &std::path::Path) -> Database {
    Database::open(&dir.join("library.sqlite"), &OpenOptions::default()).unwrap()
}

fn item(c: &rusqlite::Connection, url: &str, title: &str, added_at: i64) -> LibraryItem {
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Generic, added_at);
    it.canonical_url = Some(url.to_owned());
    it.title = Some(title.to_owned());
    repo::insert_item(c, &it).unwrap();
    it
}

fn q(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        ..Default::default()
    }
}

#[derive(Deserialize)]
struct Fx {
    version: u32,
    docs: Vec<Doc>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Doc {
    key: String,
    title: String,
    #[serde(default)]
    notes: String,
}
#[derive(Deserialize)]
struct Case {
    query: String,
    expect: Vec<String>,
    status: String,
    #[serde(default = "yes")]
    prefix: bool,
}
fn yes() -> bool {
    true
}

#[test]
fn turkish_search_fixture() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/search-tr.json"
    ))
    .unwrap();
    let fx: Fx = serde_json::from_str(&raw).unwrap();
    assert_eq!(fx.version, 1);
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let mut keys = std::collections::HashMap::new();
    for (n, doc) in fx.docs.iter().enumerate() {
        let mut it = item(
            c,
            &format!("https://example.com/{}", doc.key),
            &doc.title,
            n as i64,
        );
        if !doc.notes.is_empty() {
            it = repo::set_notes(c, &it.id, 1, &doc.notes, n as i64).unwrap();
        }
        keys.insert(it.id.clone(), doc.key.clone());
    }
    let mut failures = Vec::new();
    for case in &fx.cases {
        assert!(["match", "no_match", "limitation"].contains(&case.status.as_str()));
        let page = search(
            c,
            &SearchQuery {
                text: case.query.clone(),
                prefix_last: case.prefix,
                ..Default::default()
            },
        )
        .unwrap();
        let got: BTreeSet<&str> = page
            .hits
            .iter()
            .map(|h| keys[&h.item.id].as_str())
            .collect();
        let want: BTreeSet<&str> = case.expect.iter().map(String::as_str).collect();
        if got != want {
            failures.push(format!("{:?}: got {got:?}, want {want:?}", case.query));
        }
        for h in &page.hits {
            let s = h.snippet.as_ref().expect("a text hit has a snippet");
            assert!(s.parts.iter().any(|p| p.highlight), "{:?}", case.query);
            let doc = fx.docs.iter().find(|d| d.key == keys[&h.item.id]).unwrap();
            let source = if s.field == Field::Notes {
                &doc.notes
            } else {
                &doc.title
            };
            assert!(
                source.contains(&s.text()),
                "snippet {:?} is a real substring of {source:?}",
                s.text()
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn ac05_edits_survive_restart_and_are_searchable() {
    let d = tempfile::tempdir().unwrap();
    let id = {
        let mut db = open(d.path());
        let c = db.writer();
        let it = item(c, "https://example.com/talk", "Konferans kaydı", 1);
        repo::set_notes(c, &it.id, 1, "replikasyon gecikmesi 42. dakikada", 2).unwrap();
        // SEA-05: visible right after the write, before any restart.
        assert_eq!(search(c, &q("gecikmesi")).unwrap().hits.len(), 1);
        it.id
    };
    let db = open(d.path());
    let r = db.reader().unwrap();
    let page = search(&r, &q("replikasyon")).unwrap();
    assert_eq!(page.total, 1);
    let h = &page.hits[0];
    assert_eq!(h.item.id, id);
    assert_eq!(h.item.user_notes, "replikasyon gecikmesi 42. dakikada");
    assert_eq!(h.matched, vec![Field::Notes]);
    assert_eq!(
        h.snippet.as_ref().unwrap().parts[0],
        SnippetPart {
            text: "replikasyon".into(),
            highlight: true
        }
    );
}

#[test]
fn ac31_broken_fts_syntax_never_fails() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    item(c, "https://example.com/a", "Rust AND SQLite", 1);
    item(c, "https://example.com/b", "near the end", 2);
    for text in [
        "'",
        "\"",
        "\"\"",
        "AND",
        "rust AND",
        "OR OR",
        "NOT",
        "NEAR(",
        "near(a b",
        "title:rust",
        "^rust",
        "rust*",
        "*",
        "(",
        ")",
        "a\"b",
        "'; DROP TABLE library_item; --",
        "\u{1}\u{2}",
        "-rust",
        "{title}: x",
        "🙂",
    ] {
        let page = search(c, &q(text)).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        if build_match(text, true).is_none() {
            assert_eq!(
                page.notice,
                Some(QueryNotice::NoSearchableTerms),
                "{text:?}"
            );
            assert_eq!(
                page.total, 2,
                "falls back to the filtered library: {text:?}"
            );
        }
    }
    // Operators are words.
    assert_eq!(search(c, &q("rust AND")).unwrap().total, 1);
    assert_eq!(search(c, &q("near")).unwrap().total, 1);
    assert_eq!(repo::count_items(c).unwrap(), 2);
}

#[test]
fn empty_query_is_the_filtered_library_with_sorting_and_pages() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let a = item(c, "https://example.com/a", "Banana", 10);
    let mut b = item(c, "https://example.com/b", "apple", 30);
    let cc = item(c, "https://example.com/c", "Cherry", 20);
    c.execute(
        "UPDATE library_item SET duration_ms = 5000, source_platform = 'youtube' WHERE id = ?1",
        [&a.id],
    )
    .unwrap();
    b = repo::set_favorite(c, &b.id, b.revision, true, 31).unwrap();
    let ids = |p: SearchPage| p.hits.into_iter().map(|h| h.item.id).collect::<Vec<_>>();

    // Default: newest first.
    let page = search(c, &q("")).unwrap();
    assert_eq!((page.total, page.notice), (3, None));
    assert_eq!(ids(page), vec![b.id.clone(), cc.id.clone(), a.id.clone()]);
    // Title, case-insensitive ascending.
    let s = SearchQuery {
        sort: Sort {
            field: SortField::Title,
            descending: false,
        },
        ..q("")
    };
    assert_eq!(
        ids(search(c, &s).unwrap()),
        vec![b.id.clone(), a.id.clone(), cc.id.clone()]
    );
    // Unknown durations go last in both directions.
    for descending in [true, false] {
        let s = SearchQuery {
            sort: Sort {
                field: SortField::Duration,
                descending,
            },
            ..q("")
        };
        assert_eq!(ids(search(c, &s).unwrap())[0], a.id);
    }
    // Filters combine with text (SEA-02).
    let f = SearchQuery {
        filters: Filters {
            favorite_only: true,
            ..Default::default()
        },
        ..q("")
    };
    assert_eq!(ids(search(c, &f).unwrap()), vec![b.id.clone()]);
    let f = SearchQuery {
        filters: Filters {
            platforms: vec![SourcePlatform::Youtube],
            ..Default::default()
        },
        ..q("bana")
    };
    assert_eq!(ids(search(c, &f).unwrap()), vec![a.id.clone()]);
    // Pages are stable and complete.
    let mut seen = Vec::new();
    for offset in 0..3 {
        let p = search(
            c,
            &SearchQuery {
                limit: 1,
                offset,
                ..q("")
            },
        )
        .unwrap();
        assert_eq!(p.total, 3);
        seen.extend(ids(p));
    }
    assert_eq!(seen, vec![b.id.clone(), cc.id.clone(), a.id.clone()]);
    // Archived and removed items leave normal results.
    repo::set_removed(c, &cc.id, cc.revision, Some(40), 40).unwrap();
    c.execute(
        "UPDATE library_item SET archived_at = 41 WHERE id = ?1",
        [&a.id],
    )
    .unwrap();
    assert_eq!(ids(search(c, &q("")).unwrap()), vec![b.id.clone()]);
    let arch = SearchQuery {
        filters: Filters {
            archived: ArchiveFilter::Only,
            ..Default::default()
        },
        ..q("")
    };
    assert_eq!(ids(search(c, &arch).unwrap()), vec![a.id.clone()]);
}

#[test]
fn title_outranks_notes_and_ties_break_by_newest() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    let in_notes = item(c, "https://example.com/n", "Başka bir şey", 1);
    repo::set_notes(c, &in_notes.id, 1, "deniz kenarında", 2).unwrap();
    let in_title = item(c, "https://example.com/t", "Deniz", 3);
    let page = search(
        c,
        &SearchQuery {
            sort: Sort {
                field: SortField::Relevance,
                descending: false,
            },
            ..q("deniz")
        },
    )
    .unwrap();
    assert_eq!(
        page.hits
            .iter()
            .map(|h| h.item.id.as_str())
            .collect::<Vec<_>>(),
        vec![in_title.id.as_str(), in_notes.id.as_str()]
    );
    assert_eq!(page.hits[0].matched, vec![Field::Title]);
}

#[test]
fn index_can_be_rebuilt_from_the_rows() {
    let d = tempfile::tempdir().unwrap();
    let mut db = open(d.path());
    let c = db.writer();
    item(c, "https://example.com/a", "Işıklı gece", 1);
    c.execute("DELETE FROM search_document", []).unwrap();
    assert_eq!(search(c, &q("isikli")).unwrap().total, 0);
    assert_eq!(repo::rebuild_search_index(c).unwrap(), 1);
    assert_eq!(search(c, &q("isikli")).unwrap().total, 1);
}
