//! `matinee-player` — the boundary for Matinee's native playback engine.
//!
//! This crate exists in Phase 0 only to reserve the seam: all video
//! decoding, rendering, and transport logic will live here, never in the
//! generic UI framework (`atelier-ui`). No playback backend has been chosen
//! yet; the feasibility evaluation (libmpv, GStreamer, FFmpeg-based
//! approaches) and its recommendation are recorded in
//! `docs/architecture/playback.md`, with the standalone experiment under
//! `spikes/native-playback/`.
//!
//! Until a backend is selected, this crate deliberately exposes no API so
//! that no caller can couple to a speculative abstraction.
