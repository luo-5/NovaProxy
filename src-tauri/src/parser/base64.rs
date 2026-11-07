use crate::parser::error::{ParseError, ParseResult};
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;

/// Strips UTF-8 BOM (`\u{feff}`) and leading/trailing whitespace
pub fn strip_bom_and_trim(input: &str) -> &str {
    input.trim_start_matches('\u{feff}').trim()
}

/// Decodes a Base64 or URL-safe Base64 string tolerantly:
/// - Strips BOM and whitespace (`\r`, `\n`, spaces, tabs)
/// - Normalizes URL-safe characters (`-` -> `+`, `_` -> `/`)
/// - Handles both padded and unpadded strings
pub fn decode_base64_permissive(input: &str) -> ParseResult<Vec<u8>> {
    let cleaned: String = strip_bom_and_trim(input)
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();

    if cleaned.is_empty() {
        return Err(ParseError::Base64("empty base64 payload".to_string()));
    }

    let unpadded = cleaned.trim_end_matches('=');
    let normalized: String = unpadded
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            other => other,
        })
        .collect();

    STANDARD_NO_PAD
        .decode(&normalized)
        .or_else(|_| URL_SAFE_NO_PAD.decode(unpadded))
        .or_else(|_| STANDARD.decode(&cleaned))
        .or_else(|_| URL_SAFE.decode(&cleaned))
        .map_err(|e| ParseError::Base64(e.to_string()))
}

/// Decodes a Base64 or URL-safe Base64 string into a UTF-8 `String`
pub fn decode_base64_to_string(input: &str) -> ParseResult<String> {
    let bytes = decode_base64_permissive(input)?;
    String::from_utf8(bytes).map_err(|e| ParseError::Base64(format!("invalid UTF-8: {e}")))
}

/// Encodes bytes using standard padded Base64
pub fn encode_base64_standard(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Encodes bytes using URL-safe unpadded Base64
pub fn encode_base64_url_nopad(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_base64_variants() {
        let orig = "chacha20-ietf-poly1305:test-pass?word/123+456";
        let std_b64 = STANDARD.encode(orig.as_bytes());
        let url_nopad = URL_SAFE_NO_PAD.encode(orig.as_bytes());
        let with_bom_and_crlf = format!("\u{feff}{}\r\n  ", url_nopad);

        assert_eq!(decode_base64_to_string(&std_b64).unwrap(), orig);
        assert_eq!(decode_base64_to_string(&url_nopad).unwrap(), orig);
        assert_eq!(decode_base64_to_string(&with_bom_and_crlf).unwrap(), orig);
    }
}
