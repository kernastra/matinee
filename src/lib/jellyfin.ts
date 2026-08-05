const CLIENT_NAME = 'Matinee';
export const APP_VERSION = '0.5.0';
const DEVICE_NAME = 'Desktop';
const DEVICE_ID = 'matinee-desktop';
const SESSION_KEY = 'matinee.session.v1';
const LEGACY_SESSION_KEY = 'saintstream.session.v1';
const PREVIOUS_JELLYFIN_HOST = '192.168.1.249';
export const DEFAULT_JELLYFIN_URL = 'http://192.168.1.158:8096';

export type JellyfinUser = {
  Id: string;
  Name: string;
  PrimaryImageTag?: string;
};

export type JellyfinSession = {
  serverUrl: string;
  accessToken: string;
  user: JellyfinUser;
};

export type JellyfinItem = {
  Id: string;
  Name: string;
  Type: 'Movie' | 'Series' | 'Episode' | string;
  Overview?: string;
  ProductionYear?: number;
  RunTimeTicks?: number;
  CommunityRating?: number;
  OfficialRating?: string;
  Genres?: string[];
  ImageTags?: { Primary?: string; Logo?: string };
  BackdropImageTags?: string[];
  ParentId?: string;
  SeriesId?: string;
  SeriesName?: string;
  SeasonId?: string;
  SeasonName?: string;
  IndexNumber?: number;
  ParentIndexNumber?: number;
  PremiereDate?: string;
  DateCreated?: string;
  EndDate?: string;
  Taglines?: string[];
  Studios?: Array<{ Name: string }>;
  People?: Array<{ Id?: string; Name: string; Type?: string; Role?: string; PrimaryImageTag?: string }>;
  ProductionLocations?: string[];
  ProviderIds?: Record<string, string>;
  CriticRating?: number;
  Chapters?: ChapterInfo[];
  MediaSources?: MediaSourceInfo[];
  MediaStreams?: MediaStream[];
  UserData?: {
    PlaybackPositionTicks?: number;
    PlayedPercentage?: number;
    IsFavorite?: boolean;
    Played?: boolean;
    PlayCount?: number;
  };
};

export type ChapterInfo = {
  Name?: string;
  StartPositionTicks?: number;
  ImageTag?: string;
};

type AuthenticationResult = {
  AccessToken: string;
  User: JellyfinUser;
};

export type JellyfinServerInfo = {
  ServerName?: string;
  Version?: string;
  OperatingSystem?: string;
  ProductName?: string;
  Id?: string;
};

type ItemsResult = { Items: JellyfinItem[] };

export function normalizeServerUrl(value: string) {
  const trimmed = value.trim();
  if (!trimmed) throw new Error('Enter your Jellyfin server address.');

  const withScheme = /^[a-z][a-z\d+.-]*:\/\//i.test(trimmed) ? trimmed : `http://${trimmed}`;
  let url: URL;
  try {
    url = new URL(withScheme);
  } catch {
    throw new Error('Enter a valid Jellyfin address, such as 192.168.1.158:8096.');
  }

  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new Error('Jellyfin addresses must begin with http:// or https://.');
  }

  // A browser often copies Jellyfin as `/web/index.html#!/home.html`. The API
  // lives at the server root (or immediately before `/web` when a base path is
  // configured), so remove only the client-app suffix and preserve a base path.
  url.hash = '';
  url.search = '';
  url.pathname = url.pathname.replace(/\/web(?:\/.*)?$/i, '').replace(/\/+$/, '');
  return url.toString().replace(/\/+$/, '');
}

function authorizationHeader(token?: string) {
  const fields = [
    `Client="${CLIENT_NAME}"`,
    `Device="${DEVICE_NAME}"`,
    `DeviceId="${DEVICE_ID}"`,
    `Version="${APP_VERSION}"`,
  ];
  if (token) fields.push(`Token="${token}"`);
  return `MediaBrowser ${fields.join(', ')}`;
}

async function readError(response: Response) {
  if (response.status === 401) return 'That username or password was not accepted.';
  if (response.status === 404) return 'Jellyfin was not found at that address.';
  return `Jellyfin returned ${response.status} ${response.statusText}.`;
}

export async function authenticate(
  serverUrl: string,
  username: string,
  password: string,
): Promise<JellyfinSession> {
  const normalizedUrl = normalizeServerUrl(serverUrl);
  let response: Response;

  try {
    console.info('[jellyfin] authentication request', { serverUrl: normalizedUrl });
    response = await fetch(`${normalizedUrl}/Users/AuthenticateByName`, {
      method: 'POST',
      headers: {
        Authorization: authorizationHeader(),
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ Username: username, Pw: password }),
    });
  } catch {
    console.error('[jellyfin] authentication request could not reach server', {
      serverUrl: normalizedUrl,
    });
    throw new Error('Could not reach Jellyfin. Check the server address and try again.');
  }

  if (!response.ok) {
    console.error('[jellyfin] authentication rejected', { status: response.status });
    throw new Error(await readError(response));
  }
  const result = (await response.json()) as AuthenticationResult;
  console.info('[jellyfin] authentication succeeded', { serverUrl: normalizedUrl });
  return { serverUrl: normalizedUrl, accessToken: result.AccessToken, user: result.User };
}

async function get<T>(session: JellyfinSession, path: string): Promise<T> {
  const response = await fetch(`${session.serverUrl}${path}`, {
    headers: { Authorization: authorizationHeader(session.accessToken) },
  });
  if (!response.ok) throw new Error(await readError(response));
  return response.json() as Promise<T>;
}

export function getServerInfo(session: JellyfinSession) {
  return get<JellyfinServerInfo>(session, '/System/Info/Public');
}

async function send(session: JellyfinSession, path: string, method: 'POST' | 'DELETE') {
  const response = await fetch(`${session.serverUrl}${path}`, {
    method,
    headers: { Authorization: authorizationHeader(session.accessToken) },
  });
  if (!response.ok) throw new Error(await readError(response));
}

const fields = [
  'Overview',
  'Genres',
  'RunTimeTicks',
  'ProductionYear',
  'CommunityRating',
  'CriticRating',
  'OfficialRating',
  'PrimaryImageAspectRatio',
  'MediaStreams',
  'MediaSources',
  'Chapters',
  'ParentId',
  'DateCreated',
  'PremiereDate',
  'EndDate',
  'Taglines',
  'Studios',
  'People',
  'ProductionLocations',
  'ProviderIds',
].join(',');

export type HomeFeed = {
  resume: JellyfinItem[];
  latest: JellyfinItem[];
  movies: JellyfinItem[];
  series: JellyfinItem[];
  topRated: JellyfinItem[];
  favorites: JellyfinItem[];
};

export async function getHomeFeed(session: JellyfinSession): Promise<HomeFeed> {
  const userId = session.user.Id;
  const [resume, latest, movies, series, topRated, favorites] = await Promise.all([
    get<ItemsResult>(
      session,
      `/Users/${userId}/Items/Resume?Limit=12&MediaTypes=Video&Fields=${fields}`,
    ).then((result) => result.Items),
    get<ItemsResult>(
      session,
      `/Users/${userId}/Items?Recursive=true&IncludeItemTypes=Movie,Series&SortBy=DateCreated&SortOrder=Descending&Limit=6&Fields=${fields}&EnableUserData=true`,
    ).then((result) => result.Items),
    get<ItemsResult>(
      session,
      `/Users/${userId}/Items?Recursive=true&IncludeItemTypes=Movie&SortBy=DateCreated&SortOrder=Descending&Limit=12&Fields=${fields}`,
    ).then((result) => result.Items),
    get<ItemsResult>(
      session,
      `/Users/${userId}/Items?Recursive=true&IncludeItemTypes=Series&SortBy=DateCreated&SortOrder=Descending&Limit=12&Fields=${fields}`,
    ).then((result) => result.Items),
    get<ItemsResult>(
      session,
      `/Users/${userId}/Items?Recursive=true&IncludeItemTypes=Movie,Series&SortBy=CommunityRating&SortOrder=Descending&Limit=12&Fields=${fields}&EnableUserData=true`,
    ).then((result) => result.Items).catch(() => []),
    get<ItemsResult>(
      session,
      `/Users/${userId}/Items?Recursive=true&IncludeItemTypes=Movie,Series&Filters=IsFavorite&SortBy=DateCreated&SortOrder=Descending&Limit=12&Fields=${fields}&EnableUserData=true`,
    ).then((result) => result.Items).catch(() => []),
  ]);
  return { resume, latest, movies, series, topRated, favorites };
}

function itemQuery(params: Record<string, string | number | boolean | undefined>) {
  const query = new URLSearchParams();
  Object.entries(params).forEach(([key, value]) => {
    if (value !== undefined) query.set(key, String(value));
  });
  return query.toString();
}

export async function getLibraryItems(
  session: JellyfinSession,
  type: 'Movie' | 'Series',
  sortBy = 'SortName',
): Promise<JellyfinItem[]> {
  const query = itemQuery({
    Recursive: true,
    IncludeItemTypes: type,
    SortBy: sortBy,
    SortOrder: ['DateCreated', 'ProductionYear', 'CommunityRating'].includes(sortBy)
      ? 'Descending'
      : 'Ascending',
    Limit: 240,
    Fields: fields,
    EnableUserData: true,
  });
  return get<ItemsResult>(session, `/Users/${session.user.Id}/Items?${query}`)
    .then((result) => result.Items);
}

export async function getItemDetails(
  session: JellyfinSession,
  itemId: string,
): Promise<JellyfinItem> {
  const query = itemQuery({
    userId: session.user.Id,
    Fields: fields,
    EnableUserData: true,
  });
  return get<JellyfinItem>(session, `/Users/${session.user.Id}/Items/${itemId}?${query}`);
}

export async function getSimilarItems(
  session: JellyfinSession,
  itemId: string,
  limit = 12,
): Promise<JellyfinItem[]> {
  const query = itemQuery({
    userId: session.user.Id,
    limit,
    fields,
  });
  return get<ItemsResult>(session, `/Items/${itemId}/Similar?${query}`)
    .then((result) => result.Items);
}

export type JellyfinCollectionContext = {
  collection: JellyfinItem;
  items: JellyfinItem[];
};

export async function getItemCollectionContext(
  session: JellyfinSession,
  itemId: string,
): Promise<JellyfinCollectionContext[]> {
  const collectionsQuery = itemQuery({
    userId: session.user.Id,
    limit: 2,
    fields,
  });
  const collections = await get<ItemsResult>(
    session,
    `/Items/${itemId}/Collections?${collectionsQuery}`,
  ).then((result) => result.Items);

  return Promise.all(collections.map(async (collection) => {
    const itemsQuery = itemQuery({
      ParentId: collection.Id,
      Recursive: true,
      IncludeItemTypes: 'Movie,Series',
      SortBy: 'ProductionYear,SortName',
      SortOrder: 'Ascending',
      Fields: fields,
      EnableUserData: true,
    });
    const items = await get<ItemsResult>(
      session,
      `/Users/${session.user.Id}/Items?${itemsQuery}`,
    ).then((result) => result.Items);
    return { collection, items };
  }));
}

export function chapterImageUrl(
  session: JellyfinSession,
  itemId: string,
  chapterIndex: number,
  width = 420,
  tag?: string,
) {
  const query = new URLSearchParams({
    maxWidth: String(width),
    quality: '88',
    api_key: session.accessToken,
  });
  if (tag) query.set('tag', tag);
  return `${session.serverUrl}/Items/${itemId}/Images/Chapter/${chapterIndex}?${query.toString()}`;
}

export async function setItemFavorite(
  session: JellyfinSession,
  itemId: string,
  favorite: boolean,
) {
  await send(session, `/Users/${session.user.Id}/FavoriteItems/${itemId}`, favorite ? 'POST' : 'DELETE');
}

export async function setItemPlayed(
  session: JellyfinSession,
  itemId: string,
  played: boolean,
) {
  await send(session, `/Users/${session.user.Id}/PlayedItems/${itemId}`, played ? 'POST' : 'DELETE');
}

export async function clearItemProgress(session: JellyfinSession, itemId: string) {
  const userData = await get<NonNullable<JellyfinItem['UserData']>>(
    session,
    `/UserItems/${itemId}/UserData`,
  );
  const response = await fetch(`${session.serverUrl}/UserItems/${itemId}/UserData`, {
    method: 'POST',
    headers: {
      Authorization: authorizationHeader(session.accessToken),
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      ...userData,
      PlaybackPositionTicks: 0,
      PlayedPercentage: 0,
    }),
  });
  if (!response.ok) throw new Error(await readError(response));
}

export async function searchLibrary(
  session: JellyfinSession,
  searchTerm: string,
  signal?: AbortSignal,
): Promise<JellyfinItem[]> {
  const query = itemQuery({
    Recursive: true,
    SearchTerm: searchTerm,
    IncludeItemTypes: 'Movie,Series,Episode',
    Limit: 40,
    Fields: fields,
    EnableUserData: true,
  });
  const response = await fetch(`${session.serverUrl}/Users/${session.user.Id}/Items?${query}`, {
    signal,
    headers: { Authorization: authorizationHeader(session.accessToken) },
  });
  if (!response.ok) throw new Error(await readError(response));
  return ((await response.json()) as ItemsResult).Items;
}

export async function getSeriesSeasons(
  session: JellyfinSession,
  seriesId: string,
): Promise<JellyfinItem[]> {
  const query = itemQuery({
    userId: session.user.Id,
    fields,
    isMissing: false,
    enableImages: true,
    enableUserData: true,
  });
  return get<ItemsResult>(session, `/Shows/${seriesId}/Seasons?${query}`)
    .then((result) => result.Items);
}

export async function getSeasonEpisodes(
  session: JellyfinSession,
  seriesId: string,
  seasonId: string,
): Promise<JellyfinItem[]> {
  const query = itemQuery({
    userId: session.user.Id,
    seasonId,
    fields,
    isMissing: false,
    enableImages: true,
    enableUserData: true,
    sortBy: 'IndexNumber',
  });
  return get<ItemsResult>(session, `/Shows/${seriesId}/Episodes?${query}`)
    .then((result) => result.Items);
}

export async function getNextUpEpisode(
  session: JellyfinSession,
  seriesId: string,
): Promise<JellyfinItem | null> {
  const query = itemQuery({
    userId: session.user.Id,
    seriesId,
    limit: 1,
    fields,
    enableImages: true,
    enableUserData: true,
    enableResumable: true,
  });
  return get<ItemsResult>(session, `/Shows/NextUp?${query}`)
    .then((result) => result.Items[0] ?? null);
}

export async function getFollowingEpisode(
  session: JellyfinSession,
  episode: JellyfinItem,
): Promise<JellyfinItem | null> {
  if (!episode.SeriesId) return null;
  const query = itemQuery({
    userId: session.user.Id,
    startItemId: episode.Id,
    limit: 2,
    fields,
    isMissing: false,
    enableImages: true,
    enableUserData: true,
    sortBy: 'AiredEpisodeOrder',
  });
  return get<ItemsResult>(session, `/Shows/${episode.SeriesId}/Episodes?${query}`)
    .then((result) => result.Items.find((item) => item.Id !== episode.Id) ?? null);
}

export function imageUrl(
  session: JellyfinSession,
  item: JellyfinItem,
  type: 'Primary' | 'Backdrop' | 'Logo',
  width: number,
) {
  return `${session.serverUrl}/Items/${item.Id}/Images/${type}?maxWidth=${width}&quality=90&api_key=${encodeURIComponent(session.accessToken)}`;
}

export function userImageUrl(session: JellyfinSession, width: number) {
  const query = new URLSearchParams({
    maxWidth: String(width),
    quality: '90',
    api_key: session.accessToken,
  });
  if (session.user.PrimaryImageTag) query.set('tag', session.user.PrimaryImageTag);
  return `${session.serverUrl}/Users/${session.user.Id}/Images/Primary?${query.toString()}`;
}

export function backdropUrl(session: JellyfinSession, item: JellyfinItem, width: number) {
  return imageUrl(session, item, item.BackdropImageTags?.length ? 'Backdrop' : 'Primary', width);
}

export function backdropImageUrl(
  session: JellyfinSession,
  item: JellyfinItem,
  index: number,
  width: number,
) {
  const query = new URLSearchParams({
    maxWidth: String(width),
    quality: '90',
    api_key: session.accessToken,
  });
  const tag = item.BackdropImageTags?.[index];
  if (tag) query.set('tag', tag);
  return `${session.serverUrl}/Items/${item.Id}/Images/Backdrop/${index}?${query.toString()}`;
}

export function videoStreamUrl(session: JellyfinSession, item: JellyfinItem) {
  const query = new URLSearchParams({
    Static: 'true',
    DeviceId: DEVICE_ID,
    api_key: session.accessToken,
  });
  return `${session.serverUrl}/Videos/${item.Id}/stream?${query.toString()}`;
}

export type MediaSourceInfo = {
  Id?: string;
  Name?: string;
  Path?: string;
  Container?: string;
  Size?: number;
  Bitrate?: number;
  RunTimeTicks?: number;
  SupportsDirectPlay?: boolean;
  SupportsDirectStream?: boolean;
  SupportsTranscoding?: boolean;
  TranscodingUrl?: string;
  MediaStreams?: MediaStream[];
  DefaultAudioStreamIndex?: number;
  DefaultSubtitleStreamIndex?: number;
};

export type MediaStream = {
  Index: number;
  Type: 'Audio' | 'Video' | 'Subtitle' | string;
  Codec?: string;
  Width?: number;
  Height?: number;
  VideoRange?: string;
  Language?: string;
  Title?: string;
  DisplayTitle?: string;
  Channels?: number;
  ChannelLayout?: string;
  BitRate?: number;
  BitDepth?: number;
  SampleRate?: number;
  Profile?: string;
  Level?: number;
  PixelFormat?: string;
  AverageFrameRate?: number;
  RealFrameRate?: number;
  VideoRangeType?: string;
  ColorSpace?: string;
  IsDefault?: boolean;
  IsForced?: boolean;
  IsExternal?: boolean;
};

type PlaybackInfoResponse = {
  MediaSources?: MediaSourceInfo[];
  PlaySessionId?: string;
  ErrorCode?: 'NotAllowed' | 'NoCompatibleStream' | 'RateLimitExceeded';
};

export type PlaybackMethod = 'DirectPlay' | 'DirectStream' | 'Transcode';

export type PlaybackPlan = {
  url: string;
  playSessionId: string;
  method: PlaybackMethod;
  mediaStreams: MediaStream[];
  audioStreamIndex?: number;
  subtitleStreamIndex?: number;
};

export type PlaybackOptions = {
  maxStreamingBitrate?: number;
  audioStreamIndex?: number;
  subtitleStreamIndex?: number;
  startTimeTicks?: number;
};

const videoDeviceProfile = {
  Name: 'Matinee WebKit',
  MaxStreamingBitrate: 120_000_000,
  MaxStaticBitrate: 120_000_000,
  DirectPlayProfiles: [
    {
      Container: 'mp4,m4v',
      Type: 'Video',
      VideoCodec: 'h264',
      AudioCodec: 'aac,mp3',
    },
    {
      Container: 'hls',
      Type: 'Video',
      VideoCodec: 'h264',
      AudioCodec: 'aac',
    },
  ],
  TranscodingProfiles: [
    {
      Container: 'ts',
      Type: 'Video',
      VideoCodec: 'h264',
      AudioCodec: 'aac',
      Context: 'Streaming',
      Protocol: 'hls',
      MaxAudioChannels: '2',
      MinSegments: 1,
      BreakOnNonKeyFrames: true,
    },
  ],
  ContainerProfiles: [],
  CodecProfiles: [],
  SubtitleProfiles: [],
};

export async function getPlaybackPlan(
  session: JellyfinSession,
  item: JellyfinItem,
  options: PlaybackOptions = {},
  signal?: AbortSignal,
): Promise<PlaybackPlan> {
  const startTimeTicks = options.startTimeTicks ?? item.UserData?.PlaybackPositionTicks ?? 0;
  const response = await fetch(`${session.serverUrl}/Items/${item.Id}/PlaybackInfo`, {
    method: 'POST',
    signal,
    headers: {
      Authorization: authorizationHeader(session.accessToken),
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      UserId: session.user.Id,
      StartTimeTicks: startTimeTicks,
      MaxStreamingBitrate: options.maxStreamingBitrate,
      AudioStreamIndex: options.audioStreamIndex,
      SubtitleStreamIndex: options.subtitleStreamIndex,
      IsPlayback: true,
      AutoOpenLiveStream: true,
      EnableDirectPlay: true,
      EnableDirectStream: true,
      EnableTranscoding: true,
      AllowVideoStreamCopy: true,
      AllowAudioStreamCopy: true,
      DeviceProfile: videoDeviceProfile,
    }),
  });

  if (!response.ok) throw new Error(await readError(response));
  const result = (await response.json()) as PlaybackInfoResponse;
  if (result.ErrorCode === 'NoCompatibleStream' && (item.Type === 'Movie' || item.Type === 'Episode')) {
    console.warn('[playback] negotiation found no compatible stream; trying legacy direct play', {
      itemId: item.Id,
      itemType: item.Type,
    });
    return {
      url: videoStreamUrl(session, item),
      playSessionId: result.PlaySessionId || crypto.randomUUID(),
      method: 'DirectPlay',
      mediaStreams: [],
    };
  }
  if (result.ErrorCode) {
    const messages = {
      NotAllowed: 'Your Jellyfin account is not allowed to play this title.',
      NoCompatibleStream: 'Jellyfin could not create a compatible video stream.',
      RateLimitExceeded: 'Jellyfin is busy preparing other streams. Try again shortly.',
    };
    throw new Error(messages[result.ErrorCode]);
  }

  const sources = result.MediaSources ?? [];
  console.info('[playback] negotiation response', {
    hasPlaySession: Boolean(result.PlaySessionId),
    errorCode: result.ErrorCode,
    mediaSources: sources.map((source) => ({
      directPlay: source.SupportsDirectPlay,
      directStream: source.SupportsDirectStream,
      transcode: source.SupportsTranscoding,
      hasTranscodingUrl: Boolean(source.TranscodingUrl),
    })),
  });
  const source = sources.find((candidate) => candidate.SupportsDirectPlay)
    ?? sources.find((candidate) => candidate.TranscodingUrl)
    ?? sources[0];
  if (!source) throw new Error('No playable media source was returned by Jellyfin.');

  const playSessionId = result.PlaySessionId || crypto.randomUUID();
  const mediaStreams = source.MediaStreams ?? [];
  if (source.SupportsDirectPlay) {
    const query = new URLSearchParams({
      Static: 'true',
      DeviceId: DEVICE_ID,
      PlaySessionId: playSessionId,
      api_key: session.accessToken,
    });
    if (source.Id) query.set('MediaSourceId', source.Id);
    return {
      url: `${session.serverUrl}/Videos/${item.Id}/stream?${query.toString()}`,
      playSessionId,
      method: 'DirectPlay',
      mediaStreams,
      audioStreamIndex: options.audioStreamIndex ?? source.DefaultAudioStreamIndex,
      subtitleStreamIndex: options.subtitleStreamIndex ?? source.DefaultSubtitleStreamIndex,
    };
  }

  if (!source.TranscodingUrl) {
    throw new Error('Jellyfin returned media information without a playable stream URL.');
  }

  const streamUrl = new URL(source.TranscodingUrl, `${session.serverUrl}/`);
  if (!streamUrl.searchParams.has('api_key')) {
    streamUrl.searchParams.set('api_key', session.accessToken);
  }
  return {
    url: streamUrl.toString(),
    playSessionId,
    method: source.SupportsDirectStream ? 'DirectStream' : 'Transcode',
    mediaStreams,
    audioStreamIndex: options.audioStreamIndex ?? source.DefaultAudioStreamIndex,
    subtitleStreamIndex: options.subtitleStreamIndex ?? source.DefaultSubtitleStreamIndex,
  };
}

export type PlaybackEvent = 'start' | 'progress' | 'stopped';

export async function reportPlayback(
  session: JellyfinSession,
  item: JellyfinItem,
  event: PlaybackEvent,
  playSessionId: string,
  positionSeconds: number,
  isPaused: boolean,
  playMethod: PlaybackMethod = 'DirectPlay',
) {
  const endpoint = event === 'start'
    ? '/Sessions/Playing'
    : event === 'progress'
      ? '/Sessions/Playing/Progress'
      : '/Sessions/Playing/Stopped';

  const response = await fetch(`${session.serverUrl}${endpoint}`, {
    method: 'POST',
    headers: {
      Authorization: authorizationHeader(session.accessToken),
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      ItemId: item.Id,
      PlaySessionId: playSessionId,
      PositionTicks: Math.round(positionSeconds * 10_000_000),
      IsPaused: isPaused,
      CanSeek: true,
      PlayMethod: playMethod,
    }),
  });

  if (!response.ok) throw new Error(await readError(response));
}

export function saveSession(session: JellyfinSession) {
  sessionStorage.setItem(SESSION_KEY, JSON.stringify(session));
  sessionStorage.removeItem(LEGACY_SESSION_KEY);
}

export function loadSession(): JellyfinSession | null {
  const value = sessionStorage.getItem(SESSION_KEY) ?? sessionStorage.getItem(LEGACY_SESSION_KEY);
  if (!value) return null;
  try {
    const session = JSON.parse(value) as JellyfinSession;
    const server = new URL(session.serverUrl);
    if (server.hostname === PREVIOUS_JELLYFIN_HOST) {
      server.hostname = new URL(DEFAULT_JELLYFIN_URL).hostname;
      session.serverUrl = server.toString().replace(/\/+$/, '');
    }
    sessionStorage.setItem(SESSION_KEY, JSON.stringify(session));
    sessionStorage.removeItem(LEGACY_SESSION_KEY);
    return session;
  } catch {
    sessionStorage.removeItem(SESSION_KEY);
    sessionStorage.removeItem(LEGACY_SESSION_KEY);
    return null;
  }
}

export function clearSession() {
  sessionStorage.removeItem(SESSION_KEY);
  sessionStorage.removeItem(LEGACY_SESSION_KEY);
}
