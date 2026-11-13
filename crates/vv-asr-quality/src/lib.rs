//! Transcription quality measures for the F6.7 report (SPEC AI-A07, AC-21,
//! ADR-0034).
//!
//! A local transcript (whisper.cpp) is compared with a reference: the
//! uploader's own captions, the site's automatic captions, a text file the
//! user checked, or (when there is nothing else) the largest model's
//! transcript. The numbers are the usual ones:
//!
//! - **WER**: word edits (substitutions + deletions + insertions) over the
//!   reference's word count, after [`normalize`];
//! - **CER**: the same over characters, which forgives a wrong suffix
//!   (`Kadıköy'de` / `Kadıköyden`) more than WER does;
//! - **diacritics only**: substitutions that are the same word once
//!   `ç ğ ı ö ş ü` are folded to ASCII (`isik` for `ışık`), the Turkish
//!   errors a reader still understands but search misses;
//! - **terms**: how often given names (`Kadıköy`, `PostgreSQL`) appear in
//!   the reference and in the transcript.
//!
//! Nothing here claims an accuracy figure for the app (AI-A07): the report
//! says what was measured, on which video, against which reference.

use std::collections::HashMap;

pub mod report;

/// One normalized word and the media time it was said at (approximately:
/// spread evenly over its segment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub ms: i64,
}

/// Lowercase with the language's rules. Turkish has dotted and dotless i:
/// `I` → `ı`, `İ` → `i`; other languages map `I` → `i`. A combining dot
/// above (NFD `İ`, or what `to_lowercase` makes of `İ`) is dropped.
fn lower(s: &str, turkish: bool) -> String {
    let mut out = String::with_capacity(s.len());
    // NFD İ is I + combining dot: dotted, whatever the language.
    let s = s.replace("I\u{307}", "İ");
    for c in s.chars() {
        match c {
            'I' if turkish => out.push('ı'),
            'İ' => out.push('i'),
            _ => out.extend(c.to_lowercase()),
        }
    }
    out.replace('\u{307}', "")
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’' | 'ʼ' | '`' | '´')
}

/// Drops text in `[…]`, `(…)`, `{…}` and `<…>`: sound labels such as
/// `[Müzik]`, `(applause)`, `[BLANK_AUDIO]` and leftover markup. Not words
/// anyone said, and the two sides label them differently. A bracket never
/// closed (`a < b`) is not a label: its text is kept.
fn strip_labels(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut close: Vec<char> = Vec::new();
    let mut held = String::new();
    for c in s.chars() {
        let opener = match c {
            '[' => Some(']'),
            '(' => Some(')'),
            '{' => Some('}'),
            '<' => Some('>'),
            _ => None,
        };
        if let Some(o) = opener {
            close.push(o);
            held.push(' ');
        } else if close.last() == Some(&c) {
            close.pop();
            if close.is_empty() {
                held.clear();
                out.push(' ');
            }
        } else if close.is_empty() {
            out.push(c);
        } else {
            held.push(c);
        }
    }
    out.push_str(&held);
    out
}

/// The words of `text` as they are scored: labels dropped, lowercased for
/// `language`, apostrophes joined (`Kadıköy'de` → `kadıköyde`, `don't` →
/// `dont`, so both spellings meet), every other non-letter, non-digit a
/// word break.
pub fn normalize(text: &str, language: &str) -> Vec<String> {
    let turkish = is_turkish(language);
    let lowered = lower(&strip_labels(text), turkish);
    let mut words = Vec::new();
    let mut cur = String::new();
    for c in lowered.chars() {
        if c.is_alphanumeric() {
            cur.push(c);
        } else if is_apostrophe(c) && !cur.is_empty() {
            // Joined: the next letters continue this word.
        } else if !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

pub fn is_turkish(language: &str) -> bool {
    language == "tr" || language.starts_with("tr-")
}

/// Turkish letters folded to ASCII (`ışık` → `isik`): what a search typed on
/// an English keyboard would find.
pub fn fold(word: &str) -> String {
    word.chars()
        .map(|c| match c {
            'ç' => 'c',
            'ğ' => 'g',
            'ı' => 'i',
            'ö' => 'o',
            'ş' => 's',
            'ü' => 'u',
            'â' => 'a',
            'î' => 'i',
            'û' => 'u',
            _ => c,
        })
        .collect()
}

/// A timed segment: `(start_ms, end_ms, text)`.
pub type Timed = (i64, i64, String);

/// The words of timed segments, each word given a time spread evenly over
/// its segment.
pub fn words_of(segments: &[Timed], language: &str) -> Vec<Word> {
    let mut out = Vec::new();
    for (start, end, text) in segments {
        let ws = normalize(text, language);
        let n = ws.len() as i64;
        let span = (end - start).max(0);
        for (i, w) in ws.into_iter().enumerate() {
            out.push(Word {
                text: w,
                ms: start + span * i as i64 / n.max(1),
            });
        }
    }
    out
}

/// Plain text with no times (a reference file): words spread evenly over
/// `duration_ms`.
pub fn words_of_text(text: &str, language: &str, duration_ms: i64) -> Vec<Word> {
    let ws = normalize(text, language);
    let n = ws.len() as i64;
    ws.into_iter()
        .enumerate()
        .map(|(i, w)| Word {
            text: w,
            ms: duration_ms.max(0) * i as i64 / n.max(1),
        })
        .collect()
}

/// Sentence punctuation marks (`.`, `,`, `?`, `!`, `…`, `;`, `:`) per 100
/// words of `text`. Automatic captions have none; whisper writes them.
pub fn punctuation_per_100_words(text: &str, words: usize) -> Option<f64> {
    if words == 0 {
        return None;
    }
    let marks = strip_labels(text)
        .chars()
        .filter(|c| matches!(c, '.' | ',' | '?' | '!' | '…' | ';' | ':'))
        .count();
    Some(marks as f64 * 100.0 / words as f64)
}

/// One step of a word alignment: indexes into the reference (`r`) and the
/// hypothesis (`h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Match(usize, usize),
    Sub(usize, usize),
    Del(usize),
    Ins(usize),
}

/// A minimum-edit alignment of `r` and `h` (Levenshtein with a backtrace).
/// Memory is `(r.len() + 1) × (h.len() + 1)` bytes: callers keep that
/// bounded (see [`MAX_CELLS`]).
pub fn align<T: PartialEq>(r: &[T], h: &[T]) -> Vec<Op> {
    let (n, m) = (r.len(), h.len());
    let w = m + 1;
    // 0 = diagonal, 1 = up (deletion), 2 = left (insertion).
    let mut dir = vec![0u8; (n + 1) * w];
    let mut prev: Vec<u32> = (0..=m as u32).collect();
    let mut cur = vec![0u32; w];
    for d in dir.iter_mut().take(w).skip(1) {
        *d = 2;
    }
    for i in 1..=n {
        cur[0] = i as u32;
        dir[i * w] = 1;
        for j in 1..=m {
            let diag = prev[j - 1] + u32::from(r[i - 1] != h[j - 1]);
            let up = prev[j] + 1;
            let left = cur[j - 1] + 1;
            let (v, d) = if diag <= up && diag <= left {
                (diag, 0)
            } else if up <= left {
                (up, 1)
            } else {
                (left, 2)
            };
            cur[j] = v;
            dir[i * w + j] = d;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let mut ops = Vec::with_capacity(n.max(m));
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        match dir[i * w + j] {
            0 => {
                i -= 1;
                j -= 1;
                ops.push(if r[i] == h[j] {
                    Op::Match(i, j)
                } else {
                    Op::Sub(i, j)
                });
            }
            1 => {
                i -= 1;
                ops.push(Op::Del(i));
            }
            _ => {
                j -= 1;
                ops.push(Op::Ins(j));
            }
        }
    }
    ops.reverse();
    ops
}

/// Edit distance only, in two rows of memory (used for characters).
pub fn distance<T: PartialEq>(r: &[T], h: &[T]) -> usize {
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    let mut cur = vec![0usize; h.len() + 1];
    for (i, a) in r.iter().enumerate() {
        cur[0] = i + 1;
        for (j, b) in h.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(a != b))
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[h.len()]
}

/// Above this many alignment cells (reference words × transcript words,
/// about 5 000 × 5 000 or 35–40 minutes of speech) the texts are aligned in
/// [`WINDOW_MS`] pieces instead, so a two-hour video does not need gigabytes.
pub const MAX_CELLS: usize = 25_000_000;
/// Window length for long texts. A word said right at a window edge may land
/// in different windows on the two sides and count as two errors; with
/// 5-minute windows that is a few words in thousands.
pub const WINDOW_MS: i64 = 300_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub ref_words: usize,
    pub hyp_words: usize,
    pub substitutions: usize,
    pub deletions: usize,
    pub insertions: usize,
    /// Substitutions that are the same word with Turkish letters folded.
    pub diacritic_only: usize,
    pub ref_chars: usize,
    pub char_edits: usize,
    /// Aligned in [`WINDOW_MS`] pieces (long texts).
    pub windowed: bool,
    /// Most frequent `(reference word, transcript word)` substitutions.
    pub top_substitutions: Vec<(String, String, usize)>,
}

impl Score {
    pub fn word_errors(&self) -> usize {
        self.substitutions + self.deletions + self.insertions
    }
    pub fn wer(&self) -> Option<f64> {
        ratio(self.word_errors(), self.ref_words)
    }
    /// WER when `ç ğ ı ö ş ü` errors are forgiven.
    pub fn wer_folded(&self) -> Option<f64> {
        ratio(self.word_errors() - self.diacritic_only, self.ref_words)
    }
    pub fn cer(&self) -> Option<f64> {
        ratio(self.char_edits, self.ref_chars)
    }
}

fn ratio(a: usize, b: usize) -> Option<f64> {
    (b > 0).then(|| a as f64 / b as f64)
}

/// How many substitution pairs [`Score::top_substitutions`] keeps.
pub const TOP_SUBSTITUTIONS: usize = 10;

fn windows(words: &[Word]) -> Vec<&[Word]> {
    let mut out: Vec<&[Word]> = Vec::new();
    let mut start = 0;
    for i in 1..=words.len() {
        let edge = i == words.len() || words[i].ms / WINDOW_MS != words[start].ms / WINDOW_MS;
        if edge {
            out.push(&words[start..i]);
            start = i;
        }
    }
    out
}

/// The window index → words of that window (windows with no words on one
/// side still count the other side's words).
fn by_window(words: &[Word]) -> HashMap<i64, &[Word]> {
    windows(words)
        .into_iter()
        .filter_map(|w| w.first().map(|f| (f.ms / WINDOW_MS, w)))
        .collect()
}

/// Scores `hyp` against `reference` (both from [`words_of`]).
pub fn score(reference: &[Word], hyp: &[Word]) -> Score {
    let windowed = reference.len().saturating_mul(hyp.len()) > MAX_CELLS;
    let pieces: Vec<(&[Word], &[Word])> = if windowed {
        let (rw, hw) = (by_window(reference), by_window(hyp));
        let mut keys: Vec<i64> = rw.keys().chain(hw.keys()).copied().collect();
        keys.sort_unstable();
        keys.dedup();
        keys.into_iter()
            .map(|k| {
                (
                    rw.get(&k).copied().unwrap_or(&[]),
                    hw.get(&k).copied().unwrap_or(&[]),
                )
            })
            .collect()
    } else {
        vec![(reference, hyp)]
    };
    let mut s = Score {
        ref_words: reference.len(),
        hyp_words: hyp.len(),
        substitutions: 0,
        deletions: 0,
        insertions: 0,
        diacritic_only: 0,
        ref_chars: 0,
        char_edits: 0,
        windowed,
        top_substitutions: Vec::new(),
    };
    let mut pairs: HashMap<(String, String), usize> = HashMap::new();
    for (r, h) in pieces {
        let rt: Vec<&str> = r.iter().map(|w| w.text.as_str()).collect();
        let ht: Vec<&str> = h.iter().map(|w| w.text.as_str()).collect();
        for op in align(&rt, &ht) {
            match op {
                Op::Match(..) => {}
                Op::Sub(i, j) => {
                    s.substitutions += 1;
                    if fold(rt[i]) == fold(ht[j]) {
                        s.diacritic_only += 1;
                    }
                    *pairs
                        .entry((rt[i].to_owned(), ht[j].to_owned()))
                        .or_default() += 1;
                }
                Op::Del(_) => s.deletions += 1,
                Op::Ins(_) => s.insertions += 1,
            }
        }
        let rc: Vec<char> = rt.join(" ").chars().collect();
        let hc: Vec<char> = ht.join(" ").chars().collect();
        s.ref_chars += rc.len();
        s.char_edits += distance(&rc, &hc);
    }
    let mut top: Vec<(String, String, usize)> =
        pairs.into_iter().map(|((a, b), n)| (a, b, n)).collect();
    top.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| (&a.0, &a.1).cmp(&(&b.0, &b.1))));
    top.truncate(TOP_SUBSTITUTIONS);
    s.top_substitutions = top;
    s
}

/// How many times `term` (one or more words) occurs in `words`, matched
/// after [`normalize`] with `language`. The last word may carry a suffix:
/// `Kadıköy` counts `Kadıköy'de` (`kadıköyde`) too, since Turkish names
/// rarely appear bare.
pub fn term_count(words: &[Word], term: &str, language: &str) -> usize {
    let t = normalize(term, language);
    if t.is_empty() || t.len() > words.len() {
        return 0;
    }
    words
        .windows(t.len())
        .filter(|w| {
            let last = t.len() - 1;
            w.iter().zip(&t).enumerate().all(|(i, (a, b))| {
                if i == last {
                    a.text.starts_with(b.as_str())
                } else {
                    &a.text == b
                }
            })
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str, lang: &str) -> Vec<Word> {
        words_of(&[(0, 10_000, s.to_owned())], lang)
    }

    #[test]
    fn turkish_lowercase_keeps_dotless_i() {
        assert_eq!(normalize("IŞIK İstanbul", "tr"), ["ışık", "istanbul"]);
        assert_eq!(normalize("I think", "en"), ["i", "think"]);
        // NFD İ (I + combining dot) and to_lowercase's i̇.
        assert_eq!(normalize("I\u{307}zmir", "tr"), ["izmir"]);
        assert_eq!(normalize("İzmir", "en"), ["izmir"]);
    }

    #[test]
    fn apostrophes_join_and_labels_go() {
        assert_eq!(
            normalize("[Müzik] Kadıköy'de, (alkış) don’t >> stop… <b>x</b>", "tr"),
            ["kadıköyde", "dont", "stop", "x"]
        );
        assert_eq!(
            normalize("'tırnak' ve-tire", "tr"),
            ["tırnak", "ve", "tire"]
        );
        assert!(normalize("[BLANK_AUDIO]", "en").is_empty());
        // Never closed: not a label.
        assert_eq!(normalize("a < b (c", "en"), ["a", "b", "c"]);
    }

    #[test]
    fn wer_counts_each_kind_of_edit() {
        let r = w("bugün hava çok güzel", "tr");
        let h = w("bugün hava cok güzel mi", "tr");
        let s = score(&r, &h);
        assert_eq!((s.substitutions, s.deletions, s.insertions), (1, 0, 1));
        assert_eq!(s.diacritic_only, 1);
        assert_eq!(s.wer(), Some(0.5));
        assert_eq!(s.wer_folded(), Some(0.25));
        assert_eq!(s.top_substitutions, [("çok".into(), "cok".into(), 1)]);
        let d = score(&r, &w("hava güzel", "tr"));
        assert_eq!((d.substitutions, d.deletions, d.insertions), (0, 2, 0));
    }

    #[test]
    fn cer_is_lower_than_wer_for_suffix_errors() {
        let s = score(
            &w("kadıköyde buluştuk", "tr"),
            &w("kadıköyden buluştuk", "tr"),
        );
        assert_eq!(s.wer(), Some(0.5));
        assert_eq!(s.char_edits, 1);
        assert!(s.cer().unwrap() < 0.1);
    }

    #[test]
    fn empty_sides() {
        assert_eq!(score(&[], &[]).wer(), None);
        let s = score(&w("bir iki", "tr"), &[]);
        assert_eq!((s.deletions, s.wer()), (2, Some(1.0)));
        let s = score(&[], &w("bir", "tr"));
        assert_eq!((s.insertions, s.wer()), (1, None));
    }

    #[test]
    fn long_texts_align_in_windows_with_the_same_result_for_equal_text() {
        // 6 000 words over 40 minutes on both sides, one word changed.
        let segs: Vec<(i64, i64, String)> = (0..600)
            .map(|i| {
                (
                    i * 4_000,
                    i * 4_000 + 4_000,
                    format!("söz{} bir iki üç dört beş altı yedi sekiz dokuz", i),
                )
            })
            .collect();
        let r = words_of(&segs, "tr");
        let mut segs2 = segs.clone();
        segs2[300].2 = segs2[300].2.replace("beş", "bes");
        let h = words_of(&segs2, "tr");
        let s = score(&r, &h);
        assert!(s.windowed);
        assert_eq!(
            (s.substitutions, s.deletions, s.insertions, s.diacritic_only),
            (1, 0, 0, 1)
        );
        assert_eq!(s.char_edits, 1);
    }

    #[test]
    fn terms_match_normalized_word_runs() {
        let words = w(
            "Dün Kadıköy'de PostgreSQL replikasyonu ve postgresql konuştuk, Kadıköy güzel",
            "tr",
        );
        assert_eq!(term_count(&words, "PostgreSQL", "tr"), 2);
        assert_eq!(term_count(&words, "Kadıköy", "tr"), 2);
        assert_eq!(term_count(&words, "Kadıköy'de", "tr"), 1);
        assert_eq!(term_count(&words, "postgresql replikasyonu", "tr"), 1);
        assert_eq!(term_count(&words, "", "tr"), 0);
    }

    #[test]
    fn punctuation_rate() {
        assert_eq!(
            punctuation_per_100_words("Evet. Hayır, belki? [Müzik.]", 4),
            Some(75.0)
        );
        assert_eq!(punctuation_per_100_words("x", 0), None);
    }

    #[test]
    fn word_times_spread_over_segment() {
        let ws = words_of(&[(1_000, 4_000, "bir iki üç".into())], "tr");
        let ms: Vec<i64> = ws.iter().map(|w| w.ms).collect();
        assert_eq!(ms, [1_000, 2_000, 3_000]);
        let ws = words_of_text("bir iki", "tr", 10_000);
        assert_eq!(ws[1].ms, 5_000);
    }
}
