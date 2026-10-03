use std::time::Duration;

/// Whether the user (or platform) asked for reduced motion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MotionPreference {
    #[default]
    Full,
    Reduced,
}

/// Duration tokens. Components never hard-code durations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MotionDuration {
    /// Micro-interactions: hover washes, press feedback.
    Fast,
    /// Default UI transitions: reveals, selection changes.
    Standard,
    /// Larger spatial transitions: sheets, page changes.
    Slow,
}

impl MotionDuration {
    pub const ALL: [MotionDuration; 3] = [
        MotionDuration::Fast,
        MotionDuration::Standard,
        MotionDuration::Slow,
    ];

    pub const fn token_name(self) -> &'static str {
        match self {
            MotionDuration::Fast => "motion.fast",
            MotionDuration::Standard => "motion.standard",
            MotionDuration::Slow => "motion.slow",
        }
    }
}

/// Spring tokens for physically-modelled motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Spring {
    /// Quick, almost no overshoot. Toggles, presses.
    Snappy,
    /// Critically damped. Default for spatial movement.
    Smooth,
    /// Slower, slight overshoot. Large surfaces entering.
    Gentle,
}

impl Spring {
    pub const ALL: [Spring; 3] = [Spring::Snappy, Spring::Smooth, Spring::Gentle];

    pub const fn token_name(self) -> &'static str {
        match self {
            Spring::Snappy => "spring.snappy",
            Spring::Smooth => "spring.smooth",
            Spring::Gentle => "spring.gentle",
        }
    }
}

/// Parameters of a damped harmonic oscillator, in the same terms SwiftUI uses:
/// `response` is the period of the undamped spring, `damping` is the damping
/// fraction (1.0 = critically damped, < 1.0 overshoots).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringParams {
    pub response: f32,
    pub damping: f32,
}

impl SpringParams {
    /// Time until the spring is within 0.1% of its target, used as the
    /// animation duration when driving the spring with a normalized clock.
    pub fn settle_duration(self) -> Duration {
        let omega = std::f32::consts::TAU / self.response;
        let zeta = self.damping.clamp(0.05, 1.0);
        Duration::from_secs_f32((6.9 / (zeta * omega)).clamp(0.05, 2.0))
    }

    /// Spring position at normalized time `t` in `[0, 1]` of `settle_duration`.
    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if t >= 1.0 {
            return 1.0;
        }
        let time = t * self.settle_duration().as_secs_f32();
        let omega = std::f32::consts::TAU / self.response;
        let zeta = self.damping.clamp(0.05, 1.0);
        if zeta >= 0.999 {
            1.0 - (1.0 + omega * time) * (-omega * time).exp()
        } else {
            let omega_d = omega * (1.0 - zeta * zeta).sqrt();
            let decay = (-zeta * omega * time).exp();
            1.0 - decay
                * ((omega_d * time).cos() + (zeta * omega / omega_d) * (omega_d * time).sin())
        }
    }
}

/// The motion vocabulary of a theme.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionScale {
    pub fast: Duration,
    pub standard: Duration,
    pub slow: Duration,
    pub snappy: SpringParams,
    pub smooth: SpringParams,
    pub gentle: SpringParams,
}

impl MotionScale {
    pub const fn standard() -> Self {
        Self {
            fast: Duration::from_millis(120),
            standard: Duration::from_millis(200),
            slow: Duration::from_millis(320),
            snappy: SpringParams {
                response: 0.25,
                damping: 0.9,
            },
            smooth: SpringParams {
                response: 0.4,
                damping: 1.0,
            },
            gentle: SpringParams {
                response: 0.55,
                damping: 0.8,
            },
        }
    }

    pub fn duration(&self, token: MotionDuration) -> Duration {
        match token {
            MotionDuration::Fast => self.fast,
            MotionDuration::Standard => self.standard,
            MotionDuration::Slow => self.slow,
        }
    }

    pub fn spring(&self, token: Spring) -> SpringParams {
        match token {
            Spring::Snappy => self.snappy,
            Spring::Smooth => self.smooth,
            Spring::Gentle => self.gentle,
        }
    }

    /// Resolves a duration under the active preference. `None` means
    /// "do not animate": apply the end state immediately.
    pub fn resolve(&self, token: MotionDuration, preference: MotionPreference) -> Option<Duration> {
        match preference {
            MotionPreference::Full => Some(self.duration(token)),
            MotionPreference::Reduced => None,
        }
    }

    /// Whether continuous, decorative motion (spinners, shimmer, parallax)
    /// may run. Under reduced motion components must render a static
    /// equivalent that conveys the same state.
    pub fn allows_continuous(&self, preference: MotionPreference) -> bool {
        preference == MotionPreference::Full
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_ordered() {
        let m = MotionScale::standard();
        assert!(m.fast < m.standard && m.standard < m.slow);
    }

    #[test]
    fn reduced_motion_disables_animation() {
        let m = MotionScale::standard();
        for token in MotionDuration::ALL {
            assert!(m.resolve(token, MotionPreference::Full).is_some());
            assert_eq!(m.resolve(token, MotionPreference::Reduced), None);
        }
        assert!(!m.allows_continuous(MotionPreference::Reduced));
    }

    #[test]
    fn springs_start_at_zero_and_settle_at_one() {
        let m = MotionScale::standard();
        for token in Spring::ALL {
            let spring = m.spring(token);
            assert!(spring.sample(0.0).abs() < 1e-4, "{token:?}");
            assert_eq!(spring.sample(1.0), 1.0, "{token:?}");
            assert!(
                (spring.sample(0.99) - 1.0).abs() < 0.01,
                "{token:?} settles"
            );
        }
    }

    #[test]
    fn only_underdamped_springs_overshoot() {
        let m = MotionScale::standard();
        let peak = |s: SpringParams| {
            (0..=200)
                .map(|i| s.sample(i as f32 / 200.0))
                .fold(0.0, f32::max)
        };
        assert!(peak(m.smooth) <= 1.0 + 1e-4);
        assert!(peak(m.gentle) > 1.0);
    }
}
