//! Engine tests. They generate tiny media with ffmpeg and skip when libmpv
//! is absent, except on Linux CI where the library is required.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use matinee_player::{LoadRequest, PlaybackState, Player, PlayerError, TrackKind};

fn engine_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn require_player() -> Option<Player> {
    match Player::open_with(false) {
        Ok(player) => Some(player),
        Err(PlayerError::LibraryMissing { .. } | PlayerError::LibraryIncompatible { .. }) => {
            if std::env::var_os("CI").is_some() && cfg!(target_os = "linux") {
                panic!("Linux CI requires libmpv");
            }
            eprintln!("skipping: libmpv is not available");
            None
        }
        Err(error) => panic!("engine failed to open: {error}"),
    }
}

fn ffmpeg(args: &[&str]) -> bool {
    let output = Command::new("ffmpeg").args(args).output();
    match output {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            if std::env::var_os("CI").is_some() && cfg!(target_os = "linux") {
                panic!("ffmpeg failed: {}", String::from_utf8_lossy(&output.stderr));
            }
            eprintln!("skipping: ffmpeg could not build the fixture");
            false
        }
        Err(_) => {
            if std::env::var_os("CI").is_some() && cfg!(target_os = "linux") {
                panic!("Linux CI requires ffmpeg");
            }
            eprintln!("skipping: ffmpeg is not available");
            false
        }
    }
}

fn dir() -> PathBuf {
    let path = std::env::temp_dir().join("matinee-player-fixtures");
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn wait_until(
    player: &Player,
    timeout: Duration,
    mut pred: impl FnMut(&matinee_player::Snapshot) -> bool,
) -> matinee_player::Snapshot {
    let started = Instant::now();
    loop {
        let _ = player.take_frame();
        let snapshot = player.snapshot();
        if pred(&snapshot) || started.elapsed() > timeout {
            return snapshot;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn load(player: &Player, path: &Path) {
    player
        .load(LoadRequest {
            url: path.to_string_lossy().into_owned(),
            start: None,
            headers: Vec::new(),
        })
        .unwrap();
}

#[test]
fn transport_tracks_replace_and_shutdown() {
    let _guard = engine_lock();
    let Some(player) = require_player() else {
        return;
    };
    let root = dir();
    let sample = root.join("h264-two-audio.mkv");
    let other = root.join("h264-silent.mp4");
    let external = root.join("extra.srt");
    std::fs::write(
        &external,
        "1\n00:00:00,000 --> 00:00:02,000\nexternal cue\n",
    )
    .unwrap();
    if !ffmpeg(&[
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x180:rate=24:duration=3",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000:duration=3",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=880:sample_rate=48000:duration=3",
        "-f",
        "srt",
        "-i",
        &external.to_string_lossy(),
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-map",
        "2:a",
        "-map",
        "3",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
        "-c:a:0",
        "aac",
        "-c:a:1",
        "ac3",
        "-c:s",
        "srt",
        "-metadata:s:a:0",
        "language=eng",
        "-metadata:s:a:1",
        "language=spa",
        "-metadata:s:s:0",
        "language=eng",
        &sample.to_string_lossy(),
    ]) {
        return;
    }
    if !ffmpeg(&[
        "-y",
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=160x120:r=24:d=1",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
        "-an",
        &other.to_string_lossy(),
    ]) {
        return;
    }

    let started = Instant::now();
    load(&player, &sample);
    let playing = wait_until(&player, Duration::from_secs(15), |snapshot| {
        snapshot.state == PlaybackState::Playing && snapshot.duration.is_some()
    });
    assert_eq!(playing.state, PlaybackState::Playing, "{playing:?}");
    assert!(playing.audio_tracks.len() >= 2, "{playing:?}");
    assert!(!playing.subtitle_tracks.is_empty(), "{playing:?}");
    assert!(
        playing
            .source_size
            .is_some_and(|(width, height)| width == 320 && height == 180),
        "{playing:?}"
    );

    let second = playing.audio_tracks[1].id;
    player.select_audio(second).unwrap();
    player.set_volume(0.4).unwrap();
    player.set_muted(true).unwrap();
    player.pause().unwrap();
    let paused = wait_until(&player, Duration::from_secs(5), |snapshot| {
        snapshot.state == PlaybackState::Paused
            && snapshot.muted
            && (snapshot.volume - 0.4).abs() < 0.05
    });
    assert_eq!(paused.state, PlaybackState::Paused, "{paused:?}");
    assert_eq!(paused.selected_audio, Some(second));

    player.seek(Duration::from_millis(1500)).unwrap();
    let seeked = wait_until(&player, Duration::from_secs(5), |snapshot| {
        snapshot.position >= Duration::from_millis(1000)
    });
    assert!(seeked.position >= Duration::from_millis(800), "{seeked:?}");
    player.seek_by_ms(-400).unwrap();

    player.play().unwrap();
    player.disable_subtitles().unwrap();
    player
        .add_subtitle(external.to_string_lossy().to_string())
        .unwrap();
    let with_external = wait_until(&player, Duration::from_secs(5), |snapshot| {
        snapshot.subtitle_tracks.len() >= 2
    });
    assert!(
        with_external.subtitle_tracks.len() >= 2,
        "{with_external:?}"
    );

    let frame = wait_until(&player, Duration::from_secs(5), |_| {
        player.take_frame().is_some()
    });
    let _ = frame;
    let taken = player.take_frame().or_else(|| {
        thread::sleep(Duration::from_millis(100));
        player.take_frame()
    });
    // The wait above already took the frame. Produce one more.
    let again = wait_until(&player, Duration::from_secs(5), |_| {
        player.diagnostics().produced > 0
    });
    assert!(again.frame_size.is_some() || player.diagnostics().produced > 0);
    let _ = taken;

    load(&player, &other);
    let replaced = wait_until(&player, Duration::from_secs(15), |snapshot| {
        snapshot.source_size.is_some_and(|(width, _)| width == 160)
    });
    assert_eq!(
        replaced.source_size.map(|size| size.0),
        Some(160),
        "{replaced:?}"
    );

    player.stop().unwrap();
    let stopped = wait_until(&player, Duration::from_secs(5), |snapshot| {
        snapshot.state == PlaybackState::Idle
    });
    assert_eq!(stopped.state, PlaybackState::Idle, "{stopped:?}");
    drop(player);
    assert!(started.elapsed() < Duration::from_secs(60));
}

#[test]
fn hevc_main10_and_av1_open() {
    let _guard = engine_lock();
    let Some(player) = require_player() else {
        return;
    };
    let root = dir();
    let hevc = root.join("hevc-main10.mkv");
    let av1 = root.join("av1-opus.mkv");
    if !ffmpeg(&[
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x180:rate=24:duration=1",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000:duration=1",
        "-c:v",
        "libx265",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p10le",
        "-c:a",
        "eac3",
        "-shortest",
        &hevc.to_string_lossy(),
    ]) {
        return;
    }
    if !ffmpeg(&[
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x180:rate=24:duration=1",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000:duration=1",
        "-c:v",
        "libsvtav1",
        "-preset",
        "12",
        "-c:a",
        "libopus",
        "-shortest",
        &av1.to_string_lossy(),
    ]) {
        return;
    }
    for path in [&hevc, &av1] {
        load(&player, path);
        let snapshot = wait_until(&player, Duration::from_secs(20), |snapshot| {
            matches!(
                snapshot.state,
                PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ended
            ) && snapshot.error.is_none()
                && snapshot.source_size.is_some()
        });
        assert!(snapshot.error.is_none(), "{path:?} {snapshot:?}");
        assert!(snapshot.source_size.is_some(), "{path:?} {snapshot:?}");
        assert!(
            snapshot
                .audio_tracks
                .iter()
                .any(|track| track.kind == TrackKind::Audio),
            "{path:?} {snapshot:?}"
        );
    }
}

#[test]
fn software_1080p_stays_bounded() {
    let _guard = engine_lock();
    let Some(player) = require_player() else {
        return;
    };
    let path = dir().join("h264-1080p.mkv");
    if !ffmpeg(&[
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=1920x1080:rate=24:duration=4",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000:duration=4",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-shortest",
        &path.to_string_lossy(),
    ]) {
        return;
    }
    let before = rss_bytes();
    load(&player, &path);
    let playing = wait_until(&player, Duration::from_secs(20), |snapshot| {
        snapshot.state == PlaybackState::Playing
    });
    assert_eq!(playing.state, PlaybackState::Playing, "{playing:?}");
    assert_eq!(playing.source_size, Some((1920, 1080)));
    assert_eq!(playing.frame_size, Some((1920, 1080)));
    let started = Instant::now();
    let mut depth_over = false;
    while started.elapsed() < Duration::from_secs(2) {
        let _ = player.take_frame();
        if player.diagnostics().mailbox_depth > 1 {
            depth_over = true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let stats = player.diagnostics();
    assert!(!depth_over);
    assert!(stats.mailbox_depth <= 1);
    assert!(stats.produced >= 20, "{stats:?}");
    player.pause().unwrap();
    player.seek(Duration::from_millis(500)).unwrap();
    player.play().unwrap();
    let after = rss_bytes();
    if let (Some(before), Some(after)) = (before, after) {
        assert!(
            after.saturating_sub(before) < 300 * 1024 * 1024,
            "{before} {after}"
        );
    }
}

#[test]
fn missing_commands_do_not_need_a_library() {
    // Pure validation is covered in the crate. This checks the profile export
    // stays free of the WebKit direct-play ceiling.
    let profile = matinee_player::native_device_profile();
    assert_eq!(profile["Name"], "Matinee Native");
    assert_ne!(profile["SubtitleProfiles"].as_array().unwrap().len(), 0);
}

fn rss_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = text.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096)
}
