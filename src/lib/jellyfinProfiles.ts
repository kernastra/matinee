import { invoke } from '@tauri-apps/api/core';
import type { JellyfinSession } from './jellyfin';

export type JellyfinProfile = {
  id: string;
  serverUrl: string;
  userId: string;
  userName: string;
  primaryImageTag?: string;
  lastUsedAt: number;
};

export function listJellyfinProfiles() {
  return invoke<JellyfinProfile[]>('list_jellyfin_profiles');
}

export function rememberJellyfinProfile(session: JellyfinSession) {
  return invoke<JellyfinProfile>('remember_jellyfin_profile', {
    profile: {
      serverUrl: session.serverUrl,
      accessToken: session.accessToken,
      userId: session.user.Id,
      userName: session.user.Name,
      primaryImageTag: session.user.PrimaryImageTag,
    },
  });
}

export function restoreJellyfinProfile(profileId: string) {
  return invoke<JellyfinSession>('restore_jellyfin_profile', { profileId });
}

export function forgetJellyfinProfile(profileId: string) {
  return invoke<void>('forget_jellyfin_profile', { profileId });
}
