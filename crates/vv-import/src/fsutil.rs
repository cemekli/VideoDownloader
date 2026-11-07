//! File system helpers: path form, links, cloud placeholders, folder walk
//! (SPEC §7.2 "junction/symlink döngüleri takip edilmez", §14.2, DATA-10).

use std::fs::Metadata;
use std::path::{Component, Path, PathBuf};

/// Extensions a folder import looks at. Only a hint for which files to
/// probe; the restricted ffprobe decides (IMP-02).
pub const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "m4a", "mp3", "ogg", "oga", "opus", "flac", "wav", "aac",
];

pub fn has_media_extension(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// `canonicalize`, without Windows' `\\?\` prefix where a plain form exists,
/// so stored paths read like the user's paths and tools accept them.
pub fn canonical(p: &Path) -> std::io::Result<PathBuf> {
    let c = std::fs::canonicalize(p)?;
    Ok(strip_verbatim(&c))
}

pub fn strip_verbatim(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        let b = rest.as_bytes();
        if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\' {
            return PathBuf::from(rest);
        }
    }
    p.to_owned()
}

/// Windows file attributes we look at.
#[cfg(windows)]
mod attr {
    pub const REPARSE_POINT: u32 = 0x400;
    pub const OFFLINE: u32 = 0x1000;
    pub const RECALL_ON_OPEN: u32 = 0x4_0000;
    pub const RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
}

/// A cloud-only placeholder (OneDrive and similar): reading it would
/// download it, so import must not touch its content without consent
/// (DATA-10).
pub fn is_cloud_placeholder(md: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let a = md.file_attributes();
        a & (attr::OFFLINE | attr::RECALL_ON_OPEN | attr::RECALL_ON_DATA_ACCESS) != 0
    }
    #[cfg(not(windows))]
    {
        let _ = md;
        false
    }
}

/// Symlink, junction or other reparse point (from `symlink_metadata`).
pub fn is_link(md: &Metadata) -> bool {
    if md.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Cloud placeholders are reparse points too; they are files.
        md.file_attributes() & attr::REPARSE_POINT != 0 && !is_cloud_placeholder(md)
    }
    #[cfg(not(windows))]
    false
}

/// `file` relative to `root` as `a/b/c.mp4`, or `None` if it is not inside.
/// Both must be canonical. Component based, never a string prefix (§14.2).
pub fn relative_ref(root: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for c in rel.components() {
        match c {
            Component::Normal(s) => parts.push(s.to_str()?.to_owned()),
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// `root` + `a/b/c.mp4`.
pub fn resolve_ref(root: &Path, storage_ref: &str) -> PathBuf {
    storage_ref
        .split('/')
        .fold(root.to_owned(), |p, s| p.join(s))
}

/// Why a path was not imported before probing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Not found or unreadable.
    Unreadable,
    /// Symlink/junction: not followed (§7.2).
    Link,
    /// Cloud-only placeholder (DATA-10).
    CloudOnly,
    /// Path is not valid Unicode; it could not be stored faithfully.
    NonUnicodePath,
}

/// One file to import, with the folder its root should be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub root_dir: PathBuf,
}

#[derive(Debug, Default)]
pub struct Collected {
    pub candidates: Vec<Candidate>,
    pub skipped: Vec<(PathBuf, SkipReason)>,
    /// Files in folders without a media extension (not probed, not reported
    /// one by one).
    pub ignored: usize,
}

/// Expands the user's selection. A selected file is always a candidate
/// (ffprobe decides); in a selected folder only media extensions are, and
/// subfolders only when `recursive`. Links are never followed.
pub fn collect(inputs: &[PathBuf], recursive: bool) -> Collected {
    let mut out = Collected::default();
    for input in inputs {
        let Ok(md) = std::fs::symlink_metadata(input) else {
            out.skipped.push((input.clone(), SkipReason::Unreadable));
            continue;
        };
        if md.is_dir() && !is_link(&md) {
            match canonical(input) {
                Ok(root) => walk(&root, &root, recursive, &mut out),
                Err(_) => out.skipped.push((input.clone(), SkipReason::Unreadable)),
            }
            continue;
        }
        if is_cloud_placeholder(&md) {
            out.skipped.push((input.clone(), SkipReason::CloudOnly));
            continue;
        }
        // An explicitly chosen file link resolves to its target; the target
        // is what gets referenced.
        match canonical(input) {
            Ok(p) if p.is_file() => {
                if p.to_str().is_none() {
                    out.skipped
                        .push((input.clone(), SkipReason::NonUnicodePath));
                } else {
                    let root_dir = p.parent().map(Path::to_owned).unwrap_or_else(|| p.clone());
                    out.candidates.push(Candidate { path: p, root_dir });
                }
            }
            _ => out.skipped.push((input.clone(), SkipReason::Unreadable)),
        }
    }
    // The same file selected twice (or via file and folder) is one candidate.
    let mut seen = std::collections::HashSet::new();
    out.candidates.retain(|c| seen.insert(c.path.clone()));
    out
}

fn walk(dir: &Path, root: &Path, recursive: bool, out: &mut Collected) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        out.skipped.push((dir.to_owned(), SkipReason::Unreadable));
        return;
    };
    let mut entries: Vec<_> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let Ok(md) = std::fs::symlink_metadata(&p) else {
            out.skipped.push((p, SkipReason::Unreadable));
            continue;
        };
        if is_link(&md) {
            out.skipped.push((p, SkipReason::Link));
        } else if md.is_dir() {
            if recursive {
                walk(&p, root, recursive, out);
            }
        } else if !has_media_extension(&p) {
            out.ignored += 1;
        } else if is_cloud_placeholder(&md) {
            out.skipped.push((p, SkipReason::CloudOnly));
        } else if p.to_str().is_none() {
            out.skipped.push((p, SkipReason::NonUnicodePath));
        } else {
            out.candidates.push(Candidate {
                path: p,
                root_dir: root.to_owned(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_prefix_is_removed_only_when_safe() {
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\C:\Users\a\v.mp4")),
            PathBuf::from(r"C:\Users\a\v.mp4")
        );
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\UNC\nas\share\v.mp4")),
            PathBuf::from(r"\\nas\share\v.mp4")
        );
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\Volume{x}\v.mp4")),
            PathBuf::from(r"\\?\Volume{x}\v.mp4")
        );
        assert_eq!(
            strip_verbatim(Path::new("/home/a/v.mp4")),
            PathBuf::from("/home/a/v.mp4")
        );
    }

    #[test]
    fn relative_refs_are_component_based() {
        let root = Path::new("/media/vids");
        assert_eq!(
            relative_ref(root, Path::new("/media/vids/a/b.mp4")).as_deref(),
            Some("a/b.mp4")
        );
        assert_eq!(
            relative_ref(root, Path::new("/media/vids2/b.mp4")),
            None,
            "not a string prefix"
        );
        assert_eq!(relative_ref(root, root), None);
        assert_eq!(
            resolve_ref(root, "a/b.mp4"),
            Path::new("/media/vids/a/b.mp4")
        );
    }
}
