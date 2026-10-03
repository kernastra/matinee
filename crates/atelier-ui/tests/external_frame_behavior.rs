use atelier_ui::{BgraFrame, ExternalFrameSurface};
use gpui::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, TestAppContext, Window, div,
};

struct Harness {
    surface: Option<gpui::Entity<ExternalFrameSurface>>,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().children(self.surface.clone())
    }
}

fn solid(generation: u64, byte: u8) -> BgraFrame {
    BgraFrame::new(2, 2, 8, vec![byte; 16], generation).unwrap()
}

#[gpui::test]
fn latest_unpublished_frame_stays_one_deep_and_drop_releases_painted_frames(
    cx: &mut TestAppContext,
) {
    let (harness, window) = cx.add_window_view(|window, cx| {
        let surface = cx.new(|cx| ExternalFrameSurface::new("No frame", window, cx));
        Harness {
            surface: Some(surface),
        }
    });

    let mailbox = window.update(|_, cx| {
        harness
            .read(cx)
            .surface
            .as_ref()
            .expect("surface")
            .read(cx)
            .mailbox()
    });

    mailbox.publish(solid(1, 1)).unwrap();
    mailbox.publish(solid(2, 9)).unwrap();
    let replaced = mailbox.stats();
    assert_eq!(replaced.published, 2);
    assert_eq!(replaced.replaced, 1);
    assert_eq!(replaced.depth, 1, "unpublished depth cannot exceed one");

    window.run_until_parked();
    window.update(|window, cx| {
        let surface = harness.read(cx).surface.clone().expect("surface");
        surface.update(cx, |surface, _| {
            assert_eq!(surface.generation(), Some(2));
            assert_eq!(surface.presented_count(), 1);
            assert_eq!(mailbox.stats().depth, 0);
        });
        // Upload the presented image. GPU work starts here, not in publish.
        window.draw(cx).clear();
    });

    mailbox.publish(solid(3, 3)).unwrap();
    window.run_until_parked();
    window.update(|window, cx| {
        let surface = harness.read(cx).surface.clone().expect("surface");
        surface.update(cx, |surface, _| {
            assert_eq!(surface.generation(), Some(3));
            assert_eq!(surface.presented_count(), 2);
        });
        window.draw(cx).clear();
        surface.update(cx, |surface, _| {
            surface.release(window);
            assert!(surface.generation().is_none());
        });
    });

    // A frame published after release is still only the unpublished slot.
    mailbox.publish(solid(4, 4)).unwrap();
    mailbox.publish(solid(5, 5)).unwrap();
    assert_eq!(mailbox.stats().depth, 1);
    assert_eq!(mailbox.stats().replaced, 2);

    window.run_until_parked();
    window.update(|window, cx| {
        window.draw(cx).clear();
        harness.update(cx, |harness, cx| {
            harness.surface.take();
            cx.notify();
        });
    });
    window.run_until_parked();
    window.update(|window, cx| {
        window.draw(cx).clear();
    });
}
