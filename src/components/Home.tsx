import { useEffect, useMemo, useState } from 'react';
import {
  getHomeFeed,
  backdropUrl,
  type HomeFeed,
  type JellyfinItem,
  type JellyfinSession,
} from '../lib/jellyfin';
import MediaRow from './MediaRow';
import MaterialIcon from './MaterialIcon';
import AppNav, { type AppView } from './AppNav';
import type { AppSettings } from '../lib/settings';
import { debugWarn } from '../lib/logger';
import {
  fetchUpcomingReleases,
  homeUpcoming,
  integrationEnabled,
  upcomingWindow,
  type UpcomingRelease,
} from '../lib/integrations';
import {
  CollectionSpotlight,
  FeaturedShowcase,
  HomeFooter,
  LibraryShortcuts,
  RankedShelf,
} from './HomeEditorial';
import ComingSoonShelf from './ComingSoonShelf';

const HERO_ROTATION_MS = 7_000;

type HomeProps = {
  session: JellyfinSession;
  settings: AppSettings;
  onSignOut: () => void;
  onNavigate: (view: AppView) => void;
  onSearch: () => void;
  onSelect: (item: HomeFeed['movies'][number]) => void;
  onPlay: (item: HomeFeed['movies'][number]) => void;
};

function runtime(ticks?: number) {
  if (!ticks) return '';
  const minutes = Math.round(ticks / 600_000_000);
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

function heroItems(feed: HomeFeed) {
  const candidates = [...feed.resume, ...feed.latest, ...feed.movies, ...feed.series];
  const seen = new Set<string>();
  const unique = candidates.filter((item) => {
    if (seen.has(item.Id)) return false;
    seen.add(item.Id);
    return true;
  });
  const withBackdrops = unique.filter((item) => item.BackdropImageTags?.length);
  return (withBackdrops.length ? withBackdrops : unique).slice(0, 8);
}

function mergeUnique(...groups: JellyfinItem[][]) {
  const seen = new Set<string>();
  return groups.flat().filter((item) => {
    if (seen.has(item.Id)) return false;
    seen.add(item.Id);
    return true;
  });
}

export default function Home({ session, settings, onSignOut, onNavigate, onSearch, onSelect, onPlay }: HomeProps) {
  const [feed, setFeed] = useState<HomeFeed | null>(null);
  const [error, setError] = useState('');
  const [heroIndex, setHeroIndex] = useState(0);
  const [upcoming, setUpcoming] = useState<UpcomingRelease[]>([]);
  const calendarEnabled = integrationEnabled(settings);

  useEffect(() => {
    let cancelled = false;
    getHomeFeed(session)
      .then((result) => {
        if (!cancelled) setFeed(result);
      })
      .catch((reason) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : 'Could not load home.');
      });
    return () => {
      cancelled = true;
    };
  }, [session]);

  useEffect(() => {
    let cancelled = false;
    if (!calendarEnabled) {
      setUpcoming([]);
      return () => { cancelled = true; };
    }
    const window = upcomingWindow();
    fetchUpcomingReleases(settings, window.start, window.end)
      .then((result) => {
        if (!cancelled) setUpcoming(homeUpcoming(result.events));
      })
      .catch((reason) => {
        debugWarn('[calendar] could not load the home shelf', reason);
        if (!cancelled) setUpcoming([]);
      });
    return () => { cancelled = true; };
  }, [calendarEnabled, settings.radarrUrl, settings.sonarrUrl]);

  const heroes = useMemo(() => (feed ? heroItems(feed) : []), [feed]);
  const hero = heroes[heroIndex % heroes.length];
  const rotatesHero = settings.heroRotation && !settings.reducedMotion && heroes.length > 1;

  useEffect(() => {
    if (!rotatesHero) return;
    const rotation = window.setInterval(() => {
      setHeroIndex((current) => (current + 1) % heroes.length);
    }, HERO_ROTATION_MS);
    return () => window.clearInterval(rotation);
  }, [heroes.length, rotatesHero]);

  if (error) {
    return (
      <main className="center-state">
        <p>{error}</p>
        <button className="secondary-button" type="button" onClick={onSignOut}>Sign out</button>
      </main>
    );
  }

  if (!feed) {
    return (
      <main className="center-state loading-state">
        <span className="loading-mark">M</span>
        <p>Curating your evening…</p>
      </main>
    );
  }

  if (!hero) {
    return (
      <main className="center-state">
        <p>Your Jellyfin library is connected, but it does not contain any movies or series yet.</p>
        <button className="secondary-button" type="button" onClick={onSignOut}>Sign out</button>
      </main>
    );
  }

  const featured = feed.topRated.find((item) => item.BackdropImageTags?.length) ?? feed.topRated[0] ?? hero;
  const featuredItems = mergeUnique([featured], feed.topRated, feed.latest).slice(0, 10);
  const topMovies = mergeUnique(
    feed.topRated.filter((item) => item.Type === 'Movie'),
    feed.movies,
  );
  const topSeries = mergeUnique(
    feed.topRated.filter((item) => item.Type === 'Series'),
    feed.series,
  );
  const spotlight = topMovies.find((item) => item.Id !== featured.Id) ?? topSeries[0] ?? featured;

  return (
    <main className="home-shell">
      <AppNav session={session} activeView="home" calendarEnabled={calendarEnabled} onNavigate={onNavigate} onSearch={onSearch} onSignOut={onSignOut} />

      <section
        className="hero"
        id="home-top"
      >
        <div
          className={`hero-backdrop${rotatesHero ? ' hero-backdrop--rotating' : ''}`}
          key={hero.Id}
          style={{ backgroundImage: `url("${backdropUrl(session, hero, 1800)}")` }}
        />
        <div className="hero-scrim" />
        <div
          className={`hero-content${rotatesHero ? ' hero-content--rotating' : ''}`}
          key={`${hero.Id}-content`}
        >
          <p className="eyebrow">{hero.UserData?.PlayedPercentage ? 'Continue watching' : 'Tonight’s feature'}</p>
          <h1>{hero.Name}</h1>
          <p className="hero-meta">
            {[hero.ProductionYear, hero.OfficialRating, runtime(hero.RunTimeTicks), hero.Genres?.[0]]
              .filter(Boolean)
              .join(' · ')}
          </p>
          <p className="hero-overview">{hero.Overview || 'Your next story is ready when you are.'}</p>
          <div className="hero-actions">
            <button className="primary-button" type="button" onClick={() => onPlay(hero)}>
              <MaterialIcon name="play_arrow" filled /> {hero.UserData?.PlayedPercentage ? 'Resume' : 'Play now'}
            </button>
            <button className="secondary-button" type="button" onClick={() => onSelect(hero)}>
              <MaterialIcon name="info" /> More information
            </button>
          </div>
        </div>
      </section>

      <div className="home-content">
        <LibraryShortcuts onNavigate={onNavigate} />
        <MediaRow title="Continue watching" items={feed.resume} session={session} onSelect={onSelect} sectionId="continue" />
        <MediaRow title="Recently added" items={feed.latest} session={session} onSelect={onSelect} sectionId="recent" />
        <ComingSoonShelf events={upcoming} onViewCalendar={() => onNavigate('calendar')} />
        <RankedShelf items={feed.topRated} session={session} onSelect={onSelect} />
        <FeaturedShowcase
          items={featuredItems}
          session={session}
          onSelect={onSelect}
          onPlay={onPlay}
        />
        <MediaRow
          title="Movies"
          items={feed.movies}
          session={session}
          onSelect={onSelect}
          sectionId="movies"
          browseAllLabel="View all movies"
          onBrowseAll={() => onNavigate('movies')}
        />
        <MediaRow
          title="Series"
          items={feed.series}
          session={session}
          onSelect={onSelect}
          sectionId="series"
          browseAllLabel="View all series"
          onBrowseAll={() => onNavigate('series')}
        />
        <MediaRow title="My favorites" items={feed.favorites} session={session} onSelect={onSelect} sectionId="favorites" />
        <CollectionSpotlight
          feature={spotlight}
          movies={topMovies.filter((item) => item.Id !== spotlight.Id)}
          series={topSeries.filter((item) => item.Id !== spotlight.Id)}
          session={session}
          onSelect={onSelect}
          onPlay={onPlay}
        />
        <HomeFooter onNavigate={onNavigate} />
      </div>
    </main>
  );
}
