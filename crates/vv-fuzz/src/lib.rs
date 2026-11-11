//! Deterministic fuzzing for parsers that read text from outside the app:
//! yt-dlp output, caption files, cookie files, `profiles.ini` (docs/testing.md
//! "Fuzz testleri"; F6.2 bug of 11 Nov 2026).
//!
//! Hand-written fixtures are short and mostly ASCII, so a byte index that
//! lands inside `ş` or an emoji never shows up in them. This crate makes
//! thousands of inputs from three sources and runs a check on each:
//!
//! - [`mutate_str`] / [`mutate_bytes`]: a real sample (a fixture) with
//!   pieces inserted, cut, repeated or replaced, at character boundaries
//!   (`mutate_str`) or anywhere (`mutate_bytes`, also invalid UTF-8);
//! - [`compose`]: random strings from pieces real files have, plus
//!   [`NASTY`] (Turkish letters, combining marks, emoji, BOM, NUL, CR,
//!   direction marks);
//! - [`prefixes`]: every cut of a sample at a character boundary, as a line
//!   or file cut off mid-write would look.
//!
//! Runs are reproducible: the cases come from a fixed seed (SplitMix64, no
//! crate, same numbers on every OS). `VV_FUZZ_ITERS` sets the number of
//! cases (default [`DEFAULT_ITERS`]); `VV_FUZZ_SEED` picks another seed for a
//! longer local hunt. A failing case prints its number and seed, and
//! `VV_FUZZ_CASE=<n>` runs only that one.

use std::fmt::Debug;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

pub const DEFAULT_ITERS: usize = 2_000;
pub const DEFAULT_SEED: u64 = 0x5652_4655_5a5a_2611; // "VRFUZZ", 26-11

/// Pieces that break code which assumes one byte = one letter, or that
/// text is clean. Every parser gets these mixed into its input.
pub const NASTY: &[&str] = &[
    "ş",
    "Ş",
    "ğ",
    "Ğ",
    "ı",
    "İ",
    "ç",
    "Ç",
    "ö",
    "ü",
    "i\u{307}", // i + combining dot (NFD İ)
    "s\u{327}", // s + combining cedilla (NFD ş)
    "€",
    "…",
    "—",
    "“",
    "”",
    "’",
    "😀",
    "👍🏽",
    "🇹🇷",
    "\u{200d}",
    "\u{feff}",
    "\u{200e}",
    "\u{200f}",
    "\u{202e}",
    "\u{a0}",
    "\u{0}",
    "\r",
    "\r\n",
    "\n",
    "\t",
    " ",
    "&",
    ";",
    "<",
    ">",
    "\"",
    "'",
    "\\",
    "/",
    "%",
    "=",
    "#",
    ":",
    "@",
    "?",
    "&amp;",
    "&gt;&gt;",
    "&#x15f;",
    "&#287;",
    "&#0;",
    "&#xD800;",
    "&#99999999999;",
    "%C5%9F",
    "\\u015f",
    "\\ud83d",
    "NA",
    "null",
    "NaN",
    "Infinity",
    "-1",
    "18446744073709551616",
    "1e309",
    "0.0000001",
];

/// Byte sequences that are not valid UTF-8 (for [`mutate_bytes`]).
pub const INVALID_UTF8: &[&[u8]] = &[
    b"\xff",
    b"\xc5",         // first half of ş
    b"\xe2\x82",     // first two bytes of €
    b"\xf0\x9f\x98", // first three bytes of 😀
    b"\xc0\x80",     // overlong NUL
    b"\xed\xa0\x80", // UTF-16 surrogate
    b"\x80\x80",     // lone continuation bytes
];

/// SplitMix64 (Vigna). Same sequence everywhere; not for anything secret.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `0..n`; `0` when `n == 0`.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// True in `percent` of calls.
    pub fn chance(&mut self, percent: u64) -> bool {
        self.next_u64() % 100 < percent
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

fn env_num(name: &str) -> Option<u64> {
    let v = std::env::var(name).ok()?;
    let v = v.trim();
    match v.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => v.parse().ok(),
    }
}

/// Number of cases per target (`VV_FUZZ_ITERS`).
pub fn iterations() -> usize {
    env_num("VV_FUZZ_ITERS").map_or(DEFAULT_ITERS, |n| n as usize)
}

/// Base seed (`VV_FUZZ_SEED`).
pub fn seed() -> u64 {
    env_num("VV_FUZZ_SEED").unwrap_or(DEFAULT_SEED)
}

/// The seed of case `i` of target `name`: targets do not share sequences.
fn case_seed(name: &str, base: u64, i: usize) -> u64 {
    let mut h = base;
    for b in name.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    Rng::new(h ^ (i as u64).wrapping_mul(0xA24B_AED4_963E_E407)).next_u64()
}

/// Runs `check` on [`iterations`] inputs made by `make`. On a panic, prints
/// the target, case number, seed and input, then fails the test.
pub fn run<I: Debug>(name: &str, mut make: impl FnMut(&mut Rng) -> I, mut check: impl FnMut(&I)) {
    let base = seed();
    let only = env_num("VV_FUZZ_CASE").map(|n| n as usize);
    for i in 0..iterations() {
        if only.is_some_and(|o| o != i) {
            continue;
        }
        let mut rng = Rng::new(case_seed(name, base, i));
        let input = make(&mut rng);
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| check(&input))) {
            let shown = format!("{input:?}");
            let shown: String = shown.chars().take(4_000).collect();
            eprintln!(
                "\nfuzz target `{name}` failed on case {i} (VV_FUZZ_SEED={base:#x} VV_FUZZ_CASE={i}).\ninput: {shown}\n"
            );
            resume_unwind(e);
        }
    }
}

/// Runs `check` on every input in `inputs` (corpus entries, prefixes).
pub fn each<I: Debug>(name: &str, inputs: impl IntoIterator<Item = I>, mut check: impl FnMut(&I)) {
    for (i, input) in inputs.into_iter().enumerate() {
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| check(&input))) {
            let shown = format!("{input:?}");
            let shown: String = shown.chars().take(4_000).collect();
            eprintln!("\nfuzz target `{name}` failed on input {i}.\ninput: {shown}\n");
            resume_unwind(e);
        }
    }
}

/// Every cut of `s` at a character boundary, shortest first (`""` … `s`).
pub fn prefixes(s: &str) -> Vec<&str> {
    let mut out: Vec<&str> = s.char_indices().map(|(i, _)| s.split_at(i).0).collect();
    out.push(s);
    out
}

fn boundaries(s: &str) -> Vec<usize> {
    let mut b: Vec<usize> = s.char_indices().map(|(i, _)| i).collect();
    b.push(s.len());
    b
}

/// A random string of up to `max_pieces` items from `pieces` and [`NASTY`].
pub fn compose(rng: &mut Rng, pieces: &[&str], max_pieces: usize) -> String {
    let n = rng.below(max_pieces + 1);
    let mut s = String::new();
    for _ in 0..n {
        if pieces.is_empty() || rng.chance(30) {
            s.push_str(rng.pick(NASTY));
        } else {
            s.push_str(rng.pick(pieces));
        }
    }
    s
}

/// `sample` changed in 1–4 places, staying valid UTF-8: a piece inserted,
/// a range cut out or repeated, a character replaced, or the end cut off.
pub fn mutate_str(rng: &mut Rng, sample: &str, pieces: &[&str]) -> String {
    let mut s = sample.to_owned();
    for _ in 0..1 + rng.below(4) {
        let b = boundaries(&s);
        let at = *rng.pick(&b);
        let to = b[b.partition_point(|&x| x < at)..]
            .get(rng.below(24))
            .copied()
            .unwrap_or(s.len());
        let piece = if pieces.is_empty() || rng.chance(50) {
            *rng.pick(NASTY)
        } else {
            *rng.pick(pieces)
        };
        match rng.below(6) {
            0 | 1 => s.insert_str(at, piece),
            2 => s.replace_range(at..to, ""),
            3 => {
                let dup = s.get(at..to).unwrap_or_default().to_owned();
                s.insert_str(to, &dup);
            }
            4 => s.replace_range(at..to, piece),
            _ => s.truncate(at),
        }
    }
    s
}

/// `sample` changed in 1–4 places at any byte: may cut a character in half
/// or add invalid UTF-8, as a broken pipe or a bad file would.
pub fn mutate_bytes(rng: &mut Rng, sample: &[u8], pieces: &[&str]) -> Vec<u8> {
    let mut v = sample.to_vec();
    for _ in 0..1 + rng.below(4) {
        let at = rng.below(v.len() + 1);
        let to = (at + rng.below(24)).min(v.len());
        match rng.below(7) {
            0 => {
                let p = rng.pick(INVALID_UTF8);
                v.splice(at..at, p.iter().copied());
            }
            1 | 2 => {
                let p = if pieces.is_empty() || rng.chance(50) {
                    *rng.pick(NASTY)
                } else {
                    *rng.pick(pieces)
                };
                v.splice(at..at, p.bytes());
            }
            3 => {
                v.drain(at..to);
            }
            4 => {
                let dup = v[at..to].to_vec();
                v.splice(to..to, dup);
            }
            5 => {
                if let Some(x) = v.get_mut(at) {
                    *x ^= 1 << rng.below(8);
                }
            }
            _ => v.truncate(at),
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix_reference_values() {
        // Vigna's splitmix64.c, seed 0.
        let mut r = Rng::new(0);
        assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
    }

    #[test]
    fn mutations_stay_valid_utf8_and_cases_repeat() {
        let sample = "Merhaba dünya, şu an 😀 &gt;&gt; İstanbul";
        let mut a = Vec::new();
        run(
            "self-check",
            |r| mutate_str(r, sample, &["x"]),
            |s| a.push(s.clone()),
        );
        let mut b = Vec::new();
        run(
            "self-check",
            |r| mutate_str(r, sample, &["x"]),
            |s| b.push(s.clone()),
        );
        assert_eq!(a, b, "same seed, same cases");
        assert!(a.iter().any(|s| s != sample));
        assert_eq!(prefixes("aş").len(), 3);
        assert_eq!(prefixes("aş"), vec!["", "a", "aş"]);
    }

    #[test]
    fn byte_mutations_reach_invalid_utf8() {
        let mut invalid = 0;
        run(
            "self-check-bytes",
            |r| mutate_bytes(r, "şğı".as_bytes(), &[]),
            |v| {
                if std::str::from_utf8(v).is_err() {
                    invalid += 1;
                }
            },
        );
        assert!(invalid > 0);
    }
}
