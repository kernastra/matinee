/// The spacing unit in logical pixels. Every spacing token is a multiple of it.
pub const SPACING_UNIT: f32 = 4.0;

/// Spacing scale tokens (`space.1` = 4px, `space.2` = 8px, ...).
///
/// The scale is framework-wide rather than per-theme so that layout rhythm
/// stays identical across themes; density modes can scale `SPACING_UNIT` later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Space {
    S0,
    /// Half step for hairline gaps (2px).
    Half,
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    S8,
    S10,
    S12,
    S16,
}

impl Space {
    pub const ALL: [Space; 12] = [
        Space::S0,
        Space::Half,
        Space::S1,
        Space::S2,
        Space::S3,
        Space::S4,
        Space::S5,
        Space::S6,
        Space::S8,
        Space::S10,
        Space::S12,
        Space::S16,
    ];

    pub const fn steps(self) -> f32 {
        match self {
            Space::S0 => 0.0,
            Space::Half => 0.5,
            Space::S1 => 1.0,
            Space::S2 => 2.0,
            Space::S3 => 3.0,
            Space::S4 => 4.0,
            Space::S5 => 5.0,
            Space::S6 => 6.0,
            Space::S8 => 8.0,
            Space::S10 => 10.0,
            Space::S12 => 12.0,
            Space::S16 => 16.0,
        }
    }

    /// Logical pixels.
    pub const fn value(self) -> f32 {
        self.steps() * SPACING_UNIT
    }

    pub const fn token_name(self) -> &'static str {
        match self {
            Space::S0 => "space.0",
            Space::Half => "space.0_5",
            Space::S1 => "space.1",
            Space::S2 => "space.2",
            Space::S3 => "space.3",
            Space::S4 => "space.4",
            Space::S5 => "space.5",
            Space::S6 => "space.6",
            Space::S8 => "space.8",
            Space::S10 => "space.10",
            Space::S12 => "space.12",
            Space::S16 => "space.16",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_is_strictly_increasing_on_the_unit_grid() {
        let values: Vec<f32> = Space::ALL.iter().map(|s| s.value()).collect();
        assert!(values.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(Space::S1.value(), 4.0);
        assert_eq!(Space::S4.value(), 16.0);
        assert!(values.iter().all(|v| v % 2.0 == 0.0));
    }
}
