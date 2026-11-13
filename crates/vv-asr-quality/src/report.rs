//! Reads the app's library (read-only) and writes the F6.7 report as
//! Markdown: for one video, every local transcript scored against the best
//! reference there is, with the run time of the job that made it.

use crate::{
    punctuation_per_100_words, score, term_count, words_of, words_of_text, Score, Timed, Word,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::fmt::Write as _;
use vv_persistence::transcripts::{self, CaptionKind, Provenance, Status, Transcript};

pub type Error = Box<dyn std::error::Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub duration_ms: Option<i64>,
    /// Local (whisper.cpp) transcripts it has.
    pub local: usize,
}

impl Item {
    pub fn name(&self) -> &str {
        self.title
            .as_deref()
            .or(self.url.as_deref())
            .unwrap_or(&self.id)
    }
}

/// Items with at least one local transcript, most recently transcribed first.
pub fn items_with_local(c: &Connection) -> Result<Vec<Item>, Error> {
    let mut st = c.prepare(
        "SELECT i.id, i.title, i.canonical_url, i.duration_ms, count(t.id), max(t.updated_at) \
         FROM library_item i JOIN transcript t ON t.item_id = i.id \
         WHERE t.provenance = 'generated_local' \
         GROUP BY i.id ORDER BY max(t.updated_at) DESC, i.id",
    )?;
    let rows = st.query_map([], |r| {
        Ok(Item {
            id: r.get(0)?,
            title: r.get(1)?,
            url: r.get(2)?,
            duration_ms: r.get(3)?,
            local: r.get::<_, i64>(4)? as usize,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The item `query` names: its id (or its first 6+ characters), or a piece
/// of its address or title
/// (case-insensitive), among items with local transcripts.
pub fn find_item(c: &Connection, query: &str) -> Result<Item, Error> {
    let all = items_with_local(c)?;
    if let Some(i) = all.iter().find(|i| i.id == query) {
        return Ok(i.clone());
    }
    let q = query.to_lowercase();
    let hits: Vec<&Item> = all
        .iter()
        .filter(|i| {
            [&i.url, &i.title]
                .iter()
                .any(|f| f.as_deref().is_some_and(|s| s.to_lowercase().contains(&q)))
                || (q.len() >= 6 && i.id.starts_with(&q))
        })
        .collect();
    match hits.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(format!("no item with local transcripts matches “{query}”").into()),
        many => Err(format!(
            "“{query}” matches {} items; use an id:\n{}",
            many.len(),
            many.iter()
                .map(|i| format!("  {}  {}", i.id, i.name()))
                .collect::<Vec<_>>()
                .join("\n")
        )
        .into()),
    }
}

/// Smallest model first; unknown ids after the known ones.
fn model_rank(id: &str) -> usize {
    [
        "whisper-tiny",
        "whisper-base",
        "whisper-small",
        "whisper-medium",
        "whisper-large",
    ]
    .iter()
    .position(|m| id.starts_with(m))
    .unwrap_or(99)
}

/// `whisper-medium` → `Medium`.
pub fn model_label(id: &str) -> String {
    let s = id.strip_prefix("whisper-").unwrap_or(id);
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// The model a transcription job's payload names (`modelId`, the engine's
/// camelCase form; `model_id` read too).
fn payload_model(p: &serde_json::Value) -> Option<&str> {
    p["modelId"].as_str().or_else(|| p["model_id"].as_str())
}

/// Run time of the newest finished transcription job for this model and
/// language: the sum of its attempts (a job resumed after the app closed
/// has several), and how many attempts.
pub fn run_time(
    c: &Connection,
    item_id: &str,
    model_id: &str,
    language: &str,
) -> Result<Option<(i64, usize)>, Error> {
    let mut st = c.prepare(
        "SELECT id, payload FROM job WHERE type = 'transcription' AND item_id = ?1 \
         AND status = 'succeeded' ORDER BY finished_at DESC, id",
    )?;
    let jobs: Vec<(String, String)> = st
        .query_map([item_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, payload) in jobs {
        let p: serde_json::Value = serde_json::from_str(&payload).unwrap_or_default();
        let lang = p["language"].as_str().unwrap_or("");
        if payload_model(&p) != Some(model_id) || (lang != language && lang != "auto") {
            continue;
        }
        let row: Option<(i64, i64)> = c
            .query_row(
                "SELECT count(*), sum(ended_at - started_at) FROM job_attempt \
                 WHERE job_id = ?1 AND ended_at IS NOT NULL",
                params![id],
                |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
            )
            .optional()?;
        if let Some((n, ms)) = row.filter(|r| r.0 > 0) {
            return Ok(Some((ms, n as usize)));
        }
    }
    Ok(None)
}

/// What the transcripts are compared with.
#[derive(Debug, Clone)]
pub enum RefKind {
    /// The uploader's own captions: the closest thing to the truth.
    Manual,
    /// The site's automatic captions: another recognizer, so the numbers
    /// say how much the two agree, not how right either is.
    Automatic,
    /// A text file the user checked (`--reference`).
    File(String),
    /// No captions: the largest model's transcript, agreement only.
    Model(String),
}

impl RefKind {
    fn describe(&self) -> String {
        match self {
            RefKind::Manual => "yükleyicinin kendi altyazısı (en iyi referans: sayılar hatayı ölçer)".into(),
            RefKind::Automatic => "sitenin otomatik altyazısı (o da bir tanıyıcı: sayılar doğruluğu değil iki tanıyıcının uyuşmasını ölçer)".into(),
            RefKind::File(p) => format!("elle denetlenmiş metin dosyası `{p}` (zamanı yok: pencereleme gerekirse kelimeler süreye eşit yayılır)"),
            RefKind::Model(m) => format!("altyazı yok; en büyük model ({m}) referans alındı (yalnız modellerin uyuşması, doğruluk değil)"),
        }
    }
}

pub struct Reference {
    pub kind: RefKind,
    pub language: String,
    pub words: Vec<Word>,
    pub raw: String,
}

pub struct Options {
    /// `--reference FILE`: its text and the language to read it with.
    pub reference_file: Option<(String, String)>,
    pub reference_path: Option<String>,
    pub terms: Vec<String>,
}

fn segs_of(c: &Connection, t: &Transcript) -> Result<(Vec<Timed>, String), Error> {
    let segs: Vec<Timed> = transcripts::segments(c, &t.id)?
        .into_iter()
        .map(|s| (s.start_ms, s.end_ms, s.text))
        .collect();
    let raw = segs
        .iter()
        .map(|s| s.2.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    Ok((segs, raw))
}

fn usable(t: &Transcript) -> bool {
    t.segment_count > 0 && matches!(t.status, Status::Ready | Status::Partial | Status::Stale)
}

fn pct(v: Option<f64>) -> String {
    v.map_or("—".into(), |v| format!("{:.1} %", v * 100.0))
}

pub fn clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Table cells can't hold `|` or line breaks.
fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace(['\n', '\r'], " ")
}

struct Row {
    t: Transcript,
    score: Score,
    punct: Option<f64>,
    run: Option<(i64, usize)>,
    words: Vec<Word>,
    note: Vec<String>,
}

/// The report section for one item.
pub fn item_report(c: &Connection, item: &Item, opts: &Options) -> Result<String, Error> {
    let all = transcripts::list_for_item(c, &item.id)?;
    let mut local: Vec<Transcript> = all
        .iter()
        .filter(|t| t.origin.provenance == Provenance::GeneratedLocal && usable(t))
        .cloned()
        .collect();
    local.sort_by_key(|t| {
        (
            model_rank(t.origin.model_id.as_deref().unwrap_or("")),
            t.origin.language.clone(),
        )
    });
    let mut out = String::new();
    writeln!(out, "## {}\n", cell(item.name()))?;
    if local.is_empty() {
        writeln!(out, "Bu öğede kullanılabilir yerel döküm yok.\n")?;
        return Ok(out);
    }
    let duration = item
        .duration_ms
        .or_else(|| all.iter().filter_map(|t| t.covered_ms).max());

    let caption = |kind: CaptionKind| {
        all.iter().find(|t| {
            t.origin.provenance == Provenance::SourceCaptions
                && t.origin.caption_kind == Some(kind)
                && usable(t)
        })
    };
    // The model transcript used as reference, if it came to that.
    let mut ref_model: Option<Transcript> = None;
    let reference = if let Some((text, lang)) = &opts.reference_file {
        Reference {
            kind: RefKind::File(opts.reference_path.clone().unwrap_or_default()),
            language: lang.clone(),
            words: words_of_text(text, lang, duration.unwrap_or(0)),
            raw: text.clone(),
        }
    } else if let Some(t) = caption(CaptionKind::Manual).or_else(|| caption(CaptionKind::Automatic))
    {
        let (segs, raw) = segs_of(c, t)?;
        Reference {
            kind: if t.origin.caption_kind == Some(CaptionKind::Manual) {
                RefKind::Manual
            } else {
                RefKind::Automatic
            },
            language: t.origin.language.clone(),
            words: words_of(&segs, &t.origin.language),
            raw,
        }
    } else {
        // Largest model, ready ones before partial.
        let best = local
            .iter()
            .max_by_key(|t| {
                (
                    t.status == Status::Ready,
                    model_rank(t.origin.model_id.as_deref().unwrap_or("")),
                )
            })
            .cloned()
            .ok_or("no local transcript")?;
        let (segs, raw) = segs_of(c, &best)?;
        local.retain(|t| t.id != best.id);
        ref_model = Some(best.clone());
        Reference {
            kind: RefKind::Model(model_label(best.origin.model_id.as_deref().unwrap_or("?"))),
            language: best.origin.language.clone(),
            words: words_of(&segs, &best.origin.language),
            raw,
        }
    };

    let mut rows = Vec::new();
    for t in local {
        let (segs, raw) = segs_of(c, &t)?;
        // Scored with the reference's language rules, so `I`/`ı` match.
        let words = words_of(&segs, &reference.language);
        let mut note = Vec::new();
        let base: Vec<Word> = match (t.status, t.covered_ms) {
            (Status::Partial, Some(cov)) => {
                note.push(format!("ilk {}", clock(cov)));
                reference
                    .words
                    .iter()
                    .filter(|w| w.ms < cov)
                    .cloned()
                    .collect()
            }
            _ => reference.words.clone(),
        };
        if t.origin.language != reference.language {
            note.push(format!(
                "dil {} (referans {})",
                t.origin.language, reference.language
            ));
        }
        let run = run_time(
            c,
            &item.id,
            t.origin.model_id.as_deref().unwrap_or(""),
            &t.origin.language,
        )?;
        if let Some((_, n)) = run.filter(|r| r.1 > 1) {
            note.push(format!("{n} denemede"));
        }
        rows.push(Row {
            score: score(&base, &words),
            punct: punctuation_per_100_words(&raw, words.len()),
            run,
            words,
            t,
            note,
        });
    }

    writeln!(
        out,
        "- Öğe: {} (`{}`){}",
        item.url
            .as_deref()
            .map_or("yerel dosya".into(), |u| format!("<{u}>")),
        item.id,
        duration.map_or(String::new(), |d| format!(", süre {}", clock(d)))
    )?;
    writeln!(
        out,
        "- Referans: {}; dil `{}`, {} kelime, 100 kelimede {} noktalama işareti.",
        reference.kind.describe(),
        reference.language,
        reference.words.len(),
        punctuation_per_100_words(&reference.raw, reference.words.len())
            .map_or("—".into(), |p| format!("{p:.1}"))
    )?;
    if let Some(t) = &ref_model {
        let model = t.origin.model_id.as_deref().unwrap_or("");
        let run = run_time(c, &item.id, model, &t.origin.language)?;
        writeln!(
            out,
            "- {} (referans) süresi: {}",
            model_label(model),
            match (run, duration) {
                (Some((ms, _)), Some(d)) if ms > 0 => format!(
                    "{}, gerçek zamanın {:.1} katı.",
                    clock(ms),
                    d as f64 / ms as f64
                ),
                (Some((ms, _)), _) => format!("{}.", clock(ms)),
                _ => "bilinmiyor (işi bulunamadı).".into(),
            }
        )?;
    }
    if rows.is_empty() {
        writeln!(
            out,
            "\n**Karşılaştırılacak döküm yok.** Bu öğede site altyazısı yok ve yalnız bir model ({}) ile yazıya dökülmüş. \
             Details → Transcript'te **Get site captions** ile altyazıyı al ve/veya videoyu başka modellerle (Tiny, Base, Small) de yazıya dök, sonra raporu yeniden çalıştır.\n",
            ref_model
                .as_ref()
                .map_or("?".into(), |t| model_label(t.origin.model_id.as_deref().unwrap_or("?")))
        )?;
        return Ok(out);
    }
    if rows.iter().any(|r| r.score.windowed) {
        writeln!(out, "- Uzun metin: 5 dakikalık pencerelerle hizalandı (pencere sınırındaki birkaç kelime iki hata sayılabilir).")?;
    }
    writeln!(out)?;
    writeln!(out, "| Model | Sürüm | Durum | Kelime | WER | WER, ç ğ ı ö ş ü hariç | CER | Noktalama / 100 kelime | Süre | Gerçek zamanın katı |")?;
    writeln!(out, "|---|---|---|---|---|---|---|---|---|---|")?;
    for r in &rows {
        let mut status = r.t.status.as_str().to_owned();
        if !r.note.is_empty() {
            status = format!("{status} ({})", r.note.join(", "));
        }
        let speed = match (r.run, duration) {
            (Some((ms, _)), Some(d)) if ms > 0 => {
                let media = match (r.t.status, r.t.covered_ms) {
                    (Status::Partial, Some(cov)) => cov,
                    _ => d,
                };
                format!("{:.1}×", media as f64 / ms as f64)
            }
            _ => "—".into(),
        };
        writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            model_label(r.t.origin.model_id.as_deref().unwrap_or("?")),
            cell(r.t.origin.model_version.as_deref().unwrap_or("—")),
            status,
            r.score.hyp_words,
            pct(r.score.wer()),
            pct(r.score.wer_folded()),
            pct(r.score.cer()),
            r.punct.map_or("—".into(), |p| format!("{p:.1}")),
            r.run.map_or("—".into(), |(ms, _)| clock(ms)),
            speed
        )?;
    }
    writeln!(out)?;
    writeln!(
        out,
        "WER: kelime hatası (yerine geçen + eksik + fazla) / referans kelime sayısı. CER: aynısı harf düzeyinde. \
         “ç ğ ı ö ş ü hariç”: yalnız Türkçe harfi yanlış olan kelimeler hata sayılmazsa. \
         Süre: işin denemelerinin toplamı (uygulamanın kapalı kaldığı süre sayılmaz). \
         Gerçek zamanın katı: medya süresi / iş süresi.\n"
    )?;

    if !opts.terms.is_empty() {
        writeln!(out, "### Terimler\n")?;
        let mut head = "| Terim | Referans |".to_owned();
        let mut sep = "|---|---|".to_owned();
        for r in &rows {
            head.push_str(&format!(
                " {} |",
                model_label(r.t.origin.model_id.as_deref().unwrap_or("?"))
            ));
            sep.push_str("---|");
        }
        writeln!(out, "{head}\n{sep}")?;
        for term in &opts.terms {
            let mut line = format!(
                "| {} | {} |",
                cell(term),
                term_count(&reference.words, term, &reference.language)
            );
            for r in &rows {
                line.push_str(&format!(
                    " {} |",
                    term_count(&r.words, term, &reference.language)
                ));
            }
            writeln!(out, "{line}")?;
        }
        writeln!(out, "\nKaç kez geçtiği: büyük/küçük harf ve kesme işareti fark etmez, son kelimeye ek gelebilir (`Kadıköy` → `Kadıköy'de` de sayılır).\n")?;
    }

    writeln!(out, "### En sık karışan kelimeler\n")?;
    for r in &rows {
        let pairs = r
            .score
            .top_substitutions
            .iter()
            .map(|(a, b, n)| format!("`{}` → `{}` ×{n}", cell(a), cell(b)))
            .collect::<Vec<_>>();
        writeln!(
            out,
            "- {}: {}",
            model_label(r.t.origin.model_id.as_deref().unwrap_or("?")),
            if pairs.is_empty() {
                "yok".into()
            } else {
                pairs.join(", ")
            }
        )?;
    }
    writeln!(out)?;
    Ok(out)
}

/// A transcription job that ended in an error: what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedJob {
    pub item: String,
    pub model_id: String,
    pub error_code: String,
    pub message: Option<String>,
    pub finished_at: i64,
}

/// The newest failed transcription jobs (at most `limit`): `asr:report list`
/// shows them, so a run that failed in the app can be told apart without
/// opening the database.
pub fn failed_jobs(c: &Connection, limit: usize) -> Result<Vec<FailedJob>, Error> {
    let mut st = c.prepare(
        "SELECT ifnull(i.title, ifnull(i.canonical_url, j.item_id)), j.payload, \
             ifnull(j.error_code, '?'), j.redacted_error_message, ifnull(j.finished_at, j.updated_at) \
         FROM job j LEFT JOIN library_item i ON i.id = j.item_id \
         WHERE j.type = 'transcription' AND j.status = 'failed' \
         ORDER BY ifnull(j.finished_at, j.updated_at) DESC, j.id LIMIT ?1",
    )?;
    let rows = st.query_map([limit as i64], |r| {
        let payload: String = r.get(1)?;
        let p: serde_json::Value = serde_json::from_str(&payload).unwrap_or_default();
        Ok(FailedJob {
            item: r.get(0)?,
            model_id: payload_model(&p).unwrap_or("?").to_owned(),
            error_code: r.get(2)?,
            message: r.get(3)?,
            finished_at: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
