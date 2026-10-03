//! `matinee-player`: the boundary for Matinee's native playback engine.
//!
//! All video decoding, frame production, and transport logic will live here,
//! never in the generic UI framework (`atelier-ui`).
//!
//! The feasibility spike (`spikes/native-playback/`, findings in
//! `docs/architecture/playback.md`) recommends **libmpv** as the engine,
//! using mpv's software render API on a render thread. Frames are handed to
//! the UI as BGRA buffers and painted through a GPUI `RenderImage`. Only LGPL
//! builds of libmpv and FFmpeg may be distributed.
//!
//! The crate deliberately exposes no API in Phase 0. The engine and its API
//! are Phase 1 work (`docs/migration/roadmap.md`). Painting is planned as a
//! generic frame-surface element in `atelier-ui`, so this crate never needs
//! to depend on GPUI directly.
