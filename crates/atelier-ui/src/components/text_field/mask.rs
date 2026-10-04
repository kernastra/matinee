//! Glyph masking for secure text entry.
//!
//! One bullet is painted per Unicode scalar. Byte offsets in the real value
//! map onto that bullet string so the caret, selection, and hit testing stay
//! aligned. The real characters stay in the field buffer.

pub(super) const MASK: char = '\u{2022}';

pub(super) fn masked_glyphs(content: &str) -> String {
    let mut glyphs = String::with_capacity(content.chars().count() * MASK.len_utf8());
    for _ in content.chars() {
        glyphs.push(MASK);
    }
    glyphs
}

pub(super) fn content_to_mask_offset(content: &str, byte: usize) -> usize {
    let byte = floor_char_boundary(content, byte.min(content.len()));
    content[..byte].chars().count() * MASK.len_utf8()
}

pub(super) fn mask_offset_to_content(content: &str, mask_byte: usize) -> usize {
    let glyphs = snap_mask_offset(mask_byte) / MASK.len_utf8();
    content
        .char_indices()
        .nth(glyphs)
        .map(|(index, _)| index)
        .unwrap_or(content.len())
}

/// Move a byte index onto the nearest bullet boundary.
pub(super) fn snap_mask_offset(offset: usize) -> usize {
    let size = MASK.len_utf8();
    let glyphs = offset.saturating_add(size / 2) / size;
    glyphs * size
}

/// Value reported to the Gallery inspector. Secure fields never contribute
/// the underlying characters.
pub(super) fn inspection_value(content: &str, masked: bool) -> String {
    if masked {
        masked_glyphs(content)
    } else {
        content.to_string()
    }
}

fn floor_char_boundary(content: &str, mut byte: usize) -> usize {
    while byte > 0 && !content.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_scalar_becomes_one_bullet() {
        assert_eq!(masked_glyphs(""), "");
        assert_eq!(masked_glyphs("secret"), "••••••");
        assert_eq!(masked_glyphs("héllo"), "•••••");
        assert!(!masked_glyphs("secret").contains("secret"));
    }

    #[test]
    fn offsets_round_trip_across_multibyte_scalars() {
        let content = "héllo";
        let mut byte = 0;
        let chars = content.chars().count();
        for glyph in 0..=chars {
            assert_eq!(
                content_to_mask_offset(content, byte),
                glyph * MASK.len_utf8(),
                "glyph {glyph}"
            );
            assert_eq!(
                mask_offset_to_content(content, glyph * MASK.len_utf8()),
                byte
            );
            if let Some(ch) = content[byte..].chars().next() {
                byte += ch.len_utf8();
            }
        }
    }

    #[test]
    fn a_mid_bullet_index_snaps_to_a_boundary() {
        let size = MASK.len_utf8();
        assert_eq!(snap_mask_offset(0), 0);
        assert_eq!(snap_mask_offset(1), 0);
        assert_eq!(snap_mask_offset(size), size);
        assert_eq!(mask_offset_to_content("ab", 1), 0);
        assert_eq!(mask_offset_to_content("ab", size + 1), 1);
    }

    #[test]
    fn inspection_hides_a_secure_value() {
        let shown = inspection_value("desk-lamp-phrase", true);
        assert_eq!(shown, "••••••••••••••••");
        assert!(!shown.contains("desk"));
        assert_eq!(inspection_value("Harbor", false), "Harbor");
    }
}
