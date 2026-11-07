//! `vv-bench generate [--seed N] [--items N] [--segments N] --out DIR`
//!   writes items.jsonl, segments.jsonl and manifest.json.
//! `vv-bench run [--seed N] [--items N] [--segments N] --out DIR`
//!   generates in memory, runs BM-01…03 and prints the report as JSON.
//! Normally started by `npm run bench` (scripts/bench.mjs), which adds the
//! machine description (ADR-0009).

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use vv_benchdata::{generate, write_jsonl, Params};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().cloned().unwrap_or_default();
    let val = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let mut p = Params::default();
    if let Some(v) = val("--seed") {
        p.seed = v.parse()?;
    }
    if let Some(v) = val("--items") {
        p.items = v.parse()?;
    }
    if let Some(v) = val("--segments") {
        p.segments = v.parse()?;
    }
    let out = PathBuf::from(val("--out").ok_or("--out DIR is required")?);
    let data = generate(p);
    match mode.as_str() {
        "generate" => {
            std::fs::create_dir_all(&out)?;
            write_jsonl(
                &data.items,
                BufWriter::new(File::create(out.join("items.jsonl"))?),
            )?;
            write_jsonl(
                &data.segments,
                BufWriter::new(File::create(out.join("segments.jsonl"))?),
            )?;
            let m = data.manifest();
            std::fs::write(
                out.join("manifest.json"),
                format!("{}\n", serde_json::to_string_pretty(&m)?),
            )?;
            println!("{}", serde_json::to_string(&m)?);
        }
        "run" => {
            let report = vv_benchdata::bench::run(&data, &out)?;
            println!(
                "{}",
                serde_json::to_string(
                    &serde_json::json!({ "manifest": data.manifest(), "report": report })
                )?
            );
        }
        _ => {
            return Err(
                "usage: vv-bench generate|run [--seed N] [--items N] [--segments N] --out DIR"
                    .into(),
            )
        }
    }
    Ok(())
}
