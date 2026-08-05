import { describe, expect, it } from 'vitest';
import { homeUpcoming, normalizeCalendar, releaseDateLabel } from './integrations';

const start = '2026-08-05T00:00:00.000Z';
const end = '2026-12-03T00:00:00.000Z';

describe('media integration calendars', () => {
  it('normalizes every monitored Radarr release milestone', () => {
    const events = normalizeCalendar('radarr', [{
      id: 7,
      title: 'Future Feature',
      monitored: true,
      hasFile: false,
      inCinemas: '2026-08-20T00:00:00Z',
      digitalRelease: '2026-09-12T00:00:00Z',
      images: [{ coverType: 'poster', remoteUrl: 'https://image.example/poster.jpg' }],
    }], start, end);

    expect(events).toHaveLength(2);
    expect(events).toMatchObject([
      {
        id: 'radarr-7-theatrical',
        title: 'Future Feature',
        releaseKind: 'Theatrical',
        imageUrl: 'https://image.example/poster.jpg',
        milestones: [
          { kind: 'Theatrical', date: '2026-08-20T00:00:00Z' },
          { kind: 'Digital', date: '2026-09-12T00:00:00Z' },
        ],
      },
      {
        id: 'radarr-7-digital',
        releaseKind: 'Digital',
      },
    ]);
  });

  it('normalizes monitored Sonarr episodes with series context', () => {
    const events = normalizeCalendar('sonarr', [{
      id: 22,
      title: 'The Return',
      airDateUtc: '2026-08-10T03:00:00Z',
      monitored: true,
      seriesId: 4,
      seasonNumber: 2,
      episodeNumber: 3,
      series: { id: 4, title: 'Night Shift', monitored: true, genres: ['Drama'] },
    }], start, end);

    expect(events).toMatchObject([{
      id: 'sonarr-22',
      title: 'Night Shift',
      subtitle: 'S02E03 · The Return',
      releaseKind: 'Episode',
    }]);
  });

  it('keeps only the next episode per series on the home shelf', () => {
    const base = normalizeCalendar('sonarr', [
      { id: 1, title: 'One', airDateUtc: '2026-08-10T03:00:00Z', seriesId: 9, series: { id: 9, title: 'Same Show' } },
      { id: 2, title: 'Two', airDateUtc: '2026-08-17T03:00:00Z', seriesId: 9, series: { id: 9, title: 'Same Show' } },
      { id: 3, title: 'Pilot', airDateUtc: '2026-08-11T03:00:00Z', seriesId: 10, series: { id: 10, title: 'Another Show' } },
    ], start, end);
    expect(homeUpcoming(base).map((event) => event.id)).toEqual(['sonarr-1', 'sonarr-3']);
  });

  it('prefers a movie home release over its theatrical milestone on Home', () => {
    const events = normalizeCalendar('radarr', [{
      id: 12,
      title: 'The Long Wait',
      inCinemas: '2026-08-10T00:00:00Z',
      digitalRelease: '2026-09-08T00:00:00Z',
      physicalRelease: '2026-10-01T00:00:00Z',
    }], start, end);

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
