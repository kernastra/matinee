//! Determinate and indeterminate progress.
//!
//! A generic meter. A transport timeline is a different control. The track is
//! 4px, the same compact height as a slider track.

use gpui::{
    AnimationExt, App, IntoElement, ParentElement, RenderOnce, Styled, Window, div, relative,
};

use crate::{
    ActiveTheme, motion,
    tokens::{MotionDuration, Radius, Space},
};

const TRACK: f32 = 4.0;
/// Portion of the track the indeterminate segment occupies.
const INDETERMINATE_FRACTION: f32 = 0.35;

/// Clamps a determinate fraction to `0..=1`. Non-finite values become `0`.
pub fn clamp_progress(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// A horizontal progress meter.
///
/// `None` is indeterminate. Under reduced motion the indeterminate segment
/// stays put instead of traveling.
#[derive(IntoElement)]
pub struct ProgressBar {
    value: Option<f32>,
    disabled: bool,
    /// Accessible name. Not rendered; reserved for a future accessibility tree.
    label: Option<gpui::SharedString>,
}

impl ProgressBar {
    pub fn determinate(value: f32) -> Self {
        Self {
            value: Some(value),
            disabled: false,
            label: None,
        }
    }

    pub fn indeterminate() -> Self {
        Self {
            value: None,
            disabled: false,
            label: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn label(mut self, label: impl Into<gpui::SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn fraction(&self) -> Option<f32> {
        self.value.map(clamp_progress)
    }

    pub fn name(&self) -> Option<&str> {
        self.label.as_ref().map(gpui::SharedString::as_ref)
    }
}

impl RenderOnce for ProgressBar {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let fill = if self.disabled {
            theme.colors.text.disabled
        } else {
            theme.colors.control.accent
        };
        let track = theme.colors.border.subtle;
        let radius = theme.radius.get(Radius::Full);
        let reduced = !theme.motion.allows_continuous(cx.ui_preferences().motion());
        let bar = match self.fraction() {
            Some(fraction) => div()
                .h_full()
                .w(relative(fraction))
                .bg(fill)
                .into_any_element(),
            None if self.disabled || reduced => div()
                .absolute()
                .left(gpui::px(0.0))
                .h_full()
                .w(relative(INDETERMINATE_FRACTION))
                .bg(fill)
                .into_any_element(),
            None => {
                let segment = div()
                    .absolute()
                    .top(gpui::px(0.0))
                    .h_full()
                    .w(relative(INDETERMINATE_FRACTION))
                    .bg(fill);
                if let Some(animation) = motion::timed(cx, MotionDuration::Slow) {
                    segment
                        .with_animation(
                            "progress-indeterminate",
                            animation.repeat(),
                            |segment, delta| {
                                segment.left(relative(delta * (1.0 - INDETERMINATE_FRACTION)))
                            },
                        )
                        .into_any_element()
                } else {
                    segment.into_any_element()
                }
            }
        };

        div()
            .w_full()
            .h(gpui::px(TRACK))
            .rounded(gpui::px(radius))
            .bg(track)
            .overflow_hidden()
            .relative()
            .mt(Space::S0.px())
            .child(bar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Theme;
    use crate::tokens::MotionPreference;

    #[test]
    fn progress_clamps_to_the_unit_interval() {
        assert_eq!(clamp_progress(-0.2), 0.0);
        assert_eq!(clamp_progress(0.4), 0.4);
        assert_eq!(clamp_progress(2.0), 1.0);
        assert_eq!(clamp_progress(f32::NAN), 0.0);
        assert_eq!(clamp_progress(f32::INFINITY), 0.0);
    }

    #[test]
    fn reduced_motion_disables_indeterminate_travel() {
        let theme = Theme::neutral_dark();
        assert!(!theme.motion.allows_continuous(MotionPreference::Reduced));
        assert!(theme.motion.allows_continuous(MotionPreference::Full));
        assert!(ProgressBar::indeterminate().fraction().is_none());
        assert_eq!(ProgressBar::determinate(1.4).fraction(), Some(1.0));
    }
}
