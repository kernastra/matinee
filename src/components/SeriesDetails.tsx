import { useEffect, useState } from 'react';
import {
  getNextUpEpisode,
  getSeasonEpisodes,
  getSeriesSeasons,
  backdropUrl,
  type JellyfinItem,
  type JellyfinSession,
} from '../lib/jellyfin';
import Artwork from './Artwork';
import MaterialIcon from './MaterialIcon';

type SeriesDetailsProps = {
  item: JellyfinItem;
  session: JellyfinSession;
  onBack: () => void;
  onPlay: (episode: JellyfinItem) => void;
};

function episodeNumber(episode: JellyfinItem) {
  const season = episode.ParentIndexNumber?.toString().padStart(2, '0');
  const number = episode.IndexNumber?.toString().padStart(2, '0');
  return season && number ? `S${season} E${number}` : 'Episode';
}

export default function SeriesDetails({ item, session, onBack, onPlay }: SeriesDetailsProps) {
  const [seasons, setSeasons] = useState<JellyfinItem[]>([]);
  const [selectedSeasonId, setSelectedSeasonId] = useState('');
  const [episodes, setEpisodes] = useState<JellyfinItem[]>([]);
  const [nextUp, setNextUp] = useState<JellyfinItem | null>(null);
  const [loading, setLoading] = useState(true);
  const [episodesLoading, setEpisodesLoading] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    Promise.all([getSeriesSeasons(session, item.Id), getNextUpEpisode(session, item.Id)])
      .then(([seasonItems, nextEpisode]) => {
        if (cancelled) return;
        setSeasons(seasonItems);
        setNextUp(nextEpisode);
        setSelectedSeasonId(nextEpisode?.SeasonId || seasonItems[0]?.Id || '');
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : 'Could not load this series.');
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [item.Id, session]);

  useEffect(() => {
    if (!selectedSeasonId) return;
    let cancelled = false;
    setError('');
    setEpisodesLoading(true);
    setEpisodes([]);
    getSeasonEpisodes(session, item.Id, selectedSeasonId)
      .then((episodeItems) => {
        if (!cancelled) setEpisodes(episodeItems);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : 'Could not load episodes.');
      })
      .finally(() => {
        if (!cancelled) setEpisodesLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [item.Id, selectedSeasonId, session]);

  const backdrop = backdropUrl(session, item, 1800);

  return (
    <main className="series-shell">
      <div className="series-backdrop" style={{ backgroundImage: `url("${backdrop}")` }} />
      <div className="details-scrim" />
      <button className="back-button" type="button" onClick={onBack}>
        <MaterialIcon name="arrow_left_alt" /> Back
      </button>
      <div className="series-scroll">
        <section className="series-overview">
          <Artwork className="series-poster" item={item} session={session} width={420} loading="eager" />
          <div className="series-copy">
            <p className="eyebrow">Series</p>
            <h1>{item.Name}</h1>
            <p className="hero-meta">
              {[item.ProductionYear, item.OfficialRating, item.Genres?.slice(0, 2).join(' / ')]
                .filter(Boolean)
                .join(' · ')}
            </p>
            <p className="details-overview">{item.Overview || 'No overview is available for this series.'}</p>
            {nextUp ? (
              <button className="primary-button details-play" type="button" onClick={() => onPlay(nextUp)}>
                <MaterialIcon name="play_arrow" filled />
                {nextUp.UserData?.PlaybackPositionTicks ? 'Resume' : 'Play next'} · {episodeNumber(nextUp)}
              </button>
            ) : null}
          </div>
        </section>

        <section className="episode-browser">
          <div className="season-tabs" aria-label="Seasons">
            {seasons.map((season) => (
              <button
                className={season.Id === selectedSeasonId ? 'active' : ''}
                type="button"
                key={season.Id}
                onClick={() => setSelectedSeasonId(season.Id)}
              >
                {season.Name}
              </button>
            ))}
          </div>
          {error ? <p className="form-error">{error}</p> : null}
          {loading ? <p className="loading-state">Loading seasons…</p> : null}
          {!loading && !error && seasons.length === 0 ? (
            <p className="loading-state">No seasons are available for this series.</p>
          ) : null}
          {episodesLoading ? <p className="loading-state">Loading episodes…</p> : null}
          {!loading && !episodesLoading && !error && selectedSeasonId && episodes.length === 0 ? (
            <p className="loading-state">No episodes are available in this season.</p>
          ) : null}
          <div className="episode-grid">
            {episodes.map((episode) => (
              <button className="episode-card" type="button" key={episode.Id} onClick={() => onPlay(episode)}>
                <span className="episode-card__art">
                  <Artwork item={episode} session={session} width={560} />
                  <span className="episode-card__play"><MaterialIcon name="play_arrow" filled /></span>
                  {episode.UserData?.PlayedPercentage ? (
                    <span className="media-card__progress">
                      <span style={{ width: `${Math.min(episode.UserData.PlayedPercentage, 100)}%` }} />
                    </span>
                  ) : null}
                </span>
                <span className="episode-card__copy">
                  <span>{episodeNumber(episode)}</span>
                  <strong>{episode.Name}</strong>
                  <small>{episode.Overview || 'No episode description available.'}</small>
                </span>
              </button>
            ))}
          </div>
        </section>
      </div>
    </main>
  );
}
