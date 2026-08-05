import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  clearSession,
  clearItemProgress,
  chapterImageUrl,
  getHomeFeed,
  getItemCollectionContext,
  getPlaybackPlan,
  getLibraryItems,
  getFollowingEpisode,
  getNextUpEpisode,
  getSeasonEpisodes,
  getSeriesSeasons,
  getServerInfo,
  getSimilarItems,
  imageUrl,
  loadSession,
  normalizeServerUrl,
  searchLibrary,
  saveSession,
  userImageUrl,
  videoStreamUrl,
  type JellyfinItem,
  type JellyfinSession,
} from './jellyfin';

const session: JellyfinSession = {
  serverUrl: 'http://jellyfin.local:8096',
  accessToken: 'token with spaces',
  user: { Id: 'user-1', Name: 'Sean' },
};

beforeEach(() => sessionStorage.clear());
afterEach(() => vi.unstubAllGlobals());

describe('session storage', () => {
  it('round-trips the current session and clears it', () => {
    saveSession(session);
    expect(loadSession()).toEqual(session);
    clearSession();
    expect(loadSession()).toBeNull();
  });

  it('drops malformed session data', () => {
    sessionStorage.setItem('matinee.session.v1', '{broken');
    expect(loadSession()).toBeNull();
    expect(sessionStorage.getItem('matinee.session.v1')).toBeNull();
  });

  it('migrates a Saintstream session to Matinee', () => {
    sessionStorage.setItem('saintstream.session.v1', JSON.stringify(session));
    expect(loadSession()).toEqual(session);
    expect(sessionStorage.getItem('saintstream.session.v1')).toBeNull();
    expect(sessionStorage.getItem('matinee.session.v1')).toBe(JSON.stringify(session));
  });

  it('drops structurally incomplete session data', () => {
    sessionStorage.setItem('matinee.session.v1', JSON.stringify({ serverUrl: 'http://jellyfin.local:8096' }));
    expect(loadSession()).toBeNull();
    expect(sessionStorage.getItem('matinee.session.v1')).toBeNull();
  });
});

it('builds an encoded Jellyfin image URL', () => {
  const item: JellyfinItem = { Id: 'movie-1', Name: 'Movie', Type: 'Movie' };
  expect(imageUrl(session, item, 'Primary', 360)).toBe(
    'http://jellyfin.local:8096/Items/movie-1/Images/Primary?maxWidth=360&quality=90&api_key=token%20with%20spaces',
  );
});

it('builds an authenticated Jellyfin user image URL', () => {
  const avatarSession: JellyfinSession = {
    ...session,
    user: { ...session.user, PrimaryImageTag: 'avatar tag' },
  };
  const url = new URL(userImageUrl(avatarSession, 96));
  expect(url.pathname).toBe('/Users/user-1/Images/Primary');
  expect(url.searchParams.get('maxWidth')).toBe('96');
  expect(url.searchParams.get('tag')).toBe('avatar tag');
  expect(url.searchParams.get('api_key')).toBe('token with spaces');
});

it('loads public Jellyfin server information through the active session', async () => {
  const fetchMock = vi.fn().mockResolvedValue({
    ok: true,
    json: async () => ({ ServerName: 'Andromeda', Version: '10.10.7' }),
  });
  vi.stubGlobal('fetch', fetchMock);

  await expect(getServerInfo(session)).resolves.toEqual({ ServerName: 'Andromeda', Version: '10.10.7' });
  expect(fetchMock.mock.calls[0][0]).toBe('http://jellyfin.local:8096/System/Info/Public');
  expect(fetchMock.mock.calls[0][1].headers.Authorization).toContain('Token="token with spaces"');
});

it('builds an authenticated direct-play URL', () => {
  const item: JellyfinItem = { Id: 'movie-1', Name: 'Movie', Type: 'Movie' };
  const url = new URL(videoStreamUrl(session, item));
  expect(url.pathname).toBe('/Videos/movie-1/stream');
  expect(url.searchParams.get('Static')).toBe('true');
  expect(url.searchParams.get('api_key')).toBe('token with spaces');
});

it('builds an authenticated tagged chapter image URL', () => {
  const url = new URL(chapterImageUrl(session, 'movie-1', 3, 480, 'chapter-tag'));
  expect(url.pathname).toBe('/Items/movie-1/Images/Chapter/3');
  expect(url.searchParams.get('maxWidth')).toBe('480');
  expect(url.searchParams.get('tag')).toBe('chapter-tag');
  expect(url.searchParams.get('api_key')).toBe('token with spaces');
});

describe('playback negotiation', () => {
  const item: JellyfinItem = { Id: 'movie-1', Name: 'Movie', Type: 'Movie' };

  it('uses a negotiated direct-play source', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        PlaySessionId: 'play-1',
        MediaSources: [{
          Id: 'source-1',
          SupportsDirectPlay: true,
          SupportsDirectStream: true,
          SupportsTranscoding: true,
        }],
      }),
    });
    vi.stubGlobal('fetch', fetchMock);

    const plan = await getPlaybackPlan(session, item, {
      maxStreamingBitrate: 8_000_000,
      audioStreamIndex: 2,
      subtitleStreamIndex: -1,
      startTimeTicks: 90_000_000,
    });
    const request = fetchMock.mock.calls[0];
    const body = JSON.parse(request[1].body as string);
    const url = new URL(plan.url);

    expect(request[0]).toBe('http://jellyfin.local:8096/Items/movie-1/PlaybackInfo');
    expect(body.IsPlayback).toBe(true);
    expect(body.MaxStreamingBitrate).toBe(8_000_000);
    expect(body.AudioStreamIndex).toBe(2);
    expect(body.SubtitleStreamIndex).toBe(-1);
    expect(body.StartTimeTicks).toBe(90_000_000);
    expect(body.DeviceProfile.DirectPlayProfiles).toContainEqual(
      expect.objectContaining({ Container: 'hls', VideoCodec: 'h264', AudioCodec: 'aac' }),
    );
    expect(body.DeviceProfile.TranscodingProfiles[0].Protocol).toBe('hls');
    expect(plan.method).toBe('DirectPlay');
    expect(plan.playSessionId).toBe('play-1');
    expect(url.searchParams.get('MediaSourceId')).toBe('source-1');
  });

  it('uses Jellyfin’s authenticated HLS fallback', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        PlaySessionId: 'play-2',
        MediaSources: [{
          SupportsDirectPlay: false,
          SupportsDirectStream: false,
          SupportsTranscoding: true,
          TranscodingUrl: '/Videos/movie-1/master.m3u8?MediaSourceId=source-1',
        }],
      }),
    }));

    const plan = await getPlaybackPlan(session, item);
    const url = new URL(plan.url);

    expect(plan.method).toBe('Transcode');
    expect(url.pathname).toBe('/Videos/movie-1/master.m3u8');
    expect(url.searchParams.get('api_key')).toBe('token with spaces');
  });

  it('preserves legacy direct play when negotiation rejects a playable item', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ ErrorCode: 'NoCompatibleStream', MediaSources: [] }),
    }));

    const plan = await getPlaybackPlan(session, item);
    const url = new URL(plan.url);

    expect(plan.method).toBe('DirectPlay');
    expect(url.pathname).toBe('/Videos/movie-1/stream');
    expect(url.searchParams.get('Static')).toBe('true');
  });
});

describe('library navigation queries', () => {
  function mockItems(items: JellyfinItem[] = []) {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ Items: items }) });
    vi.stubGlobal('fetch', fetchMock);
    return fetchMock;
  }

  it('loads the complete home feed in parallel from Jellyfin', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ Items: [] }) });
    vi.stubGlobal('fetch', fetchMock);

    const feed = await getHomeFeed(session);
    const urls = fetchMock.mock.calls.map((call) => new URL(String(call[0])));

    expect(fetchMock).toHaveBeenCalledTimes(6);
    expect(feed).toEqual({ resume: [], latest: [], movies: [], series: [], topRated: [], favorites: [] });
    expect(urls.some((url) => url.searchParams.get('SortBy') === 'CommunityRating')).toBe(true);
    expect(urls.some((url) => url.searchParams.get('Filters') === 'IsFavorite')).toBe(true);
    expect(urls.some((url) =>
      url.searchParams.get('IncludeItemTypes') === 'Movie,Series'
      && url.searchParams.get('SortBy') === 'DateCreated'
      && url.searchParams.get('SortOrder') === 'Descending'
      && url.searchParams.get('Limit') === '6'
    )).toBe(true);
  });

  it('loads and sorts a dedicated movie library', async () => {
    const fetchMock = mockItems();
    await getLibraryItems(session, 'Movie', 'DateCreated');
    const url = new URL(fetchMock.mock.calls[0][0]);
    expect(url.pathname).toBe('/Users/user-1/Items');
    expect(url.searchParams.get('IncludeItemTypes')).toBe('Movie');
    expect(url.searchParams.get('SortOrder')).toBe('Descending');
  });

  it('loads similar titles from the current Jellyfin library', async () => {
    const fetchMock = mockItems([{ Id: 'movie-2', Name: 'Sequel', Type: 'Movie' }]);
    await expect(getSimilarItems(session, 'movie-1', 8)).resolves.toHaveLength(1);
    const url = new URL(fetchMock.mock.calls[0][0]);
    expect(url.pathname).toBe('/Items/movie-1/Similar');
    expect(url.searchParams.get('userId')).toBe('user-1');
    expect(url.searchParams.get('limit')).toBe('8');
  });

  it('loads collection membership and the collection titles', async () => {
    const fetchMock = vi.fn((input: string | URL | Request) => {
      const url = new URL(String(input));
      const items = url.pathname.endsWith('/Collections')
        ? [{ Id: 'box-1', Name: 'The Trilogy', Type: 'BoxSet' }]
        : [{ Id: 'movie-1', Name: 'Movie', Type: 'Movie' }];
      return Promise.resolve({ ok: true, json: async () => ({ Items: items }) });
    });
    vi.stubGlobal('fetch', fetchMock);

    await expect(getItemCollectionContext(session, 'movie-1')).resolves.toEqual([{
      collection: { Id: 'box-1', Name: 'The Trilogy', Type: 'BoxSet' },
      items: [{ Id: 'movie-1', Name: 'Movie', Type: 'Movie' }],
    }]);
    const urls = fetchMock.mock.calls.map((call) => new URL(String(call[0])));
    expect(urls[0].pathname).toBe('/Items/movie-1/Collections');
    expect(urls[1].searchParams.get('ParentId')).toBe('box-1');
  });

  it('clears resume progress without discarding the rest of user data', async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ IsFavorite: true, PlaybackPositionTicks: 42, PlayedPercentage: 12 }),
      })
      .mockResolvedValueOnce({ ok: true });
    vi.stubGlobal('fetch', fetchMock);

    await clearItemProgress(session, 'movie-1');
    expect(fetchMock.mock.calls[0][0]).toBe('http://jellyfin.local:8096/UserItems/movie-1/UserData');
    const update = fetchMock.mock.calls[1];
    expect(update[1].method).toBe('POST');
    expect(JSON.parse(update[1].body)).toEqual(expect.objectContaining({
      IsFavorite: true,
      PlaybackPositionTicks: 0,
      PlayedPercentage: 0,
    }));
  });

  it('searches movies, series, and episodes', async () => {
    const fetchMock = mockItems();
    await searchLibrary(session, 'Interstellar');
    const url = new URL(fetchMock.mock.calls[0][0]);
    expect(url.searchParams.get('SearchTerm')).toBe('Interstellar');
    expect(url.searchParams.get('IncludeItemTypes')).toBe('Movie,Series,Episode');
  });

  it('loads seasons, episodes, and next-up from the TV endpoints', async () => {
    const fetchMock = mockItems();
    await getSeriesSeasons(session, 'series-1');
    await getSeasonEpisodes(session, 'series-1', 'season-1');
    await getNextUpEpisode(session, 'series-1');
    await getFollowingEpisode(session, { Id: 'episode-1', Name: 'Pilot', Type: 'Episode', SeriesId: 'series-1' });

    const urls = fetchMock.mock.calls.map((call) => new URL(call[0]));
    expect(urls[0].pathname).toBe('/Shows/series-1/Seasons');
    expect(urls[1].pathname).toBe('/Shows/series-1/Episodes');
    expect(urls[1].searchParams.get('seasonId')).toBe('season-1');
    expect(urls[2].pathname).toBe('/Shows/NextUp');
    expect(urls[2].searchParams.get('seriesId')).toBe('series-1');
    expect(urls[3].pathname).toBe('/Shows/series-1/Episodes');
    expect(urls[3].searchParams.get('startItemId')).toBe('episode-1');
  });
});

describe('Jellyfin address normalization', () => {
  it('accepts a bare LAN address', () => {
    expect(normalizeServerUrl('jellyfin.local:8096')).toBe('http://jellyfin.local:8096');
  });

  it('removes a copied Jellyfin web-client path', () => {
    expect(
      normalizeServerUrl('http://jellyfin.local:8096/web/index.html#!/home.html'),
    ).toBe('http://jellyfin.local:8096');
  });

  it('preserves a configured base path before the web client', () => {
    expect(normalizeServerUrl('https://media.example.com/jellyfin/web/')).toBe(
      'https://media.example.com/jellyfin',
    );
  });
});
