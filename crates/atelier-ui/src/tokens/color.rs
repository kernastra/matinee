/// A framework-owned sRGB color with straight (non-premultiplied) alpha.
///
/// Tokens are expressed in this type rather than GPUI's color types so that
/// themes, token tests, and contrast checks never depend on the renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Builds an opaque color from `0xRRGGBB`.
    pub const fn hex(rgb: u32) -> Self {
        Self {
            r: ((rgb >> 16) & 0xff) as f32 / 255.0,
            g: ((rgb >> 8) & 0xff) as f32 / 255.0,
            b: (rgb & 0xff) as f32 / 255.0,
            a: 1.0,
        }
    }

    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Linear interpolation in sRGB space; `t = 0` is `self`, `t = 1` is `other`.
    pub fn mix(self, other: Color, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        Self {
            r: lerp(self.r, other.r),
            g: lerp(self.g, other.g),
            b: lerp(self.b, other.b),
            a: lerp(self.a, other.a),
        }
    }

    /// Composites `self` over an opaque `backdrop`, returning an opaque color.
    pub fn over(self, backdrop: Color) -> Self {
        let a = self.a.clamp(0.0, 1.0);
        Self {
            r: self.r * a + backdrop.r * (1.0 - a),
            g: self.g * a + backdrop.g * (1.0 - a),
            b: self.b * a + backdrop.b * (1.0 - a),
            a: 1.0,
        }
    }

    /// WCAG 2.x relative luminance of the opaque color.
    pub fn relative_luminance(self) -> f32 {
        fn channel(c: f32) -> f32 {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// WCAG 2.x contrast ratio, compositing `self` over `background` first.
    pub fn contrast_ratio(self, background: Color) -> f32 {
        let fg = self.over(background).relative_luminance();
        let bg = background.relative_luminance();
        let (hi, lo) = if fg > bg { (fg, bg) } else { (bg, fg) };
        (hi + 0.05) / (lo + 0.05)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_decodes_channels() {
        let c = Color::hex(0xff8000);
        assert_eq!(c.r, 1.0);
        assert!((c.g - 128.0 / 255.0).abs() < f32::EPSILON);
        assert_eq!(c.b, 0.0);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn contrast_matches_wcag_reference_values() {
        let black = Color::hex(0x000000);
        let white = Color::hex(0xffffff);
        assert!((white.contrast_ratio(black) - 21.0).abs() < 0.01);
        assert!((black.contrast_ratio(black) - 1.0).abs() < 0.01);
        // #777777 on white is the classic ~4.48:1 example.
        assert!((Color::hex(0x777777).contrast_ratio(white) - 4.48).abs() < 0.02);
    }

    #[test]
    fn translucent_foreground_is_composited_before_contrast() {
        let bg = Color::hex(0x000000);
        let half_white = Color::hex(0xffffff).with_alpha(0.5);
        let ratio = half_white.contrast_ratio(bg);
        assert!(ratio > 1.0 && ratio < 21.0);
    }

    #[test]
    fn mix_endpoints() {
        let a = Color::hex(0x000000);
        let b = Color::hex(0xffffff);
        assert_eq!(a.mix(b, 0.0), a);
        assert_eq!(a.mix(b, 1.0), b);
    }
}
