import { useLayoutEffect, useRef, useState } from 'react';
import {
  backdropUrl,
  type JellyfinItem,
  type JellyfinSession,
} from '../lib/jellyfin';
import Artwork from './Artwork';
import type { AppView } from './AppNav';
import MaterialIcon from './MaterialIcon';
import MediaCard from './MediaCard';

type HomeActionProps = {
  session: JellyfinSession;
  onSelect: (item: JellyfinItem) => void;
  onPlay: (item: JellyfinItem) => void;
};

type LibraryShortcutsProps = {
  onNavigate: (view: AppView) => void;
};

const shortcutItems: Array<{
  label: string;
  detail: string;
  action: AppView | 'continue' | 'recent' | 'top-rated';
}> = [
  { label: 'Continue', detail: 'Pick up where you left off', action: 'continue' },
  { label: 'Just added', detail: 'Fresh from your server', action: 'recent' },
  { label: 'Movies', detail: 'Browse the full collection', action: 'movies' },
  { label: 'Series', detail: 'Every season and episode', action: 'series' },
  { label: 'Top rated', detail: 'The best in your library', action: 'top-rated' },
];

function scrollToSection(id: string) {
  document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
}

export function LibraryShortcuts({ onNavigate }: LibraryShortcutsProps) {
  return (
    <nav className="library-shortcuts" aria-label="Browse your library">
      {shortcutItems.map((shortcut) => (
        <button
          type="button"
          key={shortcut.label}
          onClick={() => (
            shortcut.action === 'movies' || shortcut.action === 'series'
              ? onNavigate(shortcut.action)
              : scrollToSection(shortcut.action)
          )}
        >
          <span>
            <strong>{shortcut.label}</strong>
            <small>{shortcut.detail}</small>
          </span>
        </button>
      ))}
    </nav>
  );
}

type RankedShelfProps = Pick<HomeActionProps, 'session' | 'onSelect'> & {
  items: JellyfinItem[];
};

export function RankedShelf({ items, session, onSelect }: RankedShelfProps) {
  if (items.length === 0) return null;
  return (
    <section className="ranked-section media-section" id="top-rated">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Your collection</p>
          <h2>Top rated</h2>
        </div>
        <span>Based on Jellyfin ratings</span>
      </div>
      <div className="ranked-row">
        {items.slice(0, 10).map((item, index) => (
          <div className="ranked-item" key={item.Id}>
            <span className="ranked-item__number" aria-hidden="true">{index + 1}</span>
            <MediaCard item={item} session={session} onSelect={onSelect} />
          </div>
        ))}
      </div>
    </section>
  );
}

type FeaturedShowcaseProps = HomeActionProps & {
  items: JellyfinItem[];
};

export function FeaturedShowcase({ items, session, onSelect, onPlay }: FeaturedShowcaseProps) {
  const [activeIndex, setActiveIndex] = useState(0);
  const [previousIndex, setPreviousIndex] = useState<number | null>(null);
  const [slideIndex, setSlideIndex] = useState(() => items.length + 1);
  const [cardStep, setCardStep] = useState(0);
  const [sliding, setSliding] = useState(false);
  const cardsViewportRef = useRef<HTMLDivElement>(null);
  const feature = items.length ? items[activeIndex % items.length] : undefined;
  const previousFeature = previousIndex === null || items.length === 0
    ? null
    : items[previousIndex % items.length];
  const carouselItems = [...items, ...items, ...items];

  useLayoutEffect(() => {
    const viewport = cardsViewportRef.current;
    if (!viewport) return;
    const measure = () => {
      const card = viewport.querySelector<HTMLElement>('.media-card');
      if (card) setCardStep(card.getBoundingClientRect().width + 14);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(viewport);
    return () => observer.disconnect();
  }, [items.length]);

  if (!feature) return null;

  function renderFeatureCopy(item: JellyfinItem, className: string, interactive: boolean) {
    return (
      <div className={className} key={`${item.Id}-${interactive ? 'copy-in' : 'copy-out'}`} aria-hidden={interactive ? undefined : true}>
        <p className="eyebrow">Featured in Matinee</p>
        <p className="featured-showcase__kicker">A standout from your own collection</p>
        <h2>{item.Name}</h2>
        <p className="hero-meta">
          {[item.ProductionYear, item.OfficialRating, item.Genres?.slice(0, 2).join(' / ')]
            .filter(Boolean)
            .join(' · ')}
        </p>
        {item.CommunityRating ? <p className="rating">★ {item.CommunityRating.toFixed(1)}</p> : null}
        <p>{item.Overview || 'Ready to watch from your Jellyfin library.'}</p>
        <div className="hero-actions">
          <button className="primary-button" type="button" onClick={() => onPlay(item)} tabIndex={interactive ? undefined : -1}>
            <MaterialIcon name="play_arrow" filled /> Play now
          </button>
          <button className="secondary-button" type="button" onClick={() => onSelect(item)} tabIndex={interactive ? undefined : -1}>
            <MaterialIcon name="info" /> Details
          </button>
        </div>
      </div>
    );
  }

  function navigate(direction: -1 | 1) {
    setPreviousIndex(activeIndex);
    setActiveIndex((activeIndex + direction + items.length) % items.length);
    setSliding(true);
    setSlideIndex((current) => current + direction);
  }

  function finishSlide() {
    let normalizedIndex = slideIndex;
    if (slideIndex < items.length) normalizedIndex += items.length;
    if (slideIndex >= items.length * 2) normalizedIndex -= items.length;
    if (normalizedIndex === slideIndex) return;
    setSliding(false);
    setSlideIndex(normalizedIndex);
  }

  return (
    <section className="featured-showcase">
      {previousFeature && previousFeature.Id !== feature.Id ? (
        <div
          className="featured-showcase__backdrop featured-showcase__backdrop--outgoing"
          key={`${previousFeature.Id}-outgoing`}
          style={{ backgroundImage: `url("${backdropUrl(session, previousFeature, 1800)}")` }}
        />
      ) : null}
      <div
        className="featured-showcase__backdrop featured-showcase__backdrop--incoming"
        key={`${feature.Id}-incoming`}
        style={{ backgroundImage: `url("${backdropUrl(session, feature, 1800)}")` }}
      />
      <div className="featured-showcase__scrim" />
      {items.length > 1 ? (
        <button
          className="featured-showcase__arrow featured-showcase__arrow--previous"
          type="button"
          aria-label="Previous featured title"
          onClick={() => navigate(-1)}
        >
          <MaterialIcon name="arrow_back" />
        </button>
      ) : null}
      <div className="featured-showcase__copy-stack">
        {previousFeature && previousFeature.Id !== feature.Id
          ? renderFeatureCopy(previousFeature, 'featured-showcase__copy featured-showcase__copy--outgoing', false)
          : null}
        {renderFeatureCopy(feature, 'featured-showcase__copy featured-showcase__copy--enter', true)}
      </div>
      {items.length > 1 ? (
        <div className="featured-showcase__cards" ref={cardsViewportRef} aria-label="More featured titles">
          <div
            className="featured-showcase__cards-track"
            style={{
              transform: `translate3d(${-slideIndex * cardStep}px, 0, 0)`,
              transition: sliding ? 'transform 620ms cubic-bezier(0.22, 1, 0.36, 1)' : 'none',
            }}
            onTransitionEnd={(event) => {
              if (event.target === event.currentTarget) finishSlide();
            }}
          >
            {carouselItems.map((item, index) => (
              <MediaCard key={`${Math.floor(index / items.length)}-${item.Id}`} item={item} session={session} onSelect={onSelect} />
            ))}
          </div>
        </div>
      ) : null}
      {items.length > 1 ? (
        <button
          className="featured-showcase__arrow featured-showcase__arrow--next"
          type="button"
          aria-label="Next featured title"
          onClick={() => navigate(1)}
        >
          <MaterialIcon name="arrow_back" />
        </button>
      ) : null}
    </section>
  );
}

type CollectionSpotlightProps = HomeActionProps & {
  feature: JellyfinItem;
  movies: JellyfinItem[];
  series: JellyfinItem[];
};

function CompactList({
  title,
  items,
  session,
  onSelect,
}: Pick<HomeActionProps, 'session' | 'onSelect'> & { title: string; items: JellyfinItem[] }) {
  if (items.length === 0) return null;
  return (
    <section className="compact-list">
      <h3>{title}</h3>
      {items.slice(0, 4).map((item) => (
        <button type="button" key={item.Id} onClick={() => onSelect(item)}>
          <Artwork item={item} session={session} width={180} />
          <span>
            <strong>{item.Name}</strong>
            <small>{[item.ProductionYear, item.Genres?.[0]].filter(Boolean).join(' · ')}</small>
            {item.CommunityRating ? <em>★ {item.CommunityRating.toFixed(1)}</em> : null}
          </span>
        </button>
      ))}
    </section>
  );
}

export function CollectionSpotlight({ feature, movies, series, session, onSelect, onPlay }: CollectionSpotlightProps) {
  return (
    <section className="collection-spotlight">
      <div className="collection-spotlight__feature">
        <div
          className="collection-spotlight__art"
          style={{ backgroundImage: `url("${backdropUrl(session, feature, 1400)}")` }}
        >
          <div className="collection-spotlight__scrim" />
        </div>
        <div className="collection-spotlight__copy">
          <p className="eyebrow">Tonight’s spotlight</p>
          <h2>{feature.Name}</h2>
          <p>{feature.Overview || 'A highlight from your personal collection.'}</p>
          <div className="hero-actions">
            <button className="primary-button" type="button" onClick={() => onPlay(feature)}>
              <MaterialIcon name="play_arrow" filled /> Play now
            </button>
            <button className="secondary-button" type="button" onClick={() => onSelect(feature)}>
              <MaterialIcon name="info" /> Details
            </button>
          </div>
        </div>
      </div>
      <CompactList title="Top movies" items={movies} session={session} onSelect={onSelect} />
      <CompactList title="Top series" items={series} session={session} onSelect={onSelect} />
    </section>
  );
}

export function HomeFooter({ onNavigate }: LibraryShortcutsProps) {
  return (
    <footer className="home-footer">
      <div className="home-footer__glow" aria-hidden="true" />
      <div className="home-footer__intro">
        <div className="home-footer__brand">
          <span className="home-footer__mark">M</span>
          <span>
            <strong>Matinee</strong>
            <small>Movie night starts here.</small>
          </span>
        </div>
        <h2>Good stories. Better together.</h2>
      </div>
      <nav aria-label="Footer navigation">
        <div>
          <p>Browse</p>
          <button type="button" onClick={() => scrollToSection('home-top')}>Home</button>
          <button type="button" onClick={() => onNavigate('movies')}>Movies</button>
          <button type="button" onClick={() => onNavigate('series')}>Series</button>
        </div>
        <div>
          <p>Your library</p>
          <button type="button" onClick={() => scrollToSection('continue')}>Continue watching</button>
          <button type="button" onClick={() => scrollToSection('recent')}>Recently added</button>
          <button type="button" onClick={() => scrollToSection('favorites')}>Favorites</button>
        </div>
        <div>
          <p>Matinee</p>
          <button type="button" onClick={() => onNavigate('settings')}>Settings</button>
          <button type="button" onClick={() => scrollToSection('home-top')}>Back to top</button>
        </div>
      </nav>
    </footer>
  );
}
