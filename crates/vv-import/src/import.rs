//! Local file and folder import (SPEC §7.2 IMP-02…06, STATE.md F1.4).
//!
//! Per file, in order: restricted ffprobe (a manifest or list is refused
//! before anything opens what it names, AC-59), full-path duplicate check,
//! optional SHA-256, then one transaction with the item, its original asset
//! and its search document. A content-hash match with another item opens a
//! `DuplicateCandidate(content_hash)`; nothing is merged (IMP-05, AC-34).
//! Every file has its own outcome and transaction, so one bad file never
//! undoes the others (IMP-04, AC-04). The source file is only read.

use crate::fsutil::{self, Candidate, SkipReason};
use crate::roots;
use crate::tools::{MediaTools, ProbeFailure};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::result::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use vv_domain::error::ErrorCode;
use vv_ffmpeg::{Container, Probed};
use vv_persistence::library::repo::{self, LibraryItem, MediaAsset};
use vv_persistence::library::*;
use vv_persistence::rusqlite::{Connection, TransactionBehavior};

#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Walk subfolders of a selected folder (the user chooses, §7.2).
    pub recursive: bool,
    /// Compute SHA-256 for duplicate detection (IMP-05 "isteğe bağlı").
    pub hash: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            recursive: false,
            hash: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOutcome {
    Imported {
        item_id: String,
        asset_id: String,
        /// Items that already hold the same content (open candidates).
        duplicate_of: Vec<String>,
    },
    /// This exact file is already in the library.
    AlreadyImported {
        item_id: String,
    },
    /// Refused with a §19 code (`INPUT_REJECTED`, `MEDIA_CORRUPT`,
    /// `TOOL_MISSING`, `TIMEOUT`, ...). Nothing was written.
    Rejected {
        code: ErrorCode,
    },
    Skipped {
        reason: SkipReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResult {
    pub path: PathBuf,
    pub outcome: FileOutcome,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub files: Vec<FileResult>,
    /// Files in folders that were not media by extension.
    pub ignored: usize,
    /// Canceled before every file was processed; finished files stay.
    pub canceled: bool,
}

impl ImportReport {
    pub fn imported(&self) -> usize {
        self.files
            .iter()
            .filter(|f| matches!(f.outcome, FileOutcome::Imported { .. }))
            .count()
    }
    pub fn rejected(&self) -> usize {
        self.files
            .iter()
            .filter(|f| matches!(f.outcome, FileOutcome::Rejected { .. }))
            .count()
    }
}

pub fn import_paths(
    c: &mut Connection,
    clock: &dyn Clock,
    tools: &MediaTools,
    inputs: &[PathBuf],
    opts: &ImportOptions,
    cancel: &AtomicBool,
) -> Result<ImportReport, LibraryError> {
    let collected = fsutil::collect(inputs, opts.recursive);
    let mut report = ImportReport {
        ignored: collected.ignored,
        ..Default::default()
    };
    for (path, reason) in collected.skipped {
        report.files.push(FileResult {
            path,
            outcome: FileOutcome::Skipped { reason },
        });
    }
    for cand in collected.candidates {
        if cancel.load(Ordering::SeqCst) {
            report.canceled = true;
            break;
        }
        let outcome = match import_one(c, clock, tools, &cand, opts, cancel)? {
            Some(o) => o,
            None => {
                report.canceled = true;
                break;
            }
        };
        report.files.push(FileResult {
            path: cand.path,
            outcome,
        });
    }
    Ok(report)
}

/// `None` when canceled mid-file (nothing written for that file).
fn import_one(
    c: &mut Connection,
    clock: &dyn Clock,
    tools: &MediaTools,
    cand: &Candidate,
    opts: &ImportOptions,
    cancel: &AtomicBool,
) -> Result<Option<FileOutcome>, LibraryError> {
    // Probe first: a manifest is refused before any other step touches it.
    let probed = match tools.probe(&cand.path) {
        Ok(p) => p,
        Err(e) => return Ok(Some(rejected(&e))),
    };
    let size = std::fs::metadata(&cand.path).map(|m| m.len() as i64).ok();
    let hash = if opts.hash {
        match sha256(&cand.path, cancel) {
            Ok(Some(h)) => Some(h),
            Ok(None) => return Ok(None),
            Err(_) => {
                return Ok(Some(FileOutcome::Skipped {
                    reason: SkipReason::Unreadable,
                }))
            }
        }
    } else {
        None
    };

    let now = clock.now_ms();
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let root = roots::root_for(&tx, now, &cand.root_dir, &cand.path)?;
    let storage_ref = fsutil::relative_ref(Path::new(&root.path), &cand.path)
        .ok_or(LibraryError::Invalid("file outside its storage root"))?;
    if let Some(a) = repo::asset_at(&tx, &root.id, &storage_ref)? {
        tx.commit()?; // the root may be new
        return Ok(Some(FileOutcome::AlreadyImported {
            item_id: a.library_item_id,
        }));
    }

    let mut item = LibraryItem::new(ItemKind::LocalVideo, SourcePlatform::Local, now);
    item.title = cand
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_owned);
    item.duration_ms = probed.duration_s.map(|s| (s * 1000.0).round() as i64);
    item.metadata_status = MetadataStatus::Ready;
    let media_kind = if probed.video.is_some() {
        MediaKind::Video
    } else {
        MediaKind::Audio
    };
    let mut asset = MediaAsset::new(
        &item.id,
        AssetRole::Original,
        media_kind,
        StorageKind::ExternalReference,
        &root.id,
        &storage_ref,
        now,
    );
    describe(&mut asset, &probed, &cand.path);
    asset.size_bytes = size.or(probed.size_bytes.map(|s| s as i64));
    asset.hash_algorithm = hash.as_ref().map(|_| "sha256".to_owned());
    asset.content_hash = hash;
    asset.acquired_at = Some(now);
    asset.acquisition_status = AcquisitionStatus::Ready;
    asset.integrity_state = IntegrityState::Verified;

    repo::insert_item(&tx, &item)?;
    repo::insert_media_asset(&tx, &asset)?;
    let mut duplicate_of = Vec::new();
    if let Some(h) = &asset.content_hash {
        for other in repo::originals_with_hash(&tx, h, &item.id)? {
            if !duplicate_of.contains(&other.library_item_id) {
                repo::open_duplicate_candidate(
                    &tx,
                    &other.library_item_id,
                    &item.id,
                    DuplicateReason::ContentHash,
                    now,
                )?;
                duplicate_of.push(other.library_item_id);
            }
        }
    }
    tx.commit()?;
    Ok(Some(FileOutcome::Imported {
        item_id: item.id,
        asset_id: asset.id,
        duplicate_of,
    }))
}

fn rejected(e: &ProbeFailure) -> FileOutcome {
    FileOutcome::Rejected { code: e.code() }
}

/// Container name, MIME type and stream facts from the probe (IMP-03).
fn describe(a: &mut MediaAsset, p: &Probed, path: &Path) {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let video = p.video.is_some();
    let (container, mime) = match (p.container, ext.as_str()) {
        (Container::Mp4, "mov") => ("mov", "video/quicktime"),
        (Container::Mp4, _) if video => ("mp4", "video/mp4"),
        (Container::Mp4, _) => ("mp4", "audio/mp4"),
        (Container::Matroska, "webm") if video => ("webm", "video/webm"),
        (Container::Matroska, "webm") => ("webm", "audio/webm"),
        (Container::Matroska, _) => ("matroska", "video/x-matroska"),
        (Container::Mp3, _) => ("mp3", "audio/mpeg"),
        (Container::Ogg, _) => ("ogg", "audio/ogg"),
        (Container::Flac, _) => ("flac", "audio/flac"),
        (Container::Wav, _) => ("wav", "audio/wav"),
        (Container::Aac, _) => ("aac", "audio/aac"),
    };
    a.container = Some(container.to_owned());
    a.mime_type = Some(mime.to_owned());
    a.video_codec = p.video.clone().filter(|v| !v.is_empty());
    a.width = p.width.map(i64::from);
    a.height = p.height.map(i64::from);
    a.duration_ms = p.duration_s.map(|s| (s * 1000.0).round() as i64);
    let tracks: Vec<serde_json::Value> = p
        .audio_tracks
        .iter()
        .map(|t| serde_json::json!({ "index": t.index, "codec": t.codec, "channels": t.channels, "language": t.language }))
        .collect();
    a.audio_tracks = Some(serde_json::json!({ "v": 1, "tracks": tracks }).to_string());
}

/// SHA-256 of a file, reading only. `Ok(None)` when canceled.
pub fn sha256(path: &Path, cancel: &AtomicBool) -> std::io::Result<Option<String>> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(Some(
        h.finalize().iter().map(|b| format!("{b:02x}")).collect(),
    ))
}
