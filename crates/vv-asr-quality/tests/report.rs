//! F6.7 report on a real library file: captions as reference, local
//! transcripts from two models, the job that made each (ADR-0034).

use rusqlite::params;
use vv_asr_quality::report::{
    failed_jobs, find_item, item_report, items_with_local, run_time, Options,
};
use vv_persistence::library::repo::{self, LibraryItem};
use vv_persistence::library::{ItemKind, SourcePlatform};
use vv_persistence::transcripts::{self, CaptionKind, NewSegment, Origin, Provenance, Status};
use vv_persistence::{Database, OpenOptions};

/// The payload as the engine writes it (camelCase), so a rename there fails here.
#[test]
fn job_payload_matches_the_engine() {
    let p = serde_json::to_value(vv_engine::transcribe::Payload {
        model_id: "whisper-small".into(),
        language: "tr".into(),
        allow_low_memory: false,
    })
    .unwrap();
    assert_eq!(p["modelId"], "whisper-small");
}

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The real Turkish caption fixture (F6.2 test hardening) as segments.
fn tr_captions() -> Vec<NewSegment> {
    let bytes =
        std::fs::read(root().join("tests/fixtures/captions/youtube-tr-manual.vtt")).unwrap();
    vv_ytdlp::captions::parse_vtt(&bytes)
        .unwrap()
        .into_iter()
        .map(|c| NewSegment::new(c.start_ms, c.end_ms, c.text))
        .collect()
}

fn origin(item: &str, prov: Provenance, model: Option<&str>) -> Origin {
    let local = prov == Provenance::GeneratedLocal;
    Origin {
        item_id: item.to_owned(),
        asset_id: None,
        language: "tr".into(),
        provenance: prov,
        provider: if local {
            "whisper_cpp"
        } else {
            "source_captions"
        }
        .into(),
        model_id: model.map(str::to_owned),
        model_version: model.map(|_| "5359861c".to_owned()),
        source_fingerprint: "fp".into(),
        caption_kind: (!local).then_some(CaptionKind::Manual),
    }
}

fn save(db: &mut Database, o: &Origin, status: Status, covered: Option<i64>, segs: &[NewSegment]) {
    let tx = db.writer().transaction().unwrap();
    transcripts::save(&tx, o, status, covered, segs, None, 1).unwrap();
    tx.commit().unwrap();
}

fn job(db: &mut Database, id: &str, item: &str, model: &str, lang: &str, attempts: &[(i64, i64)]) {
    let c = db.writer();
    c.execute(
        "INSERT INTO job (id, type, item_id, status, payload, idempotency_key, created_at, updated_at, finished_at) \
         VALUES (?1, 'transcription', ?2, 'succeeded', ?3, ?1, 1, 1, ?4)",
        params![id, item, format!(r#"{{"modelId":"{model}","language":"{lang}","allowLowMemory":false}}"#), attempts.last().unwrap().1],
    )
    .unwrap();
    for (n, (s, e)) in attempts.iter().enumerate() {
        c.execute(
            "INSERT INTO job_attempt (job_id, attempt, generation, started_at, ended_at, outcome) VALUES (?1, ?2, 1, ?3, ?4, 'succeeded')",
            params![id, n as i64 + 1, s, e],
        )
        .unwrap();
    }
}

#[test]
fn report_scores_each_model_against_the_uploader_captions() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("library.sqlite");
    let mut db = Database::open(&path, &OpenOptions::default()).unwrap();
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Youtube, 1);
    it.canonical_url = Some("https://www.youtube.com/watch?v=abc".into());
    it.original_url = it.canonical_url.clone();
    it.title = Some("Kadıköy | vlog".into());
    it.duration_ms = Some(30_000);
    repo::insert_item(db.writer(), &it).unwrap();

    let refs = tr_captions();
    save(
        &mut db,
        &origin(&it.id, Provenance::SourceCaptions, None),
        Status::Ready,
        None,
        &refs,
    );
    // Medium: the same words, no punctuation, one Turkish letter lost.
    let medium: Vec<NewSegment> = refs
        .iter()
        .map(|s| NewSegment::new(s.start_ms, s.end_ms, s.text.replace("Kadıköy", "Kadikoy")))
        .collect();
    save(
        &mut db,
        &origin(&it.id, Provenance::GeneratedLocal, Some("whisper-medium")),
        Status::Ready,
        None,
        &medium,
    );
    // Tiny: only the first half, as a partial transcript.
    let tiny: Vec<NewSegment> = refs
        .iter()
        .filter(|s| s.end_ms <= 12_760)
        .cloned()
        .collect();
    save(
        &mut db,
        &origin(&it.id, Provenance::GeneratedLocal, Some("whisper-tiny")),
        Status::Partial,
        Some(12_760),
        &tiny,
    );
    job(
        &mut db,
        "j-med",
        &it.id,
        "whisper-medium",
        "auto",
        &[(1_000, 6_000), (10_000, 15_000)],
    );
    job(
        &mut db,
        "j-tiny",
        &it.id,
        "whisper-tiny",
        "tr",
        &[(0, 2_000)],
    );
    drop(db);

    let c = vv_persistence::open_reader(&path).unwrap();
    assert_eq!(items_with_local(&c).unwrap().len(), 1);
    assert_eq!(find_item(&c, "VLOG").unwrap().id, it.id);
    assert!(find_item(&c, "nothing").is_err());
    assert_eq!(find_item(&c, it.id.get(..8).unwrap()).unwrap().id, it.id);
    assert_eq!(
        run_time(&c, &it.id, "whisper-medium", "tr").unwrap(),
        Some((10_000, 2))
    );
    assert_eq!(run_time(&c, &it.id, "whisper-base", "tr").unwrap(), None);
    assert!(failed_jobs(&c, 10).unwrap().is_empty());

    let opts = Options {
        reference_file: None,
        reference_path: None,
        terms: vec!["Kadıköy".into(), "Iğdır".into(), "İstanbul".into()],
    };
    let md = item_report(&c, &find_item(&c, &it.id).unwrap(), &opts).unwrap();
    println!("{md}");
    assert!(md.starts_with("## Kadıköy \\| vlog\n"), "{md}");
    assert!(md.contains("yükleyicinin kendi altyazısı"));
    // Smallest model first; Tiny is scored only up to where it got.
    let tiny_at = md.find("| Tiny |").unwrap();
    let med_at = md.find("| Medium |").unwrap();
    assert!(tiny_at < med_at);
    let tiny_row = md.lines().find(|l| l.starts_with("| Tiny |")).unwrap();
    assert!(tiny_row.contains("| partial (ilk 0:12) |"), "{tiny_row}");
    assert!(tiny_row.contains("| 0.0 % | 0.0 % | 0.0 % |"), "{tiny_row}");
    assert!(tiny_row.contains("| 0:02 | 6.4× |"), "{tiny_row}");
    let med_row = md.lines().find(|l| l.starts_with("| Medium |")).unwrap();
    assert!(
        !med_row.contains("partial") && med_row.contains("2 denemede"),
        "{med_row}"
    );
    // One word in the reference is wrong only by its Turkish letters.
    assert!(
        med_row.contains("| 0.0 % |"),
        "folded WER should be 0: {med_row}"
    );
    assert!(med_row.contains("| 0:10 | 3.0× |"), "{med_row}");
    assert!(md.contains("`kadıköy` → `kadikoy` ×1"));
    assert!(md.contains("| Kadıköy | 1 | 1 | 0 |"), "{md}");
    assert!(md.contains("| Iğdır | 2 | 0 | 2 |"), "{md}");
    // `İstanbul'dayız`: the suffix still counts.
    assert!(md.contains("| İstanbul | 1 | 1 | 1 |"), "{md}");
}

#[test]
fn one_model_and_no_captions_says_what_is_missing() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("library.sqlite");
    let mut db = Database::open(&path, &OpenOptions::default()).unwrap();
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Youtube, 1);
    it.canonical_url = Some("https://www.youtube.com/watch?v=one".into());
    it.title = Some("Tek model".into());
    it.duration_ms = Some(30_000);
    repo::insert_item(db.writer(), &it).unwrap();
    save(
        &mut db,
        &origin(&it.id, Provenance::GeneratedLocal, Some("whisper-medium")),
        Status::Ready,
        None,
        &tr_captions(),
    );
    job(&mut db, "j", &it.id, "whisper-medium", "tr", &[(0, 15_000)]);
    drop(db);
    let c = vv_persistence::open_reader(&path).unwrap();
    let opts = Options {
        reference_file: None,
        reference_path: None,
        terms: vec![],
    };
    let md = item_report(&c, &find_item(&c, &it.id).unwrap(), &opts).unwrap();
    assert!(
        md.contains("- Medium (referans) süresi: 0:15, gerçek zamanın 2.0 katı."),
        "{md}"
    );
    assert!(md.contains("**Karşılaştırılacak döküm yok.**"), "{md}");
    assert!(md.contains("Get site captions"), "{md}");
    assert!(!md.contains("| Model |"), "{md}");
}

#[test]
fn failed_transcriptions_are_listed_with_their_message() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("library.sqlite");
    let mut db = Database::open(&path, &OpenOptions::default()).unwrap();
    let mut it = LibraryItem::new(ItemKind::Bookmark, SourcePlatform::Youtube, 1);
    it.canonical_url = Some("https://www.youtube.com/watch?v=f".into());
    it.title = Some("Türkçe video".into());
    repo::insert_item(db.writer(), &it).unwrap();
    db.writer()
        .execute(
            "INSERT INTO job (id, type, item_id, status, payload, idempotency_key, error_code, redacted_error_message, created_at, updated_at, finished_at) \
             VALUES ('jf', 'transcription', ?1, 'failed', '{\"modelId\":\"whisper-small\",\"language\":\"auto\"}', 'jf', 'INTERNAL_ERROR', 'segment 120: ends after the media ends', 1, 2, 3)",
            params![it.id],
        )
        .unwrap();
    drop(db);
    let c = vv_persistence::open_reader(&path).unwrap();
    let f = failed_jobs(&c, 10).unwrap();
    assert_eq!(f.len(), 1);
    assert_eq!(
        (
            f[0].item.as_str(),
            f[0].model_id.as_str(),
            f[0].error_code.as_str()
        ),
        ("Türkçe video", "whisper-small", "INTERNAL_ERROR")
    );
    assert_eq!(
        f[0].message.as_deref(),
        Some("segment 120: ends after the media ends")
    );
}
