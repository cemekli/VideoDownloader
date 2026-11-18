//! STATE.md F7.3, SPEC AI-B01…B04, AI-C07 (ADR-0037): tag suggestions from
//! a fake OpenAI-compatible chat API on 127.0.0.1 (the user's Ollama):
//! what is sent, what is stored, rejected values never come back, the same
//! text is not asked twice, accepting marks the tag, a server without
//! `reasoning_effort`, a key, a remote host, Offline Mode.

mod common;

use common::fake_embed::FakeEmbed;
use common::*;
use std::sync::atomic::Ordering;
use vv_domain::error::ErrorCode;
use vv_engine::embed::{self, EmbedError};
use vv_engine::suggest::{self, Outcome};
use vv_persistence::library::organize;
use vv_persistence::library::Clock;
use vv_persistence::proposals;
use vv_persistence::settings::{self, SuggestApi};
use vv_persistence::transcripts::{self, CaptionKind, NewSegment, Origin, Provenance};
use vv_secrets::{Secret, SecretStore};

const MODEL: &str = "qwen3.5:4b";

fn set_api(lib: &Lib, url: &str, key: Option<&str>, remote_allowed: bool) {
    let key_ref = key.map(|k| {
        lib.secrets
            .put(&Secret::new(k.as_bytes().to_vec()))
            .unwrap()
            .as_str()
            .to_owned()
    });
    let api = SuggestApi {
        url: embed::normalize_url(url).unwrap(),
        model: MODEL.into(),
        key_ref,
        remote_allowed,
        checked_at: 1,
    };
    let now = lib.clock.now_ms();
    lib.write(|c| settings::set_suggest_api(c, Some(&api), now).unwrap());
}

/// A bookmark titled `title` with a caption transcript of `lines`.
fn video(lib: &Lib, url: &str, title: &str, lines: &[&str]) -> String {
    let item = lib.save(url);
    let segs: Vec<NewSegment> = lines
        .iter()
        .enumerate()
        .map(|(i, t)| NewSegment::new(i as i64 * 40_000, i as i64 * 40_000 + 3_000, *t))
        .collect();
    let o = Origin {
        item_id: item.clone(),
        asset_id: None,
        language: "en".into(),
        provenance: Provenance::SourceCaptions,
        provider: "source_captions".into(),
        model_id: None,
        model_version: None,
        source_fingerprint: "yt-dlp:subtitles:en".into(),
        caption_kind: Some(CaptionKind::Manual),
    };
    let now = lib.clock.now_ms();
    lib.write(|c| {
        c.execute(
            "UPDATE library_item SET title = ?2 WHERE id = ?1",
            [&item, &title.to_owned()],
        )
        .unwrap();
        let tx = c.transaction().unwrap();
        transcripts::save(&tx, &o, transcripts::Status::Ready, None, &segs, None, now).unwrap();
        tx.commit().unwrap();
    });
    item
}

fn values(o: &Outcome) -> Vec<String> {
    match o {
        Outcome::Asked(p) => p.iter().map(|p| p.value.clone()).collect(),
        other => panic!("not asked: {other:?}"),
    }
}

fn run(lib: &Lib, item: &str, force: bool) -> Result<Outcome, EmbedError> {
    suggest::suggest_item(lib.cx(), item, force, || false)
}

#[test]
fn suggestions_are_stored_not_applied_and_reuse_library_tags() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake.url, None, false);
    let item = video(
        &lib,
        "https://example.com/harrier",
        "AV-8B Harrier vertical landing",
        &["The Harrier jump jet lands vertically on the deck."],
    );
    // The library already has "uçak" and the item has "belgesel".
    let other = lib.save("https://example.com/other");
    let now = lib.clock.now_ms();
    lib.write(|c| {
        let (t, _) = organize::get_or_create_tag(c, "uçak", now).unwrap();
        organize::tag_items(c, &t.id, std::slice::from_ref(&other), now).unwrap();
        let (b, _) = organize::get_or_create_tag(c, "Belgesel", now).unwrap();
        organize::tag_items(c, &b.id, std::slice::from_ref(&item), now).unwrap();
    });
    fake.set_chat_answer(
        "<think>jets</think>{\"tags\": [\"uçak\", \"#Harrier\", \"UÇAK\", \"belgesel\", \"  \", \"askeri havacılık\"]}",
    );
    let o = run(&lib, &item, false).unwrap();
    // Cleaned, without duplicates and without the tag the item has.
    assert_eq!(values(&o), vec!["uçak", "Harrier", "askeri havacılık"]);
    // Nothing was applied (AI-B02).
    let tags = lib.write(|c| organize::item_tags(c, &item).unwrap());
    assert_eq!(
        tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        vec!["Belgesel"]
    );

    // What was sent: the model, the item text, the library's tags, a schema,
    // and thinking off.
    let chats = fake.chats();
    assert_eq!(chats.len(), 1);
    let body = &chats[0];
    assert_eq!(body["model"], MODEL);
    assert_eq!(body["reasoning_effort"], "none");
    assert_eq!(body["response_format"]["type"], "json_schema");
    let user = body["messages"][1]["content"].as_str().unwrap();
    assert!(
        user.contains("Title: AV-8B Harrier vertical landing"),
        "{user}"
    );
    assert!(user.contains("jump jet lands vertically"), "{user}");
    assert!(user.contains("uçak"), "{user}");

    // The same text, model and API: not asked again (AI-B04).
    assert_eq!(run(&lib, &item, false).unwrap(), Outcome::Current);
    assert_eq!(fake.chats().len(), 1);
}

#[test]
fn rejected_values_never_come_back_and_accepting_marks_the_tag() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake.url, None, false);
    let item = video(
        &lib,
        "https://example.com/bread",
        "Sourdough at home",
        &["Mix the dough."],
    );
    fake.set_chat_answer(r#"{"tags":["ekmek","mutfak","tarif"]}"#);
    let o = run(&lib, &item, false).unwrap();
    let Outcome::Asked(ps) = o else { panic!() };
    let now = lib.clock.now_ms();
    let (ekmek, mutfak) = (ps[0].clone(), ps[1].clone());
    lib.write(|c| {
        // Reject "mutfak"; accept "ekmek" the way the app does.
        proposals::decide(c, &mutfak.id, false, now).unwrap();
        let (t, _) = organize::get_or_create_tag(c, &ekmek.value, now).unwrap();
        organize::tag_items(c, &t.id, std::slice::from_ref(&item), now).unwrap();
        assert!(proposals::link_tag(c, &item, &t.id, &ekmek.id, now).unwrap());
        proposals::decide(c, &ekmek.id, true, now).unwrap();
        let (origin, pid): (String, Option<String>) = c
            .query_row(
                "SELECT origin, proposal_id FROM library_item_tag WHERE item_id = ?1 AND tag_id = ?2",
                [&item, &t.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(origin, "ai_accepted");
        assert_eq!(pid.as_deref(), Some(ekmek.id.as_str()));
        // A decided proposal is not decided again.
        assert!(proposals::decide(c, &ekmek.id, false, now).is_err());
    });

    // Asked again (another model would do the same): "mutfak" stays out,
    // "ekmek" is a tag now, "tarif" is still pending, "hamur" is new.
    fake.set_chat_answer(r#"{"tags":["Mutfak","ekmek","tarif","hamur"]}"#);
    let o = run(&lib, &item, true).unwrap();
    assert_eq!(values(&o), vec!["tarif", "hamur"]);
    let statuses = lib.write(|c| {
        c.prepare("SELECT value, status FROM ai_proposal WHERE item_id = ?1 ORDER BY value")
            .unwrap()
            .query_map([&item], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
    });
    assert_eq!(
        statuses,
        vec![
            ("ekmek".into(), "accepted".into()),
            ("hamur".into(), "pending".into()),
            ("mutfak".into(), "rejected".into()),
            ("tarif".into(), "pending".into()),
        ]
    );
    assert_eq!(lib.write(|c| proposals::pending_count(c).unwrap()), 2);
}

#[test]
fn a_server_without_reasoning_effort_is_asked_again_without_it() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake.url, None, false);
    let item = video(
        &lib,
        "https://example.com/run",
        "Marathon training",
        &["Ten kilometres."],
    );
    fake.set_mode("no-reasoning");
    fake.set_chat_answer(r#"{"tags":["koşu"]}"#);
    assert_eq!(values(&run(&lib, &item, false).unwrap()), vec!["koşu"]);
    let chats = fake.chats();
    assert_eq!(chats.len(), 2);
    assert!(chats[1].get("reasoning_effort").is_none());
}

#[test]
fn failures_keys_remote_hosts_and_offline_mode() {
    let lib = library(Setup::fake());
    let item = video(&lib, "https://example.com/x", "Something", &["words"]);
    // No API set.
    assert_eq!(
        suggest::error_code(&run(&lib, &item, false).unwrap_err()),
        ErrorCode::ModelMissing
    );

    // A key: sent as a bearer token; a wrong one is ACCESS_DENIED.
    let keyed = FakeEmbed::start(Some("k1"));
    keyed.set_chat_answer(r#"{"tags":["a"]}"#);
    set_api(&lib, &keyed.url, Some("k1"), false);
    assert_eq!(values(&run(&lib, &item, false).unwrap()), vec!["a"]);
    set_api(&lib, &keyed.url, Some("wrong"), false);
    assert_eq!(
        suggest::error_code(&run(&lib, &item, true).unwrap_err()),
        ErrorCode::AccessDenied
    );

    // A failing server: the pending suggestions stay as they were.
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake.url, None, false);
    fake.set_mode("fail");
    assert!(run(&lib, &item, true).is_err());
    assert_eq!(
        lib.write(|c| proposals::pending(c, &item).unwrap()).len(),
        1
    );
    // An answer without a tag list.
    fake.set_mode("ok");
    fake.set_chat_answer("I cannot help with that.");
    assert_eq!(
        suggest::error_code(&run(&lib, &item, true).unwrap_err()),
        ErrorCode::ExtractorFailed
    );

    // Another host needs agreement; Offline Mode blocks it.
    set_api(&lib, "https://api.example.com/v1", None, false);
    assert_eq!(
        suggest::error_code(&run(&lib, &item, true).unwrap_err()),
        ErrorCode::UrlBlocked
    );
    set_api(&lib, "https://api.example.com/v1", None, true);
    lib.cx().offline.store(true, Ordering::SeqCst);
    assert_eq!(
        suggest::error_code(&run(&lib, &item, true).unwrap_err()),
        ErrorCode::OfflineMode
    );
    // This PC works offline.
    set_api(&lib, &fake.url, None, false);
    fake.set_chat_answer(r#"{"tags":["b"]}"#);
    assert!(run(&lib, &item, true).is_ok());
}

#[test]
fn an_item_without_text_is_not_sent() {
    let lib = library(Setup::fake());
    let fake = FakeEmbed::start(None);
    set_api(&lib, &fake.url, None, false);
    let item = lib.save("https://example.com/empty");
    lib.write(|c| {
        c.execute(
            "UPDATE library_item SET title = NULL, description = NULL WHERE id = ?1",
            [&item],
        )
        .unwrap()
    });
    assert_eq!(run(&lib, &item, false).unwrap(), Outcome::NoText);
    assert!(fake.chats().is_empty());
}
