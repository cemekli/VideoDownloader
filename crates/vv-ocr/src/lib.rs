//! Reading the text on screen in still frames (STATE.md F7.4; SPEC AI-C03,
//! DEC-13; ADR-0039).
//!
//! VideoVault ships no OCR model. On Windows it uses the OCR built into
//! Windows (`Windows.Media.Ocr`), which reads the languages whose OCR pack
//! is installed; [`system`] picks the first of the user's own languages it
//! can read (a Turkish Windows reads Turkish). Elsewhere [`system`] says
//! there is none. Tests use [`Fake`].
//!
//! A reader gets a PNG and returns its lines, top to bottom. Nothing is
//! written to disk or sent anywhere.

use std::sync::{Arc, Mutex};

#[cfg(windows)]
mod windows_ocr;

/// Stored as the producer of every OCR chunk.
pub const WINDOWS_PROVIDER: &str = "windows_ocr";

/// Why no text could be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrError {
    /// No OCR on this system, or none for any of the user's languages.
    Unavailable(String),
    /// The image could not be decoded.
    BadImage(String),
    /// The OCR engine failed on this image.
    Failed(String),
}

impl std::fmt::Display for OcrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrError::Unavailable(m) => write!(f, "no OCR available: {m}"),
            OcrError::BadImage(m) => write!(f, "image not readable: {m}"),
            OcrError::Failed(m) => write!(f, "OCR failed: {m}"),
        }
    }
}

impl std::error::Error for OcrError {}

/// One OCR engine, ready for frames. Used from one thread at a time.
pub trait Reader {
    /// Stored as `producer_provider`.
    fn provider(&self) -> &str;
    /// BCP 47 tag of the language it reads (`tr`, `en-US`).
    fn language(&self) -> &str;
    /// The lines of text in a PNG, top to bottom; empty when there is none.
    fn read_png(&self, png: &[u8]) -> Result<Vec<String>, OcrError>;
}

/// Opens a [`Reader`] when a job needs one. The desktop app uses
/// [`SystemOcr`]; tests a [`Fake`].
pub trait Ocr: Send + Sync + 'static {
    fn open(&self) -> Result<Box<dyn Reader>, OcrError>;
}

/// The system's own OCR.
pub struct SystemOcr;

impl Ocr for SystemOcr {
    fn open(&self) -> Result<Box<dyn Reader>, OcrError> {
        system()
    }
}

/// Windows' OCR for the first of the user's languages it can read.
#[cfg(windows)]
pub fn system() -> Result<Box<dyn Reader>, OcrError> {
    Ok(Box::new(windows_ocr::WindowsReader::from_user_languages()?))
}

/// No OCR outside Windows.
#[cfg(not(windows))]
pub fn system() -> Result<Box<dyn Reader>, OcrError> {
    Err(OcrError::Unavailable(
        "Windows' own OCR is the only reader".into(),
    ))
}

/// The languages the system's OCR can read, as BCP 47 tags (empty outside
/// Windows or when no OCR pack is installed).
pub fn available_languages() -> Vec<String> {
    #[cfg(windows)]
    {
        windows_ocr::available_languages()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// OCR for tests: each frame read answers the next text of a script, in
/// call order over all the readers it opened (lines split at `\n`);
/// frames past the end read as empty.
pub struct Fake {
    language: String,
    texts: Vec<String>,
    unavailable: bool,
    calls: Mutex<usize>,
}

impl Fake {
    pub fn new(language: &str, texts: &[&str]) -> Arc<Self> {
        Arc::new(Fake {
            language: language.to_owned(),
            texts: texts.iter().map(|s| (*s).to_owned()).collect(),
            unavailable: false,
            calls: Mutex::new(0),
        })
    }

    /// Every open fails with `Unavailable`.
    pub fn unavailable() -> Arc<Self> {
        Arc::new(Fake {
            language: "und".into(),
            texts: Vec::new(),
            unavailable: true,
            calls: Mutex::new(0),
        })
    }

    /// Frames read so far.
    pub fn calls(&self) -> usize {
        *self.calls.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Ocr for Arc<Fake> {
    fn open(&self) -> Result<Box<dyn Reader>, OcrError> {
        if self.unavailable {
            return Err(OcrError::Unavailable("test".into()));
        }
        Ok(Box::new(FakeReader(self.clone())))
    }
}

struct FakeReader(Arc<Fake>);

impl Reader for FakeReader {
    fn provider(&self) -> &str {
        "test"
    }
    fn language(&self) -> &str {
        &self.0.language
    }
    fn read_png(&self, png: &[u8]) -> Result<Vec<String>, OcrError> {
        if !png.starts_with(b"\x89PNG") {
            return Err(OcrError::BadImage("not a PNG".into()));
        }
        let mut n = self.0.calls.lock().unwrap_or_else(|e| e.into_inner());
        let text = self.0.texts.get(*n).cloned().unwrap_or_default();
        *n += 1;
        Ok(text
            .lines()
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty())
            .collect())
    }
}
