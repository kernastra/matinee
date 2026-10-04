//! CPU frames in, Atelier frames out. The UI keeps only the newest one.

use atelier_ui::{BgraFrame, FrameError};
use matinee_player::CpuFrame;

use super::model::PlayerPreview;

/// Hand a decoded frame to the external surface. The caller already took it
/// off the player, so the mailbox replaces anything not yet painted.
pub(crate) fn bgra_from_cpu(frame: CpuFrame) -> Result<BgraFrame, FrameError> {
    let width = frame.width();
    let height = frame.height();
    let stride = frame.stride();
    let generation = frame.generation();
    BgraFrame::new(width, height, stride, frame.into_pixels(), generation)
}

/// A generated 16:9 picture for review captures. Not a film frame.
pub(crate) fn fixture_frame(preview: PlayerPreview) -> BgraFrame {
    const WIDTH: u32 = 640;
    const HEIGHT: u32 = 360;
    let mut pixels = vec![0_u8; (WIDTH * HEIGHT * 4) as usize];
    let dim = matches!(preview, PlayerPreview::Error | PlayerPreview::Paused);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let index = ((y * WIDTH + x) * 4) as usize;
            let band = (x * 5) / WIDTH;
            let (blue, green, red): (u8, u8, u8) = match band {
                0 => (92, 48, 24),
                1 => (36, 72, 110),
                2 => (28, 64, 48),
                3 => (48, 36, 84),
                _ => (24, 28, 36),
            };
            let scale: u16 = if dim { 150 } else { 255 };
            let tone = |channel: u8| u8::try_from(u16::from(channel) * scale / 255).unwrap_or(255);
            pixels[index] = tone(blue);
            pixels[index + 1] = tone(green);
            pixels[index + 2] = tone(red);
            pixels[index + 3] = 0xff;
        }
    }
    paint_mark(&mut pixels, WIDTH, preview);
    BgraFrame::new(WIDTH, HEIGHT, WIDTH * 4, pixels, 1).expect("fixture frame is in range")
}

fn paint_mark(pixels: &mut [u8], width: u32, preview: PlayerPreview) {
    let origin_x = (width / 2) as i32;
    let origin_y = 180_i32;
    match preview {
        PlayerPreview::Paused | PlayerPreview::Error => {
            fill_rect(pixels, width, origin_x - 28, origin_y - 36, 16, 72);
            fill_rect(pixels, width, origin_x + 12, origin_y - 36, 16, 72);
        }
        PlayerPreview::Playing
        | PlayerPreview::Controls
        | PlayerPreview::AudioMenu
        | PlayerPreview::SubtitleMenu => {
            for row in -36_i32..36 {
                let half = (36 - row.abs()) / 2;
                fill_rect(pixels, width, origin_x - 10, origin_y + row, half, 1);
            }
        }
    }
}

fn fill_rect(pixels: &mut [u8], width: u32, x: i32, y: i32, w: i32, h: i32) {
    let height = pixels.len() / (width as usize * 4);
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                continue;
            }
            let index = ((py as u32 * width + px as u32) * 4) as usize;
            pixels[index] = 232;
            pixels[index + 1] = 214;
            pixels[index + 2] = 176;
            pixels[index + 3] = 0xff;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preview_builds_a_fixture_frame() {
        for scene in [
            PlayerPreview::Playing,
            PlayerPreview::Paused,
            PlayerPreview::Controls,
            PlayerPreview::Error,
            PlayerPreview::AudioMenu,
            PlayerPreview::SubtitleMenu,
        ] {
            let _ = fixture_frame(scene);
        }
    }
}
