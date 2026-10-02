use atelier_ui::{
    motion,
    prelude::*,
    tokens::{MotionDuration, Spring},
};

use super::{caption, example, metadata};

const TRACK_WIDTH: f32 = 360.0;
const DOT: f32 = 16.0;

#[derive(Default)]
struct Demo {
    at_end: bool,
    generation: usize,
}

#[derive(Clone, Copy)]
enum Token {
    Timed(MotionDuration),
    Spring(Spring),
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme().clone();
    let demo = window.use_keyed_state("motion-demo", cx, |_, _| Demo::default());
    let (at_end, generation) = {
        let d = demo.read(cx);
        (d.at_end, d.generation)
    };
    let reduced = cx.ui_preferences().motion == MotionPreference::Reduced;

    let tokens = [
        Token::Timed(MotionDuration::Fast),
        Token::Timed(MotionDuration::Standard),
        Token::Timed(MotionDuration::Slow),
        Token::Spring(Spring::Snappy),
        Token::Spring(Spring::Smooth),
        Token::Spring(Spring::Gentle),
    ];

    let travel = TRACK_WIDTH - DOT;
    let (from, to) = if at_end { (0.0, travel) } else { (travel, 0.0) };
    let mut rows = v_stack(Space::S4);
    for (index, token) in tokens.into_iter().enumerate() {
        let (name, detail, animation) = match token {
            Token::Timed(t) => (
                t.token_name(),
                format!("{} ms", theme.motion.duration(t).as_millis()),
                motion::timed(cx, t),
            ),
            Token::Spring(s) => {
                let p = theme.motion.spring(s);
                (
                    s.token_name(),
                    format!("response {:.2}s · damping {:.2}", p.response, p.damping),
                    motion::spring(cx, s),
                )
            }
        };
        let dot = div()
            .absolute()
            .top(px(0.0))
            .size(px(DOT))
            .rounded_full()
            .bg(theme.colors.control.accent);
        let dot = match animation.filter(|_| generation > 0) {
            Some(animation) => dot
                .with_animation(
                    ("motion-dot", index * 10_000 + generation),
                    animation,
                    move |el, t| el.left(px(from + (to - from) * t)),
                )
                .into_any_element(),
            None => dot.left(px(to)).into_any_element(),
        };
        rows = rows.child(
            h_stack(Space::S4)
                .child(
                    v_stack(Space::S0)
                        .w(px(220.0))
                        .child(metadata(name))
                        .child(caption(detail)),
                )
                .child(
                    div()
                        .relative()
                        .w(px(TRACK_WIDTH))
                        .h(px(DOT))
                        .rounded_full()
                        .bg(theme.colors.control.neutral)
                        .child(dot),
                ),
        );
    }

    let controls = h_stack(Space::S3)
        .child(
            Button::new("motion-play", "Play transition")
                .variant(ButtonVariant::Primary)
                .icon(IconName::Play)
                .on_click(move |_, _, cx| {
                    demo.update(cx, |d, cx| {
                        d.at_end = !d.at_end;
                        d.generation += 1;
                        cx.notify();
                    })
                }),
        )
        .child(caption(if reduced {
            "Reduced motion is on: dots move to their end state immediately."
        } else {
            "Toggle “Reduced motion” in the toolbar to compare."
        }));

    v_stack(Space::S8)
        .child(example(
            "Durations and springs",
            "Each track uses one token. Components never specify raw durations.",
            v_stack(Space::S6).child(controls).child(rows),
        ))
        .child(example(
            "Continuous motion",
            "Activity indicators spin only when motion is allowed; otherwise a static glyph conveys the same state.",
            h_stack(Space::S4)
                .child(Icon::new(IconName::Spinner).size(IconSize::Large).spinning(true))
                .child(Button::new("motion-loading", "Saving").loading(true)),
        ))
        .into_any_element()
}
