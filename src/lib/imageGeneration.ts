import { invoke } from '@tauri-apps/api/core';
import type { ImageProvider } from './settings';
import type { ArtworkAssetType } from './posterPrompts';

export type LocalImageProvider = Extract<ImageProvider, 'codex' | 'higgsfield'>;
export type KeyImageProvider = Extract<ImageProvider, 'fal' | 'higgsfield'>;

export type LocalProviderStatus = {
  provider: LocalImageProvider;
  found: boolean;
  authenticated: boolean;
  path?: string;
  detail: string;
};

export type ProviderKeyStatus = {
  provider: KeyImageProvider;
  configured: boolean;
};

export type GeneratedImage = {
  provider: ImageProvider;
  localPath: string;
  dataUrl: string;
};

export type CustomPoster = {
  itemId: string;
  localPath: string;
  dataUrl: string;
};

export function scanLocalImageProvider(provider: LocalImageProvider) {
  return invoke<LocalProviderStatus>('scan_local_image_provider', { provider });
}

export function getProviderKeyStatus(provider: KeyImageProvider) {
  return invoke<ProviderKeyStatus>('provider_key_status', { provider });
}

export function saveProviderKey(provider: KeyImageProvider, key: string) {
  return invoke<ProviderKeyStatus>('save_provider_key', { provider, key });
}

export function removeProviderKey(provider: KeyImageProvider) {
  return invoke<ProviderKeyStatus>('remove_provider_key', { provider });
}

export function generatePosterImage(provider: ImageProvider, prompt: string, referenceUrls: string[] = [], assetType: ArtworkAssetType = 'Poster') {
  return invoke<GeneratedImage>('generate_poster_image', { provider, prompt, referenceUrls, assetType });
}

export function exportGeneratedImage(localPath: string, title: string) {
  return invoke<string>('export_generated_image', { localPath, title });
}

export function listCustomPosters() {
  return invoke<CustomPoster[]>('list_custom_posters');
}

export function assignGeneratedPoster(itemId: string, localPath: string) {
  return invoke<CustomPoster>('assign_generated_poster', { itemId, localPath });
}

export function exportPosterToMediaFolder(localPath: string, mediaPath: string, overwrite = false) {
  return invoke<string>('export_poster_to_media_folder', { localPath, mediaPath, overwrite });
}
