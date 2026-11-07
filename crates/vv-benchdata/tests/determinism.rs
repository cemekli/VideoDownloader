//! STATE.md F0.12 exit check: the same seed gives the same output, on every
//! OS, and the data has the shape SPEC §25.2 asks for (ADR-0009).

use serde_json::Value;
use vv_benchdata::{generate, Params, GENERATOR_VERSION};

const GOLDEN: &str = include_str!("../../../tests/fixtures/benchdata/golden.json");

#[test]
fn default_dataset_matches_the_golden_hashes() {
    let g: Value = serde_json::from_str(GOLDEN).unwrap();
    let m = generate(Params::default()).manifest();
    assert_eq!(
        g["generatorVersion"], GENERATOR_VERSION,
        "bump the golden file together with GENERATOR_VERSION"
    );
    assert_eq!(g["seed"], m.seed);
    assert_eq!((m.items, m.segments), (10_000, 100_000));
    assert_eq!(
        g["itemsSha256"],
        m.items_sha256.as_str(),
        "items.jsonl changed: bump GENERATOR_VERSION and the golden file"
    );
    assert_eq!(
        g["segmentsSha256"],
        m.segments_sha256.as_str(),
        "segments.jsonl changed: bump GENERATOR_VERSION and the golden file"
    );
}

#[test]
fn same_seed_same_bytes_other_seed_other_bytes() {
    let p = Params {
        seed: 7,
        items: 500,
        segments: 3_000,
    };
    let (a, b) = (generate(p).manifest(), generate(p).manifest());
    assert_eq!(a, b);
    let c = generate(Params { seed: 8, ..p }).manifest();
    assert_ne!(a.items_sha256, c.items_sha256);
    assert_ne!(a.segments_sha256, c.segments_sha256);
}

#[test]
fn segment_rules_do_not_shift_items() {
    // Items come from their own stream: asking for fewer segments must not
    // change a single item.
    let a = generate(Params {
        seed: 3,
        items: 300,
        segments: 5_000,
    });
    let b = generate(Params {
        seed: 3,
        items: 300,
        segments: 10,
    });
    assert_eq!(a.items, b.items);
}

#[test]
fn data_has_the_shape_spec_25_2_asks_for() {
    let d = generate(Params::default());
    let it = &d.items;
    let count = |f: &dyn Fn(&vv_benchdata::Item) -> bool| it.iter().filter(|i| f(i)).count();
    assert!(count(&|i| i.kind == "bookmark") > 5_000);
    assert!(count(&|i| i.kind == "downloaded") > 2_000);
    assert!(count(&|i| i.kind == "local_video") > 1_000);
    assert!(
        count(&|i| i.title.chars().count() > 200) > 100,
        "very long titles"
    );
    assert!(
        count(&|i| i.duration_ms.is_none()) > 500,
        "unknown durations"
    );
    assert!(count(&|i| !i.has_thumbnail) > 500, "missing thumbnails");
    assert!(count(&|i| i.tags.len() >= 3) > 1_000, "several tags");
    assert!(
        count(&|i| i.collections.len() >= 2) > 500,
        "several collections"
    );
    assert!(
        count(&|i| i.title.contains('\u{0308}') || i.title.contains('\u{0327}')) > 10,
        "NFD titles"
    );
    assert!(
        count(&|i| i.title.chars().any(|c| c as u32 >= 0x1F300)) > 300,
        "emoji"
    );
    for s in ["ı", "İ", "ş", "ğ", "ç", "ö", "ü"] {
        assert!(count(&|i| i.title.contains(s)) > 50, "Turkish letter {s}");
    }
    for s in ["pending", "failed", "ok"] {
        assert!(
            count(&|i| i.metadata_status == s) > 100,
            "metadata status {s}"
        );
    }
    // Never a real host (SPEC §25.2).
    for i in it {
        if let Some(u) = &i.source_url {
            let host = u.trim_start_matches("https://").split('/').next().unwrap();
            assert!(host.ends_with(".example"), "{u}");
        }
    }
    // Segments: ordered, non-overlapping, only on items with media.
    let mut last = (0u32, 0u64);
    for s in &d.segments {
        assert!(s.end_ms > s.start_ms);
        if s.item_id == last.0 {
            assert!(s.start_ms >= last.1, "overlap in item {}", s.item_id);
        }
        last = (s.item_id, s.end_ms);
        assert_ne!(it[s.item_id as usize - 1].kind, "bookmark");
    }
    let mut ids: Vec<u32> = it.iter().map(|i| i.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), 10_000);
    assert!(it.windows(2).all(|w| w[0].created_at < w[1].created_at));
}

#[test]
fn queries_are_deterministic_and_quoted() {
    let a = vv_benchdata::bench::queries(1, 50);
    assert_eq!(a, vv_benchdata::bench::queries(1, 50));
    assert!(a.iter().all(|q| q.starts_with('"')));
}

#[test]
fn benchmark_runs_on_a_small_library() {
    let d = generate(Params {
        seed: 5,
        items: 400,
        segments: 2_000,
    });
    let dir = std::env::temp_dir().join(format!("vv-bench-test-{}", std::process::id()));
    let r = vv_benchdata::bench::run(&d, &dir).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let ids: Vec<&str> = r.stats.iter().map(|s| s.id).collect();
    assert_eq!(ids, ["BM-01", "BM-02", "BM-03"]);
    assert!(r
        .stats
        .iter()
        .all(|s| s.samples > 0 && s.p95_ms >= s.p50_ms));
    assert!(
        r.stats[1].rows > 0 && r.stats[2].rows > 0,
        "queries must return rows"
    );
}
