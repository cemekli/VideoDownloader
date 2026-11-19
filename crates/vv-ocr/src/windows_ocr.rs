//! `Windows.Media.Ocr` (Windows 10 and later). WinRT objects are created on
//! the calling thread; the windows crate joins the multithreaded apartment
//! on first use, so no set-up is needed. Async calls are waited for with
//! `join`.

use crate::{OcrError, Reader};
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

pub struct WindowsReader {
    engine: OcrEngine,
    language: String,
}

fn failed(e: windows::core::Error) -> OcrError {
    OcrError::Failed(format!("{:#010x}", e.code().0))
}

impl WindowsReader {
    /// The engine for the first user language with an OCR pack.
    pub fn from_user_languages() -> Result<Self, OcrError> {
        let unavailable =
            |e: windows::core::Error| OcrError::Unavailable(format!("{:#010x}", e.code().0));
        // `null` (an error here) when none of the user's languages has OCR.
        let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(unavailable)?;
        let language = engine
            .RecognizerLanguage()
            .and_then(|l| l.LanguageTag())
            .map_err(unavailable)?
            .to_string();
        Ok(WindowsReader { engine, language })
    }
}

pub fn available_languages() -> Vec<String> {
    let Ok(list) = OcrEngine::AvailableRecognizerLanguages() else {
        return Vec::new();
    };
    list.into_iter()
        .filter_map(|l| l.LanguageTag().ok())
        .map(|t| t.to_string())
        .collect()
}

impl Reader for WindowsReader {
    fn provider(&self) -> &str {
        crate::WINDOWS_PROVIDER
    }

    fn language(&self) -> &str {
        &self.language
    }

    fn read_png(&self, png: &[u8]) -> Result<Vec<String>, OcrError> {
        let bad = |e: windows::core::Error| OcrError::BadImage(format!("{:#010x}", e.code().0));
        let stream = InMemoryRandomAccessStream::new().map_err(failed)?;
        let writer = DataWriter::CreateDataWriter(&stream).map_err(failed)?;
        writer.WriteBytes(png).map_err(failed)?;
        writer
            .StoreAsync()
            .and_then(|op| op.join())
            .map_err(failed)?;
        writer
            .FlushAsync()
            .and_then(|op| op.join())
            .map_err(failed)?;
        writer.DetachStream().map_err(failed)?;
        stream.Seek(0).map_err(failed)?;
        let decoder = BitmapDecoder::CreateAsync(&stream)
            .and_then(|op| op.join())
            .map_err(bad)?;
        // The engine takes Gray8 or BGRA8; PNG frames from FFmpeg are RGB.
        let bitmap = decoder
            .GetSoftwareBitmapConvertedAsync(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Premultiplied,
            )
            .and_then(|op| op.join())
            .map_err(bad)?;
        let max = OcrEngine::MaxImageDimension().unwrap_or(u32::MAX);
        let (w, h) = (
            bitmap.PixelWidth().map_err(failed)?,
            bitmap.PixelHeight().map_err(failed)?,
        );
        if w <= 0 || h <= 0 || w as u32 > max || h as u32 > max {
            return Err(OcrError::BadImage(format!("{w}x{h} is larger than {max}")));
        }
        let result = self
            .engine
            .RecognizeAsync(&bitmap)
            .and_then(|op| op.join())
            .map_err(failed)?;
        let mut lines = Vec::new();
        for line in result.Lines().map_err(failed)? {
            let text = line.Text().map_err(failed)?.to_string();
            let text = text.trim();
            if !text.is_empty() {
                lines.push(text.to_owned());
            }
        }
        Ok(lines)
    }
}
