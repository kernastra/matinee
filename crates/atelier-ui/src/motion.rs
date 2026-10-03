//! Resolves motion tokens into GPUI animations, honoring reduced motion.
//!
//! Both constructors return `None` when motion is reduced; callers must then
//! render the end state directly. This keeps the reduced-motion decision in
//! one place instead of in every component.

use gpui::{Animation, App};

use crate::{
    ActiveTheme,
    tokens::{MotionDuration, Spring},
};

/// A timed transition using the framework's standard deceleration curve.
pub fn timed(cx: &App, duration: MotionDuration) -> Option<Animation> {
    let theme = cx.theme();
    let duration = theme
        .motion
        .resolve(duration, cx.ui_preferences().motion())?;
    Some(Animation::new(duration).with_easing(gpui::ease_out_quint()))
}

/// A spring-driven transition. Duration is the spring's settle time.
pub fn spring(cx: &App, spring: Spring) -> Option<Animation> {
    let theme = cx.theme();
    // Springs share the reduced-motion rule of timed transitions.
    theme
        .motion
        .resolve(MotionDuration::Standard, cx.ui_preferences().motion())?;
    let params = theme.motion.spring(spring);
    Some(Animation::new(params.settle_duration()).with_easing(move |t| params.sample(t)))
}
