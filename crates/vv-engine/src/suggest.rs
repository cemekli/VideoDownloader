//! Tag suggestions from the user's chat API (SPEC AI-B01…B04, AI-C07;
//! STATE.md F7.3; ADR-0037).
//!
//! - VideoVault hosts no language model. The user gives the address of an
//!   OpenAI-compatible `/v1/chat/completions` API (Ollama, LM Studio,
//!   llama.cpp's server, a cloud service), a model name and, when the API
//!   wants one, a key (kept in the OS credential store). The address rules
//!   are those of the embedding API: a host other than this PC needs the
//!   user's agreement and is blocked in Offline Mode.
//! - The model reads the item's title, description and a transcript
//!   excerpt spread over the whole video, plus the library's most used
//!   tags (so it reuses them), and answers `{"tags": [...]}`. The answer is
//!   asked for with a JSON schema; a reasoning model's `<think>` text and
//!   any words around the JSON are ignored.
//! - Thinking is turned off with `reasoning_effort: "none"` (Ollama); a
//!   server that refuses the field (HTTP 400) is asked again without it.
//! - Suggestions are stored, never applied ([`vv_persistence::proposals`]).
//!   The same text with the same model and API is not asked again
//!   (AI-B04) unless the user asks for new suggestions.

use crate::context::{Context, Store};
use crate::embed::{self, agent, checked_base, post_json, stored_key, EmbedError};
use serde::Deserialize;
use vv_domain::error::ErrorCode;
use vv_persistence::proposals::{self, Proposal, Source};
use vv_persistence::rusqlite::Connection;
use vv_persistence::settings::{self, SuggestApi};
use vv_secrets::SecretStore;

/// Changes when the prompt does, so old runs are not taken as current.
pub const PROMPT_VERSION: &str = "1";
pub const TITLE_MAX: usize = 300;
pub const DESCRIPTION_MAX: usize = 1200;
/// About 1,300 tokens: with the rest the prompt fits a 4,096-token context
/// (Ollama's default) with room for the answer.
pub const TRANSCRIPT_MAX: usize = 4000;
/// Existing tags shown to the model.
pub const EXISTING_TAGS: usize = 60;
/// Largest answer text read (a reasoning model may think at length).
const ANSWER_MAX: usize = 256 * 1024;

/// One chat API, ready for requests.
#[derive(Clone)]
pub struct ChatApi {
    base: String,
    model: String,
    key: Option<String>,
    agent: ureq::Agent,
}

impl std::fmt::Debug for ChatApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatApi")
            .field("base", &self.base)
            .field("model", &self.model)
            .field("key", &self.key.as_ref().map(|_| "<set>"))
            .finish()
    }
}

impl ChatApi {
    /// An API for `url` and `model`. Refuses a host other than this PC in
    /// Offline Mode, and one the user did not agree to send text to.
    pub fn new(
        url: &str,
        model: &str,
        key: Option<String>,
        remote_allowed: bool,
        offline: bool,
    ) -> Result<ChatApi, EmbedError> {
        let base = checked_base(url, remote_allowed, offline)?;
        let model = model.trim();
        if model.is_empty() || model.len() > 200 {
            return Err(EmbedError::Failed(
                ErrorCode::ModelMissing,
                "model name".into(),
            ));
        }
        Ok(ChatApi {
            base,
            model: model.to_owned(),
            key: key.filter(|k| !k.is_empty()),
            agent: agent(),
        })
    }

    /// The API in the settings, with its key from the credential store.
    pub fn from_settings(
        cfg: &SuggestApi,
        secrets: &dyn SecretStore,
        offline: bool,
    ) -> Result<ChatApi, EmbedError> {
        let key = stored_key(cfg.key_ref.as_deref(), secrets)?;
        ChatApi::new(&cfg.url, &cfg.model, key, cfg.remote_allowed, offline)
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn body(&self, system: &str, user: &str, no_think: bool) -> String {
        let mut v = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": 0.2,
            "stream": false,
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "tags",
                    "strict": true,
                    "schema": {
                        "type": "object",
                        "properties": {
                            "tags": {"type": "array", "items": {"type": "string"}}
                        },
                        "required": ["tags"],
                        "additionalProperties": false
                    }
                }
            }
        });
        if no_think {
            v["reasoning_effort"] = "none".into();
        }
        v.to_string()
    }

    /// The model's answer text to one system + user message.
    pub fn complete(
        &self,
        system: &str,
        user: &str,
        mut should_stop: impl FnMut() -> bool,
    ) -> Result<String, EmbedError> {
        let url = format!("{}/chat/completions", self.base);
        let key = self.key.as_deref();
        let buf = match post_json(
            &self.agent,
            &url,
            key,
            self.body(system, user, true),
            &mut should_stop,
        ) {
            Err(EmbedError::Failed(_, m)) if m == "HTTP 400" => post_json(
                &self.agent,
                &url,
                key,
                self.body(system, user, false),
                &mut should_stop,
            )?,
            r => r?,
        };
        answer_text(&buf).map_err(|m| EmbedError::Failed(ErrorCode::ExtractorFailed, m))
    }
}

#[derive(Deserialize)]
struct ChatOut {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Msg,
}

#[derive(Deserialize)]
struct Msg {
    #[serde(default)]
    content: Option<String>,
}

/// The first choice's text from a `/chat/completions` answer.
pub fn answer_text(body: &[u8]) -> Result<String, String> {
    let out: ChatOut = serde_json::from_slice(body).map_err(|_| "not a chat answer")?;
    let text = out
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .ok_or("an answer without text")?;
    let mut end = text.len().min(ANSWER_MAX);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(text.get(..end).unwrap_or_default().to_owned())
}

/// The tags in a model's answer: `{"tags": [...]}` (or a bare list),
/// after dropping `<think>…</think>` and anything around the JSON.
pub fn parse_tags(answer: &str) -> Result<Vec<String>, String> {
    let mut text = answer.to_owned();
    while let Some(start) = text.find("<think>") {
        match text.get(start..).and_then(|t| t.find("</think>")) {
            Some(len) => text.replace_range(start..start + len + "</think>".len(), ""),
            None => text.truncate(start),
        }
    }
    let text = text.trim();
    let candidates = [('{', '}'), ('[', ']')];
    for (open, close) in candidates {
        let (Some(a), Some(b)) = (text.find(open), text.rfind(close)) else {
            continue;
        };
        if b <= a {
            continue;
        }
        let Some(Ok(v)) = text
            .get(a..=b)
            .map(serde_json::from_str::<serde_json::Value>)
        else {
            continue;
        };
        let list = match &v {
            serde_json::Value::Array(_) => &v,
            o => &o["tags"],
        };
        if let serde_json::Value::Array(items) = list {
            return Ok(items
                .iter()
                .filter_map(|x| x.as_str())
                .map(str::to_owned)
                .collect());
        }
    }
    Err("no tag list in the answer".into())
}

/// The system and user messages for one item.
pub fn messages(s: &Source, existing: &[String]) -> (String, String) {
    let system = "You suggest tags for one video in a personal video library. \
Answer only with JSON: {\"tags\": [\"...\"]}. Give 3 to 6 tags of 1 to 3 words each \
about what the video is about: its topics, places, people, things and genre. \
When one of the library's tags fits, use it exactly as written. \
Write new tags in the language most of the library's tags use; \
when the library has no tags, in the language of the title. \
Do not suggest the site name, the word \"video\" or other words that fit any video."
        .to_owned();
    let mut user = String::new();
    if existing.is_empty() {
        user.push_str("The library has no tags yet.\n\n");
    } else {
        user.push_str("Tags in the library: ");
        user.push_str(&existing.join(", "));
        user.push_str("\n\n");
    }
    user.push_str(&format!("Title: {}\n", one_line(&s.title)));
    if !s.description.trim().is_empty() {
        user.push_str(&format!("Description: {}\n", one_line(&s.description)));
    }
    if !s.transcript.trim().is_empty() {
        user.push_str(&format!(
            "Transcript excerpt: {}\n",
            one_line(&s.transcript)
        ));
    }
    (system, user)
}

fn one_line(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Asks the API one small question ("Check" in the settings).
pub fn check(api: &ChatApi) -> Result<(), EmbedError> {
    let a = api.complete(
        "Answer only with JSON: {\"tags\": [\"...\"]}.",
        "Give one tag for a video titled: İstanbul'da bir gün / A day in Istanbul",
        || false,
    )?;
    parse_tags(&a)
        .map(|_| ())
        .map_err(|m| EmbedError::Failed(ErrorCode::ExtractorFailed, m))
}

/// The API in the settings, if one was set and checked.
pub fn configured(c: &Connection) -> Option<SuggestApi> {
    settings::suggest_api(c).ok().flatten()
}

/// What one item's run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The model was asked; the item's pending suggestions now.
    Asked(Vec<Proposal>),
    /// The same text with this model and API was asked before (AI-B04).
    Current,
    /// The item has no text to suggest from, or is gone.
    NoText,
}

/// Suggests tags for one item with the API in the settings. `force` asks
/// again even when the text did not change ("Suggest again").
pub fn suggest_item<S: Store>(
    cx: &Context<S>,
    item_id: &str,
    force: bool,
    should_stop: impl FnMut() -> bool,
) -> Result<Outcome, EmbedError> {
    let cfg = cx.store.read(configured).ok_or_else(|| {
        EmbedError::Failed(ErrorCode::ModelMissing, "no suggestion API is set".into())
    })?;
    let api = ChatApi::from_settings(&cfg, cx.secrets.as_ref(), cx.is_offline())?;
    suggest_with(cx, &api, item_id, force, should_stop)
}

/// [`suggest_item`] with a given API (a bulk run makes it once).
pub fn suggest_with<S: Store>(
    cx: &Context<S>,
    api: &ChatApi,
    item_id: &str,
    force: bool,
    should_stop: impl FnMut() -> bool,
) -> Result<Outcome, EmbedError> {
    let db = |_| EmbedError::Failed(ErrorCode::InternalError, "library".into());
    let (src, existing, last) = cx
        .store
        .read(|c| {
            let src = proposals::source(c, item_id, TITLE_MAX, DESCRIPTION_MAX, TRANSCRIPT_MAX)?;
            let tags = proposals::top_tags(c, EXISTING_TAGS)?;
            let last = proposals::last_run(c, item_id)?;
            Ok::<_, proposals::ProposalError>((src, tags, last))
        })
        .map_err(db)?;
    let Some(src) = src.filter(|s| !s.is_empty()) else {
        return Ok(Outcome::NoText);
    };
    let fp = proposals::fingerprint(PROMPT_VERSION, api.model(), api.base(), &src);
    if !force && last.as_deref() == Some(fp.as_str()) {
        return Ok(Outcome::Current);
    }
    let (system, user) = messages(&src, &existing);
    let answer = api.complete(&system, &user, should_stop)?;
    let tags =
        parse_tags(&answer).map_err(|m| EmbedError::Failed(ErrorCode::ExtractorFailed, m))?;
    let now = cx.now();
    let (model, base) = (api.model().to_owned(), api.base().to_owned());
    let item = item_id.to_owned();
    let saved = cx.store.write(move |c| -> Result<Vec<Proposal>, ()> {
        let tx = c.transaction().map_err(|_| ())?;
        let p = proposals::save_run(&tx, &item, &model, &base, &fp, &tags, now).map_err(|_| ())?;
        tx.commit().map_err(|_| ())?;
        Ok(p)
    });
    saved
        .map(Outcome::Asked)
        .map_err(|_| EmbedError::Failed(ErrorCode::InternalError, "suggestions not saved".into()))
}

/// Error code for the screen.
pub fn error_code(e: &EmbedError) -> ErrorCode {
    embed::error_code(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_read_from_plain_wrapped_and_thinking_answers() {
        assert_eq!(
            parse_tags(r#"{"tags":["uçak","Harrier"]}"#).unwrap(),
            vec!["uçak", "Harrier"]
        );
        assert_eq!(
            parse_tags("<think>\nThe video is {about} jets.\n</think>\n{\"tags\": [\"jet\"]}")
                .unwrap(),
            vec!["jet"]
        );
        assert_eq!(
            parse_tags("Here you go:\n```json\n{\"tags\": [\"a\", 3, \"b\"]}\n```").unwrap(),
            vec!["a", "b"]
        );
        assert_eq!(parse_tags(r#"["x","y"]"#).unwrap(), vec!["x", "y"]);
        assert!(parse_tags("no idea").is_err());
        assert!(parse_tags("<think>{\"tags\":[\"hidden\"]}").is_err());
    }

    #[test]
    fn chat_answers_are_read() {
        let body = br#"{"choices":[{"index":0,"message":{"role":"assistant","content":"{\"tags\":[\"a\"]}"}}]}"#;
        assert_eq!(answer_text(body).unwrap(), r#"{"tags":["a"]}"#);
        assert!(answer_text(br#"{"choices":[]}"#).is_err());
        assert!(answer_text(b"<html>").is_err());
    }

    #[test]
    fn the_prompt_holds_the_item_text_and_library_tags() {
        let s = Source {
            title: "AV-8B Harrier\nlanding".into(),
            description: "".into(),
            transcript: "vertical take off".into(),
        };
        let (system, user) = messages(&s, &["uçak".into(), "belgesel".into()]);
        assert!(system.contains("JSON"));
        assert!(user.contains("Tags in the library: uçak, belgesel"));
        assert!(user.contains("Title: AV-8B Harrier landing"));
        assert!(!user.contains("Description:"));
        assert!(user.contains("Transcript excerpt: vertical take off"));
        let (_, empty) = messages(&s, &[]);
        assert!(empty.contains("no tags yet"));
    }

    #[test]
    fn a_remote_api_needs_agreement_and_the_key_stays_hidden() {
        let remote = "https://api.example.com/v1";
        assert_eq!(
            error_code(&ChatApi::new(remote, "m", None, false, false).unwrap_err()),
            ErrorCode::UrlBlocked
        );
        assert_eq!(
            error_code(&ChatApi::new(remote, "m", None, true, true).unwrap_err()),
            ErrorCode::OfflineMode
        );
        let a = ChatApi::new(
            "http://localhost:11434/v1",
            "qwen3.5:4b",
            Some("s3cret".into()),
            false,
            true,
        )
        .unwrap();
        assert!(!format!("{a:?}").contains("s3cret"));
        assert!(a
            .body("s", "u", true)
            .contains("\"reasoning_effort\":\"none\""));
        assert!(!a.body("s", "u", false).contains("reasoning_effort"));
    }
}
