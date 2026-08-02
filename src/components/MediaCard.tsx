import { type JellyfinItem, type JellyfinSession } from '../lib/jellyfin';
import Artwork from './Artwork';
import MaterialIcon from './MaterialIcon';

type MediaCardProps = {
  item: JellyfinItem;
  session: JellyfinSession;
  landscape?: boolean;
  onSelect: (item: JellyfinItem) => void;
};

export default function MediaCard({ item, session, landscape = false, onSelect }: MediaCardProps) {
  const progress = item.UserData?.PlayedPercentage ?? 0;
  const rating = item.CommunityRating ? item.CommunityRating.toFixed(1) : null;
  const genre = item.Genres?.[0];
  return (
    <button
      className={`media-card ${landscape ? 'media-card--landscape' : ''}`}
      type="button"
      aria-label={item.Name}
      onClick={() => onSelect(item)}
    >
      <span className="media-card__art">
        <Artwork item={item} session={session} width={landscape ? 560 : 360} />
        <span className="media-card__play" aria-hidden="true"><MaterialIcon name="play_arrow" filled /></span>
        {rating || genre ? (
          <span className="media-card__overlay">
            {rating ? (
              <span className="media-card__rating">
                <span className="media-card__rating-symbol" aria-hidden="true">★</span> {rating}
              </span>
            ) : null}
            {rating && genre ? <span aria-hidden="true">·</span> : null}
            {genre ? <span className="media-card__genre">{genre}</span> : null}
          </span>
        ) : null}
        {progress > 0 ? (
          <span className="media-card__progress">
            <span style={{ width: `${Math.min(progress, 100)}%` }} />
          </span>
        ) : null}
      </span>
    </button>
  );
}
