//! Pure single-line editing helpers.
//!
//! Offsets are UTF-8 byte indexes. UTF-16 conversions exist because GPUI's
//! input handler speaks UTF-16. Word boundaries are whitespace-delimited,
//! not UAX #29.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// Removes line breaks. `shape_line` panics on newlines, and these controls
/// are single-line.
pub fn sanitize_single_line(text: &str) -> String {
    text.chars().filter(|c| *c != '\n' && *c != '\r').collect()
}

pub fn previous_grapheme(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    text.grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

pub fn next_grapheme(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    text.grapheme_indices(true)
        .find_map(|(idx, _)| (idx > offset).then_some(idx))
        .unwrap_or(text.len())
}

/// Start of the next word, or the end of the string.
///
/// From inside a word, skip the rest of it and the following whitespace.
/// From whitespace, skip to the next non-whitespace character.
pub fn next_word(text: &str, offset: usize) -> usize {
    let mut i = offset.min(text.len());
    if i >= text.len() {
        return text.len();
    }
    let at_space = text[i..].chars().next().is_some_and(char::is_whitespace);
    if at_space {
        skip_whitespace(text, &mut i);
        return i;
    }
    skip_word(text, &mut i);
    skip_whitespace(text, &mut i);
    i
}

/// Start of the previous word, or 0.
pub fn previous_word(text: &str, offset: usize) -> usize {
    let mut i = offset.min(text.len());
    if i == 0 {
        return 0;
    }
    i = previous_grapheme(text, i);
    while i > 0 {
        let ch = text[..i].chars().next_back().unwrap();
        if !ch.is_whitespace() {
            break;
        }
        i -= ch.len_utf8();
    }
    while i > 0 {
        let ch = text[..i].chars().next_back().unwrap();
        if ch.is_whitespace() {
            break;
        }
        i -= ch.len_utf8();
    }
    i
}

/// The whitespace-delimited word containing `index`.
pub fn word_bounds(text: &str, index: usize) -> Range<usize> {
    let mut start = index.min(text.len());
    while start > 0 {
        let ch = text[..start].chars().next_back().unwrap();
        if ch.is_whitespace() {
            break;
        }
        start -= ch.len_utf8();
    }
    let mut end = index.min(text.len());
    let bytes = text.as_bytes();
    while end < bytes.len() {
        let ch = text[end..].chars().next().unwrap();
        if ch.is_whitespace() {
            break;
        }
        end += ch.len_utf8();
    }
    start..end
}

pub fn offset_to_utf16(text: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    let mut utf8 = 0;
    for ch in text.chars() {
        if utf8 >= offset {
            break;
        }
        utf8 += ch.len_utf8();
        utf16 += ch.len_utf16();
    }
    utf16
}

pub fn offset_from_utf16(text: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    let mut utf8 = 0;
    for ch in text.chars() {
        if utf16 >= offset {
            break;
        }
        utf16 += ch.len_utf16();
        utf8 += ch.len_utf8();
    }
    utf8
}

pub fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    offset_to_utf16(text, range.start)..offset_to_utf16(text, range.end)
}

pub fn range_from_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    offset_from_utf16(text, range.start)..offset_from_utf16(text, range.end)
}

fn skip_whitespace(text: &str, i: &mut usize) {
    let bytes = text.as_bytes();
    while *i < bytes.len() {
        let ch = text[*i..].chars().next().unwrap();
        if !ch.is_whitespace() {
            break;
        }
        *i += ch.len_utf8();
    }
}

fn skip_word(text: &str, i: &mut usize) {
    let bytes = text.as_bytes();
    while *i < bytes.len() {
        let ch = text[*i..].chars().next().unwrap();
        if ch.is_whitespace() {
            break;
        }
        *i += ch.len_utf8();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_movement_on_a_simple_phrase() {
        let text = "hello world";
        assert_eq!(next_word(text, 0), 6);
        assert_eq!(next_word(text, 3), 6);
        assert_eq!(next_word(text, 6), 11);
        assert_eq!(previous_word(text, 11), 6);
        assert_eq!(previous_word(text, 6), 0);
        assert_eq!(previous_word(text, 3), 0);
    }

    #[test]
    fn graphemes_do_not_split_a_combining_mark() {
        let text = "e\u{0301}x";
        assert_eq!(next_grapheme(text, 0), "e\u{0301}".len());
        assert_eq!(previous_grapheme(text, text.len()), "e\u{0301}".len());
    }

    #[test]
    fn utf16_round_trip_for_non_bmp() {
        let text = "a😀b";
        let emoji = "a".len();
        assert_eq!(offset_to_utf16(text, emoji), 1);
        assert_eq!(offset_from_utf16(text, 1), emoji);
        assert_eq!(offset_to_utf16(text, text.len()), 4);
        assert_eq!(sanitize_single_line("a\nb\rc"), "abc");
    }
}
