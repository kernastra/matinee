import { invoke } from '@tauri-apps/api/core';

export type MediaPathMapping = {
  jellyfinPrefix: string;
  localRoot: string;
};

export type MediaPathSettings = {
  mappings: MediaPathMapping[];
  trustedRoots: string[];
};

export function getMediaPathSettings() {
  return invoke<MediaPathSettings>('get_media_path_settings');
}

export function saveMediaPathSettings(settings: MediaPathSettings) {
  return invoke<MediaPathSettings>('save_media_path_settings', { settings });
}
