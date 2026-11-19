//! The readers on a still frame with text (F7.4, AC-41). The real one runs
//! on Windows only, and only where an OCR language is installed (it is on
//! Windows 10/11 desktops; a server image may lack it, which is printed).

use std::path::Path;
use vv_ocr::{Fake, Ocr, OcrError};

fn frame() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/ocr/screen-text.png"),
    )
    .unwrap()
}

#[test]
fn the_fake_answers_its_script_in_order() {
    let f = Fake::new("en", &["HARRIER\n  \n JUMP JET ", ""]);
    let r = f.open().unwrap();
    assert_eq!(r.language(), "en");
    assert_eq!(r.read_png(&frame()).unwrap(), vec!["HARRIER", "JUMP JET"]);
    assert!(r.read_png(&frame()).unwrap().is_empty());
    assert!(r.read_png(&frame()).unwrap().is_empty(), "past the end");
    assert!(matches!(r.read_png(b"GIF89a"), Err(OcrError::BadImage(_))));
    assert_eq!(f.calls(), 3);
    assert!(matches!(
        Fake::unavailable().open(),
        Err(OcrError::Unavailable(_))
    ));
}

#[cfg(not(windows))]
#[test]
fn there_is_no_system_reader_off_windows() {
    assert!(matches!(vv_ocr::system(), Err(OcrError::Unavailable(_))));
    assert!(vv_ocr::available_languages().is_empty());
}

#[cfg(windows)]
#[test]
fn windows_reads_the_text_of_a_frame() {
    let langs = vv_ocr::available_languages();
    let r = match vv_ocr::system() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipped: {e} (OCR languages: {langs:?})");
            return;
        }
    };
    assert!(langs.iter().any(|l| l == r.language()), "{langs:?}");
    let lines = r.read_png(&frame()).unwrap();
    let text = lines.join(" ").to_uppercase();
    assert!(text.contains("HARRIER"), "{lines:?}");
    assert!(text.contains("1985"), "{lines:?}");
    assert!(matches!(
        r.read_png(b"not an image"),
        Err(OcrError::BadImage(_))
    ));
}
