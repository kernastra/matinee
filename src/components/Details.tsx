import { useEffect, useMemo, useRef, useState } from 'react';
import {
  backdropUrl,
  clearItemProgress,
  getItemCollectionContext,
  getItemDetails,
  getSimilarItems,
  imageUrl,
  setItemFavorite,
  setItemPlayed,
  type JellyfinCollectionContext,
  type JellyfinItem,
  type JellyfinSession,
} from '../lib/jellyfin';
import DetailsModal, { type DetailsModalMode } from './DetailsModal';
import DetailsSupplemental from './DetailsSupplemental';
import MaterialIcon from './MaterialIcon';

type DetailsProps = {
  item: JellyfinItem;
  session: JellyfinSession;
  onBack: () => void;
  onPlay: (item: JellyfinItem) => void;
  onSelect: (item: JellyfinItem) => void;
};

function runtime(ticks?: number) {
  if (!ticks) return null;
  const minutes = Math.round(ticks / 600_000_000);
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

function resumeTime(ticks?: number) {
  if (!ticks) return null;
  const minutes = Math.floor(ticks / 600_000_000);
  if (minutes < 1) return null;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

function videoQuality(item: JellyfinItem) {
  const video = item.MediaStreams?.find((stream) => stream.Type === 'Video');
  if (!video) return null;
  const resolution = video.Height
    ? video.Height >= 2160
      ? '4K'
      : `${video.Height}p`
    : null;
  const range = video.VideoRange && video.VideoRange !== 'SDR' ? video.VideoRange : null;
  return [resolution, range].filter(Boolean).join(' ') || video.Codec?.toUpperCase() || null;
}

function audioCodec(item: JellyfinItem) {
  const audio = item.MediaStreams?.find((stream) => stream.Type === 'Audio');
  if (!audio) return null;
  const codec = audio.Codec?.toUpperCase();
  const channelMap: Record<number, string> = {
    1: 'Mono',
    2: 'Stereo',
    6: '5.1',
    8: '7.1',
  };
  const channels = audio.Channels ? channelMap[audio.Channels] ?? `${audio.Channels}ch` : null;
  return [codec, channels].filter(Boolean).join(' ') || audio.DisplayTitle || null;
}

function peopleByType(item: JellyfinItem, type: string) {
  return item.People?.filter((person) => person.Type === type) ?? [];
}

function personImageUrl(session: JellyfinSession, personId?: string) {
  if (!personId) return null;
  return `${session.serverUrl}/Items/${personId}/Images/Primary?maxWidth=260&quality=88&api_key=${encodeURIComponent(session.accessToken)}`;
}

export default function Details({ item, session, onBack, onPlay, onSelect }: DetailsProps) {
  const [details, setDetails] = useState(item);
  const [similarItems, setSimilarItems] = useState<JellyfinItem[]>([]);
  const [collections, setCollections] = useState<JellyfinCollectionContext[]>([]);
  const [loadingDetails, setLoadingDetails] = useState(false);
  const [busyAction, setBusyAction] = useState<'favorite' | 'played' | 'progress' | null>(null);
  const [modal, setModal] = useState<DetailsModalMode | null>(null);
  const [actionStatus, setActionStatus] = useState('');
  const shellRef = useRef<HTMLElement>(null);

  useEffect(() => {
    let cancelled = false;
    setDetails(item);
    setSimilarItems([]);
    setCollections([]);
    setModal(null);
    setActionStatus('');
    shellRef.current?.scrollTo({ top: 0 });
    setLoadingDetails(true);
    getItemDetails(session, item.Id)
      .then((fullItem) => {
        if (!cancelled) setDetails(fullItem);
      })
      .catch(() => {
        if (!cancelled) setDetails(item);
      })
      .finally(() => {
        if (!cancelled) setLoadingDetails(false);
      });

    getSimilarItems(session, item.Id)
      .then((relatedItems) => {
        if (!cancelled) setSimilarItems(relatedItems.filter((relatedItem) => relatedItem.Id !== item.Id));
      })
      .catch(() => {});

    getItemCollectionContext(session, item.Id)
      .then((collectionItems) => {
        if (!cancelled) setCollections(collectionItems);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [item, session]);

  useEffect(() => {
    if (!modal) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setModal(null);
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [modal]);

  const directors = useMemo(() => peopleByType(details, 'Director').map((person) => person.Name), [details]);
  const writers = useMemo(() => peopleByType(details, 'Writer').map((person) => person.Name), [details]);
  const cast = useMemo(() => peopleByType(details, 'Actor').slice(0, 6), [details]);
  const resumedAt = resumeTime(details.UserData?.PlaybackPositionTicks);
  const technicalTags = [
    ['Video', videoQuality(details)],
    ['Audio', audioCodec(details)],
  ].filter(([, value]) => Boolean(value));
  const studio = details.Studios?.[0]?.Name;
  const logo = details.ImageTags?.Logo ? imageUrl(session, details, 'Logo', 620) : null;
  const isFavorite = Boolean(details.UserData?.IsFavorite);
  const isPlayed = Boolean(details.UserData?.Played);

  async function toggleFavorite() {
    const nextFavorite = !isFavorite;
    setBusyAction('favorite');
    setDetails((current) => ({
      ...current,
      UserData: { ...current.UserData, IsFavorite: nextFavorite },
    }));
    try {
      await setItemFavorite(session, details.Id, nextFavorite);
      setActionStatus(nextFavorite ? 'Added to your watchlist.' : 'Removed from your watchlist.');
    } catch {
      setDetails((current) => ({
        ...current,
        UserData: { ...current.UserData, IsFavorite: isFavorite },
      }));
    } finally {
      setBusyAction(null);
    }
  }

  async function togglePlayed() {
    const nextPlayed = !isPlayed;
    setBusyAction('played');
    setDetails((current) => ({
      ...current,
      UserData: {
        ...current.UserData,
        Played: nextPlayed,
        PlayedPercentage: nextPlayed ? 100 : 0,
        PlaybackPositionTicks: nextPlayed ? 0 : current.UserData?.PlaybackPositionTicks,
      },
    }));
    try {
      await setItemPlayed(session, details.Id, nextPlayed);
      setActionStatus(nextPlayed ? 'Marked as played.' : 'Marked as unplayed.');
    } catch {
      setDetails((current) => ({
        ...current,
        UserData: {
          ...current.UserData,
          Played: isPlayed,
          PlayedPercentage: details.UserData?.PlayedPercentage,
          PlaybackPositionTicks: details.UserData?.PlaybackPositionTicks,
        },
      }));
    } finally {
      setBusyAction(null);
    }
  }

  async function clearProgress() {
    setBusyAction('progress');
    setActionStatus('');
    try {
      await clearItemProgress(session, details.Id);
      setDetails((current) => ({
        ...current,
        UserData: {
          ...current.UserData,
          PlaybackPositionTicks: 0,
          PlayedPercentage: 0,
        },
      }));
      setActionStatus('Removed from Continue Watching.');
    } catch {
      setActionStatus('Matinee could not clear the saved progress.');
    } finally {
      setBusyAction(null);
    }
  }

  function playFromBeginning() {
    setModal(null);
    onPlay({
      ...details,
      UserData: { ...details.UserData, PlaybackPositionTicks: 0 },
    });
  }

  async function copyTitle() {
    try {
      await navigator.clipboard.writeText(details.Name);
      setActionStatus('Title copied.');
    } catch {
      setActionStatus('Matinee could not copy the title.');
    }
  }

  return (
    <main
      className="details-shell"
      ref={shellRef}
      style={{ backgroundImage: `url("${backdropUrl(session, details, 1800)}")` }}
    >
      <div className="details-scrim" />
      <button className="back-button" type="button" onClick={onBack} aria-label="Back to home">
        <MaterialIcon name="arrow_left_alt" /> Back
      </button>
      <section className="details-content">
        <div className="details-copy">
          <div className="details-logo-lockup">
            {logo ? (
              <img className="details-logo" src={logo} alt={details.Name} />
            ) : (
              <h1>{details.Name}</h1>
            )}
          </div>
          {details.Taglines?.[0] ? <p className="details-tagline">{details.Taglines[0]}</p> : null}
          <p className="hero-meta">
            {[details.ProductionYear, details.OfficialRating, runtime(details.RunTimeTicks), details.Genres?.slice(0, 2).join(' / ')]
              .filter(Boolean)
              .join(' · ')}
          </p>
          {details.UserData?.PlayedPercentage && details.UserData.PlayedPercentage < 100 ? (
            <span className="details-progress" aria-label={`${Math.round(details.UserData.PlayedPercentage)} percent watched`}>
              <span style={{ width: `${Math.min(details.UserData.PlayedPercentage, 100)}%` }} />
            </span>
          ) : null}
          <p className="details-overview">{details.Overview || 'No overview is available for this title.'}</p>
          <div className="details-credits">
            {directors.length ? <p><span>Director</span>{directors.join(', ')}</p> : null}
            {writers.length ? <p><span>Writers</span>{writers.slice(0, 3).join(', ')}</p> : null}
          </div>
          {studio ? (
            <div className="details-genres details-studio-chip" aria-label="Studio">
              <span>Studio: {studio}</span>
            </div>
          ) : null}
        </div>
        <div className="details-actions-block">
          <div className="details-actions">
            <button className="details-action-tile details-action-tile--primary" type="button" onClick={() => onPlay(details)}>
              <span className="details-action-tile__box"><MaterialIcon name="play_arrow" filled /></span>
              <span className="details-action-tile__label">{details.UserData?.PlaybackPositionTicks ? 'Resume' : 'Play'}</span>
            </button>
            <button className="details-action-tile" type="button" disabled title="Trailer support is not available yet">
              <span className="details-action-tile__box"><MaterialIcon name="movie" /></span>
              <span className="details-action-tile__label">Trailer</span>
            </button>
            <button className="details-action-tile details-action-tile--watchlist" type="button" onClick={() => void toggleFavorite()} disabled={busyAction === 'favorite'}>
              <span className="details-action-tile__box"><MaterialIcon name={isFavorite ? 'bookmark_added' : 'bookmark'} /></span>
              <span className="details-action-tile__label">{isFavorite ? 'Watchlisted' : 'Add to Watchlist'}</span>
            </button>
            <button className="details-action-tile details-action-tile--played" type="button" onClick={() => void togglePlayed()} disabled={busyAction === 'played'}>
              <span className="details-action-tile__box"><MaterialIcon name="check_circle" filled={isPlayed} /></span>
              <span className="details-action-tile__label">{isPlayed ? 'Played' : 'Mark as Played'}</span>
            </button>
            <button className="details-action-tile" type="button" onClick={() => { setActionStatus(''); setModal('more'); }}>
              <span className="details-action-tile__box"><MaterialIcon name="more_horiz" /></span>
              <span className="details-action-tile__label">More</span>
            </button>
          </div>
          {resumedAt ? <p className="details-resume">Resume from {resumedAt}</p> : null}
        </div>
        <aside className="details-side-panel" aria-label="Movie details">
          <div className="details-score-row">
            {details.CommunityRating ? <span><strong>{Math.round(details.CommunityRating * 10)}%</strong> Audience</span> : null}
            {details.CriticRating ? <span><strong>{Math.round(details.CriticRating)}%</strong> Critics</span> : null}
          </div>
          <div className="details-technical-row">
            {isFavorite ? <span className="details-favorite">In Watchlist</span> : null}
            {technicalTags.length ? (
              <div className="details-genres" aria-label="Media details">
                {technicalTags.map(([label, value]) => (
                  <button type="button" key={label} onClick={() => setModal('media')}>
                    {label}: {value}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
          {loadingDetails ? <p className="details-loading">Loading Jellyfin details</p> : null}
        </aside>
      </section>
      <div className="details-lower">
        {cast.length ? (
          <section className="details-cast" aria-label="Cast">
            <h2>Cast</h2>
            <div className="details-cast-row">
              {cast.map((person) => {
                const portrait = personImageUrl(session, person.Id);
                return (
                  <div className="details-cast-card" key={`${person.Id ?? person.Name}-${person.Role ?? ''}`}>
                    <span className="details-cast-photo">
                      {portrait ? <img src={portrait} alt="" loading="lazy" /> : <span>{person.Name.slice(0, 1)}</span>}
                    </span>
                    <strong>{person.Name}</strong>
                    {person.Role ? <small>{person.Role}</small> : null}
                  </div>
                );
              })}
            </div>
          </section>
        ) : null}
        <DetailsSupplemental
          chapters={details.Chapters ?? []}
          collections={collections}
          item={details}
          session={session}
          similarItems={similarItems}
          onPlay={onPlay}
          onSelect={onSelect}
        />
      </div>
      {modal ? (
        <DetailsModal
          item={details}
          mode={modal}
          actionStatus={actionStatus}
          busy={busyAction !== null}
          onClearProgress={() => void clearProgress()}
          onClose={() => setModal(null)}
          onCopyTitle={() => void copyTitle()}
          onMediaInfo={() => { setActionStatus(''); setModal('media'); }}
          onPlayFromBeginning={playFromBeginning}
          onToggleFavorite={() => void toggleFavorite()}
          onTogglePlayed={() => void togglePlayed()}
        />
      ) : null}
    </main>
  );
}
