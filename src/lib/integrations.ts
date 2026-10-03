import { invoke } from '@tauri-apps/api/core';
import type { AppSettings } from './settings';

export type MediaIntegration = 'radarr' | 'sonarr';

export type IntegrationKeyStatus = {
  provider: MediaIntegration;
  configured: boolean;
};

export type IntegrationConnection = IntegrationKeyStatus & {
  serverUrl: string;
  version?: string;
};

export type UpcomingRelease = {
  id: string;
  source: MediaIntegration;
  sourceId: number;
  seriesId?: number;
  title: string;
  subtitle?: string;
  overview?: string;
  date: string;
  releaseKind: 'Theatrical' | 'Digital' | 'Physical' | 'Episode';
  imageUrl?: string;
  genres: string[];
  monitored: boolean;
  downloaded: boolean;
  seasonNumber?: number;
  episodeNumber?: number;
  milestones?: ReleaseMilestone[];
};

export type ReleaseMilestone = {
  date: string;
  kind: 'Theatrical' | 'Digital' | 'Physical';
};

export type UpcomingResult = {
  events: UpcomingRelease[];
  errors: Partial<Record<MediaIntegration, string>>;
  home: UpcomingRelease[];
};

export function getIntegrationKeyStatus(provider: MediaIntegration) {
  return invoke<IntegrationKeyStatus>('integration_key_status', { provider });
}

export function testAndSaveIntegration(
  provider: MediaIntegration,
  serverUrl: string,
  apiKey?: string,
) {
  return invoke<IntegrationConnection>('test_and_save_integration', {
    provider,
    serverUrl,
    apiKey: apiKey?.trim() || null,
  });
}

export function removeIntegration(provider: MediaIntegration) {
  return invoke<IntegrationKeyStatus>('remove_integration_key', { provider });
}

export function integrationEnabled(settings: AppSettings) {
  return Boolean(settings.radarrUrl || settings.sonarrUrl);
}

export function fetchUpcomingReleases(
  settings: Pick<AppSettings, 'radarrUrl' | 'sonarrUrl'>,
  start: string,
  end: string,
): Promise<UpcomingResult> {
  return invoke<UpcomingResult>('fetch_upcoming_releases', {
    radarrUrl: settings.radarrUrl,
    sonarrUrl: settings.sonarrUrl,
    start,
    end,
  });
}

export function clearUpcomingCache() {
  return invoke<void>('clear_integration_calendar_cache');
}

export function upcomingWindow(from = new Date(), days = 120) {
  const start = new Date(from);
  start.setHours(0, 0, 0, 0);
  const end = new Date(start);
  end.setDate(end.getDate() + days);
  return { start: start.toISOString(), end: end.toISOString() };
}

export function homeUpcoming(events: UpcomingRelease[], limit = 12) {
  const preferredMovieEvents = new Map<number, UpcomingRelease>();
  for (const event of events) {
    if (event.source !== 'radarr' || event.downloaded) continue;
    const current = preferredMovieEvents.get(event.sourceId);
    const isHomeRelease = event.releaseKind === 'Digital' || event.releaseKind === 'Physical';
    const currentIsHomeRelease = current?.releaseKind === 'Digital' || current?.releaseKind === 'Physical';
    if (!current || (isHomeRelease && !currentIsHomeRelease)) preferredMovieEvents.set(event.sourceId, event);
  }

  const seenSeries = new Set<number>();
  const seenMovies = new Set<number>();
  const selected: UpcomingRelease[] = [];
  for (const event of events) {
    if (event.downloaded) continue;
    if (event.source === 'radarr') {
      if (seenMovies.has(event.sourceId) || preferredMovieEvents.get(event.sourceId)?.id !== event.id) continue;
      seenMovies.add(event.sourceId);
    }
    if (event.source === 'sonarr' && event.seriesId !== undefined) {
      if (seenSeries.has(event.seriesId)) continue;
      seenSeries.add(event.seriesId);
    }
    selected.push(event);
  }
  return selected
    .sort((left, right) => Date.parse(left.date) - Date.parse(right.date))
    .slice(0, limit);
}

export function releaseDateLabel(value: string, now = new Date()) {
  const date = new Date(value);
  const today = new Date(now);
  date.setHours(0, 0, 0, 0);
  today.setHours(0, 0, 0, 0);
  const days = Math.round((date.getTime() - today.getTime()) / 86_400_000);
  if (days === 0) return 'Today';
  if (days === 1) return 'Tomorrow';
  if (days > 1 && days < 14) return `In ${days} days`;
  return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(date);
}
