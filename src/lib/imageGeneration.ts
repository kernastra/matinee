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
  versionId: string;
  localPath: string;
  dataUrl: string;
  createdAt: number;
};

export type ArtworkRecord = {
  versionId: string;
  itemId: string;
  title: string;
  itemType: string;
  assetType: ArtworkAssetType;
  localPath: string;
  thumbnailDataUrl: string;
  createdAt: number;
  active: boolean;
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

export function generatePosterImage(provider: ImageProvider, prompt: string, jellyfinServerUrl: string, referenceUrls: string[] = [], assetType: ArtworkAssetType = 'Poster') {
  return invoke<GeneratedImage>('generate_poster_image', { provider, prompt, jellyfinServerUrl, referenceUrls, assetType });
}

export function exportGeneratedImage(localPath: string, title: string) {
  return invoke<string>('export_generated_image', { localPath, title });
}

export function listCustomPosters() {
  return invoke<CustomPoster[]>('list_custom_posters');
}

export function assignGeneratedPoster(itemId: string, title: string, itemType: string, localPath: string) {
  return invoke<CustomPoster>('assign_generated_poster', { itemId, title, itemType, localPath });
}

export function storeGeneratedArtwork(itemId: string, title: string, itemType: string, assetType: ArtworkAssetType, localPath: string) {
  return invoke<ArtworkRecord>('store_generated_artwork', { itemId, title, itemType, assetType, localPath });
}

export function listArtworkLibrary() {
  return invoke<ArtworkRecord[]>('list_artwork_library');
}

export function activateArtworkVersion(itemId: string, versionId: string) {
  return invoke<void>('activate_artwork_version', { itemId, versionId });
}

export function deleteArtworkVersion(itemId: string, versionId: string) {
  return invoke<void>('delete_artwork_version', { itemId, versionId });
}

export function exportArtworkToMediaFolder(localPath: string, mediaPath: string, itemType: string, assetType: ArtworkAssetType, overwrite = false) {
  return invoke<string>('export_artwork_to_media_folder', { localPath, mediaPath, itemType, assetType, overwrite });
}
