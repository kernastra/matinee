import { invoke } from '@tauri-apps/api/core';

export type ArtworkStorageSettings = {
  libraryRoot: string;
  exportRoot: string;
};

export function getArtworkStorageSettings() {
  return invoke<ArtworkStorageSettings>('get_artwork_storage_settings');
}

export function saveArtworkStorageSettings(settings: ArtworkStorageSettings) {
  return invoke<ArtworkStorageSettings>('save_artwork_storage_settings', { settings });
}
