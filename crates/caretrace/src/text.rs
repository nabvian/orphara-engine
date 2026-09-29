//! Small text helpers shared across the extractor.
//!
//! All offsets in this crate are BYTE offsets into the original `&str` (that is
//! what `regex` reports), which stays self-consistent because every span is
//! sliced from the same source string. These helpers inspect the character on
//! either side of a byte boundary so word-boundary and adjacency decisions can
//! be made in code — Rust's `regex` has no lookbehind/lookahead.

/// The character ending just before byte offset `i`, if any.
pub fn char_before(s: &str, i: usize) -> Option<char> {
    if i == 0 || i > s.len() {
        return None;
    }
    s[..i].chars().next_back()
}

/// The character starting at byte offset `i`, if any.
pub fn char_at(s: &str, i: usize) -> Option<char> {
    if i >= s.len() {
        return None;
    }
    s[i..].chars().next()
}

/// Whether the char is alphanumeric (Unicode-aware).
pub fn is_alnum(c: char) -> bool {
    c.is_alphanumeric()
}

/// Bounds of the line containing byte offset `pos`: `(line_start, line_end)`
/// where `line_end` is the '\n' position or the string length.
pub fn line_bounds(text: &str, pos: usize) -> (usize, usize) {
    let start = text[..pos.min(text.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = text[pos.min(text.len())..]
        .find('\n')
        .map(|i| pos + i)
        .unwrap_or(text.len());
    (start, end)
}
