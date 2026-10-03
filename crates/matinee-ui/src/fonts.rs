//! Bundled Matinee fonts (SIL Open Font License 1.1).
//!
//! Fraunces and Manrope are static instances of the variable sources, with
//! name tables set to the family and weight the theme asks for. GPUI 0.2.2
//! does not apply variable axes, and fontconfig weight matching on partial
//! variable fonts is unreliable. IBM Plex Mono is the official Regular file.
//! One Fraunces optical size is included (72pt).

use std::borrow::Cow;

use atelier_ui::prelude::App;

const FRAUNCES_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Fraunces-SemiBold.ttf");
const MANROPE_REGULAR: &[u8] = include_bytes!("../assets/fonts/Manrope-Regular.ttf");
const MANROPE_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Manrope-SemiBold.ttf");
const PLEX_MONO_REGULAR: &[u8] = include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf");

pub fn bundled_font_data() -> [Cow<'static, [u8]>; 4] {
    [
        Cow::Borrowed(FRAUNCES_SEMIBOLD),
        Cow::Borrowed(MANROPE_REGULAR),
        Cow::Borrowed(MANROPE_SEMIBOLD),
        Cow::Borrowed(PLEX_MONO_REGULAR),
    ]
}

/// Registers the bundled faces. Call before opening windows.
pub fn load_bundled_fonts(cx: &mut App) -> atelier_ui::gpui::Result<()> {
    atelier_ui::add_fonts(cx, bundled_font_data())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_sfnt_and_name_the_theme_families() {
        assert_family(FRAUNCES_SEMIBOLD, "Fraunces");
        assert_family(MANROPE_REGULAR, "Manrope");
        assert_family(MANROPE_SEMIBOLD, "Manrope");
        assert_family(PLEX_MONO_REGULAR, "IBM Plex Mono");
    }

    #[test]
    fn licenses_are_the_sil_open_font_license() {
        let licenses = [
            include_str!("../assets/fonts/OFL-Fraunces.txt"),
            include_str!("../assets/fonts/OFL-Manrope.txt"),
            include_str!("../assets/fonts/OFL-IBMPlexMono.txt"),
        ];
        for license in licenses {
            assert!(license.contains("SIL Open Font License"));
        }
    }

    fn assert_family(bytes: &[u8], family: &str) {
        assert_eq!(&bytes[..4], &[0x00, 0x01, 0x00, 0x00], "{family} sfnt");
        let encoded: Vec<u8> = family
            .encode_utf16()
            .flat_map(|unit| unit.to_be_bytes())
            .collect();
        assert!(
            bytes.windows(encoded.len()).any(|window| window == encoded),
            "{family} name table"
        );
    }
}
