import { useEffect, useRef, useState } from 'react';
import type Hls from 'hls.js';
import MaterialIcon from './MaterialIcon';
import {
  getPlaybackPlan,
  reportPlayback,
  type JellyfinItem,
  type PlaybackMethod,
  type PlaybackPlan,
  type JellyfinSession,
} from '../lib/jellyfin';

type PlayerProps = {
  item: JellyfinItem;
  session: JellyfinSession;
  onBack: () => void;
  onFinished?: (item: JellyfinItem) => void;
};

type PlaybackQuality = 'auto' | 'original' | '1080p' | '720p';

const qualityBitrates: Record<PlaybackQuality, number | undefined> = {
  auto: undefined,
  original: 120_000_000,
  '1080p': 8_000_000,
  '720p': 4_000_000,
};

function formatTime(seconds: number) {
  if (!Number.isFinite(seconds) || seconds < 0) return '0:00';
  const wholeSeconds = Math.floor(seconds);
  const hours = Math.floor(wholeSeconds / 3600);
  const minutes = Math.floor((wholeSeconds % 3600) / 60);
  const remainder = wholeSeconds % 60;
  return hours
    ? `${hours}:${minutes.toString().padStart(2, '0')}:${remainder.toString().padStart(2, '0')}`
    : `${minutes}:${remainder.toString().padStart(2, '0')}`;
}

export default function Player({ item, session, onBack, onFinished }: PlayerProps) {
  const playerRef = useRef<HTMLElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const playSessionId = useRef<string>(crypto.randomUUID());
  const playMethod = useRef<PlaybackMethod>('DirectPlay');
  const lastReportedSecond = useRef(0);
  const stopped = useRef(false);
  const hlsRecoveryAttempts = useRef(0);
  const controlsTimer = useRef<number | null>(null);
  const controlsHovered = useRef(false);
  const restartTimeTicks = useRef<number | undefined>(undefined);
  const seekOnLoadSeconds = useRef<number | undefined>(undefined);
  const [plan, setPlan] = useState<PlaybackPlan | null>(null);
  const [status, setStatus] = useState('Preparing your video…');
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [controlsVisible, setControlsVisible] = useState(true);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [buffered, setBuffered] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [quality, setQuality] = useState<PlaybackQuality>('auto');
  const [audioStreamIndex, setAudioStreamIndex] = useState<number | undefined>(undefined);
  const [subtitleStreamIndex, setSubtitleStreamIndex] = useState<number | undefined>(undefined);

  useEffect(() => {
    const controller = new AbortController();
    stopped.current = false;
    lastReportedSecond.current = 0;
    setPlan(null);
    setError('');
    setStatus('Checking the best playback format…');

    getPlaybackPlan(session, item, {
      maxStreamingBitrate: qualityBitrates[quality],
      audioStreamIndex,
      subtitleStreamIndex,
      startTimeTicks: restartTimeTicks.current,
    }, controller.signal)
      .then((nextPlan) => {
        if (controller.signal.aborted) return;
        playSessionId.current = nextPlan.playSessionId;
        playMethod.current = nextPlan.method;
        restartTimeTicks.current = undefined;
        setStatus(nextPlan.method === 'DirectPlay' ? 'Opening video…' : 'Preparing a compatible stream…');
        setPlan(nextPlan);
      })
      .catch((reason: unknown) => {
        if (controller.signal.aborted) return;
        setError(reason instanceof Error ? reason.message : 'Matinee could not prepare this video.');
      });

    return () => controller.abort();
  }, [attempt, audioStreamIndex, item, quality, session, subtitleStreamIndex]);

  function report(event: 'start' | 'progress' | 'stopped') {
    const video = videoRef.current;
    if (!video) return;
    void reportPlayback(
      session,
      item,
      event,
      playSessionId.current,
      video.currentTime,
      video.paused,
      playMethod.current,
    ).catch((error: unknown) => console.warn('[playback] progress report failed', error));
  }

  function stop() {
    const video = videoRef.current;
    if (!video || video.readyState === 0) return;
    if (stopped.current) return;
    stopped.current = true;
    report('stopped');
  }

  useEffect(() => () => stop(), []);

  function revealControls() {
    setControlsVisible(true);
    if (controlsTimer.current) window.clearTimeout(controlsTimer.current);
    if (isPlaying && !controlsHovered.current) {
      controlsTimer.current = window.setTimeout(() => setControlsVisible(false), 5_000);
    }
  }

  function keepControlsVisible() {
    controlsHovered.current = true;
    setControlsVisible(true);
    if (controlsTimer.current) window.clearTimeout(controlsTimer.current);
  }

  function releaseControls() {
    controlsHovered.current = false;
    revealControls();
  }

  useEffect(() => {
    revealControls();
    return () => {
      if (controlsTimer.current) window.clearTimeout(controlsTimer.current);
    };
  }, [isPlaying]);

  useEffect(() => {
    const fullscreenChanged = () => setIsFullscreen(document.fullscreenElement === playerRef.current);
    document.addEventListener('fullscreenchange', fullscreenChanged);
    return () => document.removeEventListener('fullscreenchange', fullscreenChanged);
  }, []);

  useEffect(() => {
    const video = videoRef.current;
    if (!video || !plan || !new URL(plan.url).pathname.toLowerCase().endsWith('.m3u8')) return;

    let cancelled = false;
    let hls: Hls | null = null;
    hlsRecoveryAttempts.current = 0;
    void import('hls.js')
      .then(({ default: HlsPlayer }) => {
        if (cancelled) return;
        if (HlsPlayer.isSupported()) {
          hls = new HlsPlayer({
            enableWorker: true,
            startFragPrefetch: true,
          });
          hls.on(HlsPlayer.Events.MANIFEST_LOADING, () => setStatus('Loading stream…'));
          hls.on(HlsPlayer.Events.MANIFEST_PARSED, () => {
            setStatus('Starting video…');
            void video.play().catch(() => {
              setStatus('Press play to begin');
            });
          });
          hls.on(HlsPlayer.Events.ERROR, (_event, data) => {
            if (!data.fatal || !hls) return;
            if (hlsRecoveryAttempts.current < 1 && data.type === HlsPlayer.ErrorTypes.NETWORK_ERROR) {
              hlsRecoveryAttempts.current += 1;
              setStatus('Reconnecting to Jellyfin…');
              hls.startLoad();
              return;
            }
            if (hlsRecoveryAttempts.current < 1 && data.type === HlsPlayer.ErrorTypes.MEDIA_ERROR) {
              hlsRecoveryAttempts.current += 1;
              setStatus('Recovering video playback…');
              hls.recoverMediaError();
              return;
            }
            setError(
              data.type === HlsPlayer.ErrorTypes.NETWORK_ERROR
                ? 'The HLS stream could not be loaded from Jellyfin.'
                : 'The HLS video stream could not be decoded.',
            );
          });
          hls.loadSource(plan.url);
          hls.attachMedia(video);
          return;
        }

        if (video.canPlayType('application/vnd.apple.mpegurl')) {
          video.src = plan.url;
          video.load();
          return;
        }

        setError('This system does not provide an HLS playback engine.');
      })
      .catch(() => {
        if (!cancelled) setError('Matinee could not load its HLS playback engine.');
      });

    return () => {
      cancelled = true;
      hls?.destroy();
    };
  }, [plan]);

  function loaded() {
    const video = videoRef.current;
    if (!video) return;
    const resumeAt = seekOnLoadSeconds.current
      ?? (item.UserData?.PlaybackPositionTicks ?? 0) / 10_000_000;
    if (resumeAt > 0 && resumeAt < video.duration - 30) video.currentTime = resumeAt;
    seekOnLoadSeconds.current = undefined;
    setDuration(video.duration);
    setCurrentTime(video.currentTime);
    report('start');
  }

  function playbackFailed() {
    const code = videoRef.current?.error?.code;
    const messages: Record<number, string> = {
      1: 'Video loading was cancelled.',
      2: 'The connection to Jellyfin was interrupted.',
      3: 'This video could not be decoded by the player.',
      4: 'Jellyfin did not return a supported video format.',
    };
    setError((code && messages[code]) || 'The video could not be played.');
  }

  function timeUpdated() {
    const video = videoRef.current;
    const second = Math.floor(video?.currentTime ?? 0);
    setCurrentTime(video?.currentTime ?? 0);
    if (second - lastReportedSecond.current < 10) return;
    lastReportedSecond.current = second;
    report('progress');
  }

  function updateBuffered() {
    const video = videoRef.current;
    if (!video?.buffered.length || !video.duration) return;
    setBuffered(video.buffered.end(video.buffered.length - 1));
  }

  function togglePlayback() {
    const video = videoRef.current;
    if (!video) return;
    revealControls();
    if (video.paused) {
      void video.play();
    } else {
      video.pause();
    }
  }

  function seekBy(seconds: number) {
    const video = videoRef.current;
    if (!video) return;
    video.currentTime = Math.min(Math.max(video.currentTime + seconds, 0), video.duration || 0);
    setCurrentTime(video.currentTime);
    revealControls();
  }

  function seekTo(seconds: number) {
    const video = videoRef.current;
    if (!video) return;
    video.currentTime = seconds;
    setCurrentTime(seconds);
  }

  function changeVolume(nextVolume: number) {
    const video = videoRef.current;
    if (!video) return;
    video.volume = nextVolume;
    video.muted = false;
    setVolume(nextVolume);
    setMuted(false);
  }

  function toggleMute() {
    const video = videoRef.current;
    if (!video) return;
    video.muted = !video.muted;
    setMuted(video.muted);
  }

  function renegotiatePlayback(change: () => void) {
    const video = videoRef.current;
    if (video) {
      restartTimeTicks.current = Math.round(video.currentTime * 10_000_000);
      seekOnLoadSeconds.current = video.currentTime;
    }
    stop();
    change();
    setAttempt((current) => current + 1);
  }

  async function toggleFullscreen() {
    if (document.fullscreenElement) {
      await document.exitFullscreen();
    } else {
      await playerRef.current?.requestFullscreen();
    }
  }

  useEffect(() => {
    const keyPressed = (event: KeyboardEvent) => {
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLButtonElement) return;
      if (event.code === 'Space') {
        event.preventDefault();
        togglePlayback();
      } else if (event.code === 'ArrowLeft') {
        seekBy(-10);
      } else if (event.code === 'ArrowRight') {
        seekBy(10);
      } else if (event.key.toLowerCase() === 'm') {
        toggleMute();
      } else if (event.key.toLowerCase() === 'f') {
        void toggleFullscreen();
      } else {
        return;
      }
      revealControls();
    };
    window.addEventListener('keydown', keyPressed);
    return () => window.removeEventListener('keydown', keyPressed);
  });

  function leave() {
    stop();
    onBack();
  }

  function retry() {
    stop();
    setAttempt((current) => current + 1);
  }

  function playbackEnded() {
    stop();
    onFinished?.(item);
  }

  const audioStreams = plan?.mediaStreams.filter((stream) => stream.Type === 'Audio') ?? [];
  const subtitleStreams = plan?.mediaStreams.filter((stream) => stream.Type === 'Subtitle') ?? [];
  const selectedAudio = audioStreamIndex ?? plan?.audioStreamIndex ?? audioStreams[0]?.Index ?? '';
  const selectedSubtitle = subtitleStreamIndex ?? plan?.subtitleStreamIndex ?? -1;

  return (
    <main
      ref={playerRef}
      className={`player-shell${controlsVisible ? ' player-shell--controls-visible' : ''}`}
      onMouseMove={revealControls}
      onMouseLeave={releaseControls}
    >
      {plan ? (
        <video
          ref={videoRef}
          className="video-player"
          src={new URL(plan.url).pathname.toLowerCase().endsWith('.m3u8') ? undefined : plan.url}
          autoPlay
          onClick={togglePlayback}
          onLoadStart={() => setStatus('Loading video…')}
          onLoadedMetadata={loaded}
          onCanPlay={() => setStatus('')}
          onPlaying={() => {
            setStatus('');
            setIsPlaying(true);
            report('progress');
          }}
          onWaiting={() => setStatus('Buffering…')}
          onStalled={() => setStatus('Waiting for Jellyfin…')}
          onTimeUpdate={timeUpdated}
          onProgress={updateBuffered}
          onDurationChange={() => setDuration(videoRef.current?.duration ?? 0)}
          onVolumeChange={() => {
            const video = videoRef.current;
            if (!video) return;
            setVolume(video.volume);
            setMuted(video.muted);
          }}
          onPause={() => {
            setIsPlaying(false);
            report('progress');
          }}
          onError={new URL(plan.url).pathname.toLowerCase().endsWith('.m3u8') ? undefined : playbackFailed}
          onEnded={playbackEnded}
        />
      ) : null}
      {error ? (
        <div className="player-message player-message--error" role="alert">
          <strong>Playback couldn’t start</strong>
          <p>{error}</p>
          <div>
            <button className="primary-button" type="button" onClick={retry}>Try again</button>
            <button className="secondary-button" type="button" onClick={leave}>Back to details</button>
          </div>
        </div>
      ) : status ? (
        <div className="player-message" role="status" aria-live="polite">
          <span className="player-spinner" aria-hidden="true" />
          <strong>{status}</strong>
          <p>{item.Name}</p>
        </div>
      ) : null}
      <div className="player-topbar">
        <button className="player-back" type="button" onClick={leave}>
          <MaterialIcon name="arrow_back" /> Back
        </button>
        <div>
          <span>Now playing{plan ? ` · ${plan.method.replace(/([a-z])([A-Z])/g, '$1 $2')}` : ''}</span>
          <strong>{item.Name}</strong>
        </div>
      </div>
      {plan && !error ? (
        <div className="player-controls" onMouseEnter={keepControlsVisible} onMouseLeave={releaseControls}>
          <div className="player-timeline">
            <span className="player-timeline__track" aria-hidden="true">
              <span className="player-timeline__buffered" style={{ width: `${duration ? Math.min((buffered / duration) * 100, 100) : 0}%` }} />
              <span className="player-timeline__played" style={{ width: `${duration ? (currentTime / duration) * 100 : 0}%` }} />
            </span>
            <input
              type="range"
              min="0"
              max={duration || 0}
              step="0.1"
              value={Math.min(currentTime, duration || 0)}
              onChange={(event) => seekTo(Number(event.target.value))}
              aria-label="Playback position"
            />
          </div>
          <div className="player-control-row">
            <div className="player-control-group">
              <button type="button" onClick={togglePlayback} aria-label={isPlaying ? 'Pause' : 'Play'}>
                <MaterialIcon name={isPlaying ? 'pause' : 'play_arrow'} filled />
              </button>
              <button type="button" onClick={() => seekBy(-10)} aria-label="Rewind 10 seconds"><MaterialIcon name="fast_rewind" /></button>
              <button type="button" onClick={() => seekBy(10)} aria-label="Forward 10 seconds"><MaterialIcon name="fast_forward" /></button>
              <button type="button" onClick={toggleMute} aria-label={muted ? 'Unmute' : 'Mute'}>
                <MaterialIcon name={muted || volume === 0 ? 'volume_off' : 'volume_up'} />
              </button>
              <input
                className="player-volume"
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={muted ? 0 : volume}
                onChange={(event) => changeVolume(Number(event.target.value))}
                aria-label="Volume"
              />
            </div>
            <span className="player-time">{formatTime(currentTime)} / {formatTime(duration)}</span>
            <div className="player-stream-controls">
              {audioStreams.length > 1 ? (
                <label title="Audio track">
                  <MaterialIcon name="audio_track" />
                  <select
                    value={selectedAudio}
                    aria-label="Audio track"
                    onChange={(event) => renegotiatePlayback(() => setAudioStreamIndex(Number(event.target.value)))}
                  >
                    {audioStreams.map((stream) => (
                      <option value={stream.Index} key={stream.Index}>
                        {stream.DisplayTitle || stream.Title || stream.Language || `Audio ${stream.Index + 1}`}
                      </option>
                    ))}
                  </select>
                </label>
              ) : null}
              {subtitleStreams.length ? (
                <label title="Subtitles">
                  <MaterialIcon name="subtitles" />
                  <select
                    value={selectedSubtitle}
                    aria-label="Subtitles"
                    onChange={(event) => renegotiatePlayback(() => setSubtitleStreamIndex(Number(event.target.value)))}
                  >
                    <option value={-1}>Off</option>
                    {subtitleStreams.map((stream) => (
                      <option value={stream.Index} key={stream.Index}>
                        {stream.DisplayTitle || stream.Title || stream.Language || `Subtitle ${stream.Index + 1}`}
                      </option>
                    ))}
                  </select>
                </label>
              ) : null}
              <label title="Playback quality">
                <MaterialIcon name="tune" />
                <select
                  value={quality}
                  aria-label="Playback quality"
                  onChange={(event) => renegotiatePlayback(() => setQuality(event.target.value as PlaybackQuality))}
                >
                  <option value="auto">Auto</option>
                  <option value="original">Original</option>
                  <option value="1080p">1080p</option>
                  <option value="720p">720p</option>
                </select>
              </label>
            </div>
            <button
              className="player-fullscreen"
              type="button"
              onClick={() => void toggleFullscreen()}
              aria-label={isFullscreen ? 'Exit fullscreen' : 'Enter fullscreen'}
            >
              <MaterialIcon name={isFullscreen ? 'fullscreen_exit' : 'fullscreen'} />
            </button>
          </div>
        </div>
      ) : null}
    </main>
  );
}
