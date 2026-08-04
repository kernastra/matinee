import { beforeEach, describe, expect, it } from 'vitest';
import { defaultSettings, loadSettings, resetSettings, saveSettings } from './settings';

beforeEach(() => localStorage.clear());

describe('application settings', () => {
  it('uses Matinee defaults when no preferences have been saved', () => {
    expect(loadSettings()).toEqual(defaultSettings);
  });

  it('round-trips playback and interface preferences', () => {
    const settings = {
      ...defaultSettings,
      playbackQuality: '1080p' as const,
      audioLanguage: 'eng' as const,
      subtitleMode: 'off' as const,
      autoplayNextEpisode: false,
      heroRotation: false,
      reducedMotion: true,
      imageProvider: 'fal' as const,
      posterMetadata: 'never' as const,
    };
    saveSettings(settings);
    expect(loadSettings()).toEqual(settings);
  });

  it('preserves defaults for missing or unsupported stored values', () => {
    localStorage.setItem('matinee.settings.v1', JSON.stringify({
      playbackQuality: '4k',
      heroRotation: false,
      imageProvider: 'unknown',
      posterMetadata: 'somewhere',
    }));
    expect(loadSettings()).toEqual({ ...defaultSettings, heroRotation: false });
  });

  it('clears malformed settings and restores defaults', () => {
    localStorage.setItem('matinee.settings.v1', '{broken');
    expect(loadSettings()).toEqual(defaultSettings);
    expect(localStorage.getItem('matinee.settings.v1')).toBeNull();

    saveSettings({ ...defaultSettings, reducedMotion: true });
    expect(resetSettings()).toEqual(defaultSettings);
    expect(localStorage.getItem('matinee.settings.v1')).toBeNull();
  });
});
