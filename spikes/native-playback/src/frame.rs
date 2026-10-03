//! Backend-agnostic helpers for CPU video frames headed to a GPUI image.

use std::time::Duration;

/// GPUI's `RenderImage` is BGRA with meaningful alpha, while mpv's `bgr0`
/// leaves the fourth byte uninitialized. Force every pixel opaque.
pub fn force_opaque_bgra(buffer: &mut [u8]) {
    for pixel in buffer.as_chunks_mut::<4>().0 {
        pixel[3] = 0xff;
    }
}

/// Fit `source` into `bounds` preserving aspect ratio, never upscaling, with
/// even dimensions (some scalers misbehave on odd sizes).
pub fn fit_within(source: (u32, u32), bounds: (u32, u32)) -> (u32, u32) {
    let (sw, sh) = (source.0.max(1) as f64, source.1.max(1) as f64);
    let scale = (bounds.0 as f64 / sw).min(bounds.1 as f64 / sh).min(1.0);
    let even = |value: f64| ((value as u32) & !1).max(2);
    (even(sw * scale), even(sh * scale))
}

/// Running min/mean/max for a per-frame cost.
#[derive(Debug, Default, Clone)]
pub struct Timing {
    count: u64,
    total: Duration,
    min: Option<Duration>,
    max: Duration,
}

impl Timing {
    pub fn record(&mut self, sample: Duration) {
        self.count += 1;
        self.total += sample;
        self.min = Some(self.min.map_or(sample, |min| min.min(sample)));
        self.max = self.max.max(sample);
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn mean_ms(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.total.as_secs_f64() * 1000.0 / self.count as f64
    }

    pub fn summary(&self) -> String {
        format!(
            "n={} mean={:.2}ms min={:.2}ms max={:.2}ms",
            self.count,
            self.mean_ms(),
            self.min.unwrap_or_default().as_secs_f64() * 1000.0,
            self.max.as_secs_f64() * 1000.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forces_alpha_without_touching_color() {
        let mut pixels = vec![1, 2, 3, 0, 4, 5, 6, 7];
        force_opaque_bgra(&mut pixels);
        assert_eq!(pixels, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn fits_without_upscaling_and_keeps_aspect() {
        assert_eq!(fit_within((1920, 1080), (1280, 1280)), (1280, 720));
        assert_eq!(fit_within((1280, 720), (3840, 2160)), (1280, 720));
        assert_eq!(fit_within((1921, 1081), (4000, 4000)), (1920, 1080));
    }

    #[test]
    fn timing_summarizes_samples() {
        let mut timing = Timing::default();
        timing.record(Duration::from_millis(2));
        timing.record(Duration::from_millis(4));
        assert_eq!(timing.count(), 2);
        assert!((timing.mean_ms() - 3.0).abs() < 1e-9);
    }
}
