use super::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Radius {
    None,
    Small,
    Medium,
    Large,
    Full,
}

impl Radius {
    pub const ALL: [Radius; 5] = [
        Radius::None,
        Radius::Small,
        Radius::Medium,
        Radius::Large,
        Radius::Full,
    ];

    pub const fn token_name(self) -> &'static str {
        match self {
            Radius::None => "radius.none",
            Radius::Small => "radius.small",
            Radius::Medium => "radius.medium",
            Radius::Large => "radius.large",
            Radius::Full => "radius.full",
        }
    }
}

/// Corner radii in logical pixels. Themes may change the character of
/// corners (e.g. softer or squarer) without touching components.
#[derive(Clone, Debug, PartialEq)]
pub struct RadiusScale {
    pub small: f32,
    pub medium: f32,
    pub large: f32,
}

impl RadiusScale {
    /// Large enough to produce a capsule for any control height.
    pub const FULL: f32 = 9999.0;

    pub const fn standard() -> Self {
        Self {
            small: 4.0,
            medium: 6.0,
            large: 10.0,
        }
    }

    pub fn get(&self, radius: Radius) -> f32 {
        match radius {
            Radius::None => 0.0,
            Radius::Small => self.small,
            Radius::Medium => self.medium,
            Radius::Large => self.large,
            Radius::Full => Self::FULL,
        }
    }
}

/// Elevation levels, from flush with the canvas to modal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Elevation {
    Flat,
    Raised,
    Overlay,
    Modal,
}

impl Elevation {
    pub const ALL: [Elevation; 4] = [
        Elevation::Flat,
        Elevation::Raised,
        Elevation::Overlay,
        Elevation::Modal,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ElevationScale {
    pub raised: Vec<Shadow>,
    pub overlay: Vec<Shadow>,
    pub modal: Vec<Shadow>,
}

impl ElevationScale {
    /// Two-layer shadows (tight contact + soft ambient), tinted by `shadow`.
    pub fn from_shadow_color(shadow: Color) -> Self {
        let layer = |offset_y, blur, alpha| Shadow {
            offset_y,
            blur,
            spread: 0.0,
            color: shadow.with_alpha(alpha),
        };
        Self {
            raised: vec![layer(1.0, 2.0, 0.18), layer(2.0, 8.0, 0.12)],
            overlay: vec![layer(2.0, 4.0, 0.20), layer(8.0, 24.0, 0.22)],
            modal: vec![layer(4.0, 8.0, 0.24), layer(16.0, 48.0, 0.32)],
        }
    }

    pub fn get(&self, elevation: Elevation) -> &[Shadow] {
        match elevation {
            Elevation::Flat => &[],
            Elevation::Raised => &self.raised,
            Elevation::Overlay => &self.overlay,
            Elevation::Modal => &self.modal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_scale_is_ordered() {
        let scale = RadiusScale::standard();
        let values: Vec<f32> = Radius::ALL.iter().map(|r| scale.get(*r)).collect();
        assert!(values.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn elevation_blur_grows_with_level() {
        let scale = ElevationScale::from_shadow_color(Color::hex(0x000000));
        let max_blur = |e| {
            scale
                .get(e)
                .iter()
                .map(|s: &Shadow| s.blur)
                .fold(0.0, f32::max)
        };
        assert_eq!(max_blur(Elevation::Flat), 0.0);
        assert!(max_blur(Elevation::Raised) < max_blur(Elevation::Overlay));
        assert!(max_blur(Elevation::Overlay) < max_blur(Elevation::Modal));
    }
}
