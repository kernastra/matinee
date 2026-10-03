import { describe, expect, it } from 'vitest';
import { homeUpcoming, releaseDateLabel, type UpcomingRelease } from './integrations';

function release(overrides: Partial<UpcomingRelease> & Pick<UpcomingRelease, 'id' | 'source' | 'sourceId' | 'date'>): UpcomingRelease {
  return {
    title: 'Title',
    releaseKind: overrides.source === 'sonarr' ? 'Episode' : 'Theatrical',
    genres: [],
    monitored: true,
    downloaded: false,
    ...overrides,
  };
}

describe('media integration calendars', () => {
  it('keeps only the next episode per series on the home shelf', () => {
    const events = [
      release({ id: 'sonarr-1', source: 'sonarr', sourceId: 1, seriesId: 9, title: 'Same Show', date: '2026-08-10T03:00:00Z' }),
      release({ id: 'sonarr-2', source: 'sonarr', sourceId: 2, seriesId: 9, title: 'Same Show', date: '2026-08-17T03:00:00Z' }),
      release({ id: 'sonarr-3', source: 'sonarr', sourceId: 3, seriesId: 10, title: 'Another Show', date: '2026-08-11T03:00:00Z' }),
    ];
    expect(homeUpcoming(events).map((event) => event.id)).toEqual(['sonarr-1', 'sonarr-3']);
  });

  it('prefers a movie home release over its theatrical milestone on Home', () => {
    const events = [
      release({
        id: 'radarr-12-theatrical',
        source: 'radarr',
        sourceId: 12,
        title: 'The Long Wait',
        date: '2026-08-10T00:00:00Z',
        releaseKind: 'Theatrical',
      }),
      release({
        id: 'radarr-12-digital',
        source: 'radarr',
        sourceId: 12,
        title: 'The Long Wait',
        date: '2026-09-08T00:00:00Z',
        releaseKind: 'Digital',
      }),
      release({
        id: 'radarr-12-physical',
        source: 'radarr',
        sourceId: 12,
        title: 'The Long Wait',
        date: '2026-10-01T00:00:00Z',
        releaseKind: 'Physical',
      }),
    ];

    expect(homeUpcoming(events)).toMatchObject([{
      id: 'radarr-12-digital',
      releaseKind: 'Digital',
    }]);
  });

  it('formats nearby releases conversationally', () => {
    expect(releaseDateLabel('2026-08-05T18:00:00Z', new Date('2026-08-05T08:00:00'))).toBe('Today');
    expect(releaseDateLabel('2026-08-06T18:00:00Z', new Date('2026-08-05T08:00:00'))).toBe('Tomorrow');
  });
});
