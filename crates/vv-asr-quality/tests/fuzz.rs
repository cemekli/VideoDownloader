//! Fuzz tests for the report's text handling (docs/testing.md "Fuzz
//! testleri"): transcript and caption text comes from whisper.cpp and from
//! sites, so [`normalize`] and [`score`] get Turkish letters, combining
//! marks, emoji, stray brackets and control characters.

use vv_asr_quality::{fold, normalize, punctuation_per_100_words, score, term_count, words_of};
use vv_fuzz::{compose, mutate_str, run, NASTY};

const PIECES: &[&str] = &[
    " ",
    "  ",
    "\n",
    "Kadıköy'de",
    "IŞIK",
    "İzmir",
    "I\u{307}",
    "don’t",
    "[Müzik]",
    "(alkış)",
    "[BLANK_AUDIO]",
    "<i>",
    "</i>",
    "<",
    ">",
    "(",
    ")",
    "[",
    "]",
    ">>",
    "…",
    ".",
    ",",
    "?",
    "25 ₺",
    "çok",
    "cok",
    "ışık",
    "isik",
    "ğ",
    "'",
];

const SAMPLE: &str =
    ">> Merhaba arkadaşlar, bugün İstanbul'dayız. [Müzik] Iğdır, ılık, IŞIK… (alkış)";

#[test]
fn normalize_never_panics_and_yields_clean_words() {
    run(
        "asr/normalize",
        |r| {
            let mut pieces: Vec<&str> = PIECES.to_vec();
            pieces.extend_from_slice(NASTY);
            if r.below(2) == 0 {
                compose(r, &pieces, 30)
            } else {
                mutate_str(r, SAMPLE, &pieces)
            }
        },
        |s| {
            for lang in ["tr", "en", "tr-TR"] {
                let ws = normalize(s, lang);
                for w in &ws {
                    assert!(
                        !w.is_empty() && w.chars().all(char::is_alphanumeric),
                        "{w:?} from {s:?}"
                    );
                    assert!(!w.contains('\u{307}'));
                }
                let timed = words_of(&[(0, 5_000, s.clone())], lang);
                let sc = score(&timed, &timed);
                assert_eq!(sc.word_errors(), 0);
                assert_eq!(sc.char_edits, 0);
                assert!(punctuation_per_100_words(s, ws.len()).is_none_or(|p| p >= 0.0));
                for w in &ws {
                    assert!(term_count(&timed, w, lang) >= 1);
                    let _ = fold(w);
                }
            }
        },
    );
}

#[test]
fn score_is_symmetric_in_edit_count() {
    run(
        "asr/score",
        |r| {
            let a = mutate_str(r, SAMPLE, PIECES);
            let b = mutate_str(r, SAMPLE, PIECES);
            (a, b)
        },
        |(a, b)| {
            let wa = words_of(&[(0, 9_000, a.clone())], "tr");
            let wb = words_of(&[(0, 9_000, b.clone())], "tr");
            let ab = score(&wa, &wb);
            let ba = score(&wb, &wa);
            assert_eq!(ab.word_errors(), ba.word_errors());
            assert!(ab.substitutions + ab.deletions <= wa.len());
            assert!(ab.substitutions + ab.insertions <= wb.len());
            assert!(ab.diacritic_only <= ab.substitutions);
            assert!(ab.word_errors() >= wa.len().abs_diff(wb.len()));
        },
    );
}
