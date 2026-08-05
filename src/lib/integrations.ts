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
};

const upcomingCache = new Map<string, { expires: number; request: Promise<UpcomingResult> }>();

type RawImage = {
  coverType?: string;
  remoteUrl?: string;
  url?: string;
};

type RadarrMovie = {
  id?: number;
  title?: string;
  overview?: string;
  monitored?: boolean;
  hasFile?: boolean;
  inCinemas?: string;
  digitalRelease?: string;
  physicalRelease?: string;
  images?: RawImage[];
  genres?: string[];
};

type SonarrSeries = {
  id?: number;
  title?: string;
  overview?: string;
  monitored?: boolean;
  images?: RawImage[];
  genres?: string[];
};

type SonarrEpisode = {
  id?: number;
  title?: string;
  airDateUtc?: string;
  airDate?: string;
  monitored?: boolean;
  hasFile?: boolean;
  seasonNumber?: number;
  episodeNumber?: number;
  seriesId?: number;
  series?: SonarrSeries;
  images?: RawImage[];
};

function errorMessage(error: unknown) {
  return typeof error === 'string'
    ? error
    : error instanceof Error
      ? error.message
      : 'The integration request failed.';
}

function imageUrl(images: RawImage[] | undefined, preferred: string[]) {
  for (const coverType of preferred) {
    const image = images?.find((candidate) => candidate.coverType?.toLowerCase() === coverType);
    const url = image?.remoteUrl || image?.url;
    if (url?.startsWith('http://') || url?.startsWith('https://')) return url;
  }
  return undefined;
}

function validDate(value: string | undefined) {
  if (!value) return undefined;
  const milliseconds = Date.parse(value);
  return Number.isFinite(milliseconds) ? { value, milliseconds } : undefined;
}

function isInWindow(milliseconds: number, start: string, end: string) {
  return milliseconds >= Date.parse(start) && milliseconds < Date.parse(end);
}

export function normalizeCalendar(
  provider: MediaIntegration,
  response: unknown,
  start: string,
  end: string,
): UpcomingRelease[] {
  if (!Array.isArray(response)) return [];

  if (provider === 'radarr') {
    return (response as RadarrMovie[]).flatMap((movie) => {
      if (!movie.id || !movie.title || movie.monitored === false) return [];
      const movieId = movie.id;
      const movieTitle = movie.title;
      const milestones = [
        { date: validDate(movie.inCinemas), kind: 'Theatrical' as const },
        { date: validDate(movie.digitalRelease), kind: 'Digital' as const },
        { date: validDate(movie.physicalRelease), kind: 'Physical' as const },
      ]
        .filter((candidate): candidate is { date: { value: string; milliseconds: number }; kind: ReleaseMilestone['kind'] } => Boolean(candidate.date))
        .sort((left, right) => left.date.milliseconds - right.date.milliseconds);
      const releaseMilestones = milestones.map(({ date, kind }) => ({ date: date.value, kind }));
      return milestones
        .filter(({ date }) => isInWindow(date.milliseconds, start, end))
        .map((release) => ({
          id: `radarr-${movieId}-${release.kind.toLowerCase()}`,
          source: 'radarr' as const,
          sourceId: movieId,
          title: movieTitle,
          overview: movie.overview,
          date: release.date.value,
          releaseKind: release.kind,
          imageUrl: imageUrl(movie.images, ['poster', 'fanart']),
          genres: movie.genres ?? [],
          monitored: true,
          downloaded: Boolean(movie.hasFile),
          milestones: releaseMilestones,
        }));
    });
  }

  return (response as SonarrEpisode[]).flatMap((episode) => {
    const date = validDate(episode.airDateUtc || episode.airDate);
    const series = episode.series;
    if (!episode.id || !date || !isInWindow(date.milliseconds, start, end)) return [];
    if (episode.monitored === false || series?.monitored === false) return [];
    const seriesTitle = series?.title || 'Upcoming series';
    const season = episode.seasonNumber;
    const number = episode.episodeNumber;
    const episodeCode = season !== undefined && number !== undefined
      ? `S${String(season).padStart(2, '0')}E${String(number).padStart(2, '0')}`
      : undefined;
    return [{
      id: `sonarr-${episode.id}`,
      source: 'sonarr' as const,
      sourceId: episode.id,
      seriesId: episode.seriesId || series?.id,
      title: seriesTitle,
      subtitle: [episodeCode, episode.title].filter(Boolean).join(' · '),
      overview: series?.overview,
      date: date.value,
      releaseKind: 'Episode' as const,
      imageUrl: imageUrl(episode.images, ['screenshot']) || imageUrl(series?.images, ['poster', 'fanart']),
      genres: series?.genres ?? [],
      monitored: true,
      downloaded: Boolean(episode.hasFile),
      seasonNumber: season,
      episodeNumber: number,
    }];
  });
}

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

function fetchProviderCalendar(provider: MediaIntegration, start: string, end: string) {
  return invoke<unknown>('fetch_integration_calendar', { provider, start, end });
}

export function integrationEnabled(settings: AppSettings) {
  return Boolean(settings.radarrUrl || settings.sonarrUrl);
}

async function requestUpcomingReleases(
  settings: Pick<AppSettings, 'radarrUrl' | 'sonarrUrl'>,
  start: string,
  end: string,
): Promise<UpcomingResult> {
  const configured: Array<{ provider: MediaIntegration; url: string }> = [];
  if (settings.radarrUrl) configured.push({ provider: 'radarr', url: settings.radarrUrl });
  if (settings.sonarrUrl) configured.push({ provider: 'sonarr', url: settings.sonarrUrl });
  const settled = await Promise.allSettled(
    configured.map(({ provider }) => fetchProviderCalendar(provider, start, end)),
  );
  const errors: UpcomingResult['errors'] = {};
  const events: UpcomingRelease[] = [];
  settled.forEach((result, index) => {
    const source = configured[index];
    if (result.status === 'fulfilled') {
      events.push(...normalizeCalendar(source.provider, result.value, start, end));
    } else {
      errors[source.provider] = errorMessage(result.reason);
    }
  });
  events.sort((left, right) => Date.parse(left.date) - Date.parse(right.date));
  return { events, errors };
}

export function fetchUpcomingReleases(
  settings: Pick<AppSettings, 'radarrUrl' | 'sonarrUrl'>,
  start: string,
  end: string,
): Promise<UpcomingResult> {
  const key = JSON.stringify([settings.radarrUrl, settings.sonarrUrl, start, end]);
  const cached = upcomingCache.get(key);
  if (cached && cached.expires > Date.now()) return cached.request;
  const request = requestUpcomingReleases(settings, start, end);
  upcomingCache.set(key, { expires: Date.now() + 5 * 60_000, request });
  return request;
}

export function clearUpcomingCache() {
  upcomingCache.clear();
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
