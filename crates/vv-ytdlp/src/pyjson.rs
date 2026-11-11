//! yt-dlp is Python: its JSON may contain bare `NaN`/`Infinity` (json.dumps
//! default) and our progress template prints missing fields as bare `NA`.
//! Neither is JSON, so before parsing we replace those tokens, when they sit
//! outside a string, with `null`.

/// Returns `input` with bare `NaN`, `Infinity`, `-Infinity` and `NA` outside
/// strings replaced by `null`. Everything else is copied unchanged.
#[expect(
    clippy::string_slice,
    reason = "slices start and end at ASCII token bytes (`NaN`, `NA`, …)"
)]
pub(crate) fn nullify_bare_tokens(input: &str) -> String {
    const TOKENS: [&str; 4] = ["-Infinity", "Infinity", "NaN", "NA"];
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0;
    let mut copied = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        let at_value_start = i == 0 || matches!(prev_non_space(bytes, i), Some(b':' | b',' | b'['));
        if at_value_start {
            if let Some(t) = TOKENS.iter().find(|t| bytes[i..].starts_with(t.as_bytes())) {
                let end = i + t.len();
                let boundary = bytes
                    .get(end)
                    .is_none_or(|c| !c.is_ascii_alphanumeric() && *c != b'_');
                if boundary {
                    out.push_str(&input[copied..i]);
                    out.push_str("null");
                    i = end;
                    copied = end;
                    continue;
                }
            }
        }
        i += 1;
    }
    out.push_str(&input[copied..]);
    out
}

fn prev_non_space(bytes: &[u8], i: usize) -> Option<u8> {
    bytes[..i]
        .iter()
        .rev()
        .copied()
        .find(|c| !c.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::nullify_bare_tokens as n;

    #[test]
    fn replaces_bare_tokens_only() {
        assert_eq!(
            n(r#"{"a":NA,"b":NaN,"c":[Infinity, -Infinity]}"#),
            r#"{"a":null,"b":null,"c":[null, null]}"#
        );
        assert_eq!(
            n(r#"{"a":"NA","b":"x:NaN,"}"#),
            r#"{"a":"NA","b":"x:NaN,"}"#
        );
        assert_eq!(n(r#"{"a":"q\"NA","b":NA}"#), r#"{"a":"q\"NA","b":null}"#);
        assert_eq!(n(r#"{"NA":1}"#), r#"{"NA":1}"#);
        assert_eq!(n(r#"{"a":NAB}"#), r#"{"a":NAB}"#);
        assert_eq!(
            n("{\"t\":\"İş NA\",\"x\":NA}"),
            "{\"t\":\"İş NA\",\"x\":null}"
        );
    }
}
