const SETTINGS_KEY = 'matinee.settings.v1';

export type PlaybackQuality = 'auto' | 'original' | '1080p' | '720p';
export type SubtitleMode = 'jellyfin' | 'off';
export type AudioLanguage = 'default' | 'eng' | 'spa' | 'jpn';

export type AppSettings = {
  playbackQuality: PlaybackQuality;
  subtitleMode: SubtitleMode;
  audioLanguage: AudioLanguage;
  autoplayNextEpisode: boolean;
  heroRotation: boolean;
  reducedMotion: boolean;
};

export const defaultSettings: AppSettings = {
  playbackQuality: 'auto',
  subtitleMode: 'jellyfin',
  audioLanguage: 'default',
  autoplayNextEpisode: true,
  heroRotation: true,
  reducedMotion: false,
};

const qualityValues = new Set<PlaybackQuality>(['auto', 'original', '1080p', '720p']);
const subtitleValues = new Set<SubtitleMode>(['jellyfin', 'off']);
const languageValues = new Set<AudioLanguage>(['default', 'eng', 'spa', 'jpn']);

function isBoolean(value: unknown): value is boolean {
  return typeof value === 'boolean';
}

export function loadSettings(): AppSettings {
  const value = localStorage.getItem(SETTINGS_KEY);
  if (!value) return defaultSettings;
  try {
    const stored = JSON.parse(value) as Partial<AppSettings>;
    return {
      playbackQuality: qualityValues.has(stored.playbackQuality as PlaybackQuality)
        ? stored.playbackQuality as PlaybackQuality
        : defaultSettings.playbackQuality,
      subtitleMode: subtitleValues.has(stored.subtitleMode as SubtitleMode)
        ? stored.subtitleMode as SubtitleMode
        : defaultSettings.subtitleMode,
      audioLanguage: languageValues.has(stored.audioLanguage as AudioLanguage)
        ? stored.audioLanguage as AudioLanguage
        : defaultSettings.audioLanguage,
      autoplayNextEpisode: isBoolean(stored.autoplayNextEpisode)
        ? stored.autoplayNextEpisode
        : defaultSettings.autoplayNextEpisode,
      heroRotation: isBoolean(stored.heroRotation) ? stored.heroRotation : defaultSettings.heroRotation,
      reducedMotion: isBoolean(stored.reducedMotion) ? stored.reducedMotion : defaultSettings.reducedMotion,
    };
  } catch {
    localStorage.removeItem(SETTINGS_KEY);
    return defaultSettings;
  }
}

export function saveSettings(settings: AppSettings) {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
}

export function resetSettings() {
  localStorage.removeItem(SETTINGS_KEY);
  return defaultSettings;
}
