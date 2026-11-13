//! `vv-asr-report --db LIBRARY.sqlite list`
//!   items that have local transcripts (id, number of transcripts, title).
//! `vv-asr-report --db LIBRARY.sqlite --item ID|TEXT [--reference FILE
//!   --reference-language tr|en] [--terms "Kadıköy,PostgreSQL"]`
//!   the F6.7 report section for that item, as Markdown on stdout.
//! Normally started by `npm run asr:report` (scripts/asr-report.mjs), which
//! finds the library and adds the machine description (ADR-0034). The
//! library is opened read-only; the app may stay open.

use std::path::PathBuf;
use vv_asr_quality::report::{self, clock, Options};

fn main() {
    if let Err(e) = run() {
        eprintln!("vv-asr-report: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), report::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let val = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let db = PathBuf::from(val("--db").ok_or("--db LIBRARY.sqlite is required")?);
    if !db.is_file() {
        return Err(format!("no library at {}", db.display()).into());
    }
    let c = vv_persistence::open_reader(&db)?;
    if args.iter().any(|a| a == "list") {
        let items = report::items_with_local(&c)?;
        if items.is_empty() {
            println!("No item has a local transcript yet (Details → Transcript → Transcribe on this PC).");
        }
        for i in items {
            println!(
                "{}  {} local  {}  {}",
                i.id,
                i.local,
                i.duration_ms.map_or("—".into(), clock),
                i.name()
            );
        }
        let failed = report::failed_jobs(&c, 10)?;
        if !failed.is_empty() {
            println!("\nFailed transcriptions (newest first):");
            for f in failed {
                println!(
                    "  {}  {}  {}  — {}",
                    report::model_label(&f.model_id),
                    f.error_code,
                    f.message.as_deref().unwrap_or("(no message)"),
                    f.item
                );
            }
        }
        return Ok(());
    }
    let q = val("--item").ok_or("--item ID|TEXT is required (or `list`)")?;
    let item = report::find_item(&c, &q)?;
    let reference_path = val("--reference");
    let reference_file = match &reference_path {
        Some(p) => {
            let lang = val("--reference-language")
                .ok_or("--reference needs --reference-language tr|en")?;
            Some((std::fs::read_to_string(p)?, lang))
        }
        None => None,
    };
    let terms = val("--terms")
        .map(|t| {
            t.split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let opts = Options {
        reference_file,
        reference_path,
        terms,
    };
    print!("{}", report::item_report(&c, &item, &opts)?);
    Ok(())
}
