import { useState } from 'react';
import {
  chapterImageUrl,
  type ChapterInfo,
  type JellyfinCollectionContext,
  type JellyfinItem,
  type JellyfinSession,
} from '../lib/jellyfin';
import MediaCard from './MediaCard';

type DetailsSupplementalProps = {
  chapters: ChapterInfo[];
  collections: JellyfinCollectionContext[];
  item: JellyfinItem;
  session: JellyfinSession;
  similarItems: JellyfinItem[];
  onPlay: (item: JellyfinItem) => void;
  onSelect: (item: JellyfinItem) => void;
};

function chapterTime(ticks = 0) {
  const seconds = Math.floor(ticks / 10_000_000);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainingSeconds = seconds % 60;
  return hours
    ? `${hours}:${minutes.toString().padStart(2, '0')}:${remainingSeconds.toString().padStart(2, '0')}`
    : `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
}

function ChapterArtwork({
  chapter,
  chapterIndex,
  itemId,
  session,
}: {
  chapter: ChapterInfo;
  chapterIndex: number;
  itemId: string;
  session: JellyfinSession;
}) {
  const [failed, setFailed] = useState(false);

  if (!chapter.ImageTag || failed) {
    return <span className="details-chapter-card__unavailable">Preview not generated</span>;
  }

  return (
    <img
      src={chapterImageUrl(session, itemId, chapterIndex, 420, chapter.ImageTag)}
      alt=""
      loading="lazy"
      onError={() => setFailed(true)}
    />
  );
}

export default function DetailsSupplemental({
  chapters,
  collections,
  item,
  session,
  similarItems,
  onPlay,
  onSelect,
}: DetailsSupplementalProps) {
  const hasChapterImages = chapters.some((chapter) => Boolean(chapter.ImageTag));
  const usefulCollections = collections
    .map((context) => ({
      ...context,
      items: context.items.filter((collectionItem) => collectionItem.Id !== item.Id),
    }))
    .filter((context) => context.items.length > 0);

  if (!chapters.length && !usefulCollections.length && !similarItems.length) return null;

  return (
    <div className="details-supplemental">
      {chapters.length ? (
        <section className="details-shelf details-chapters" aria-labelledby="details-chapters-title">
          <div className="details-shelf__heading">
            <div>
              <p className="eyebrow">Jump back in</p>
              <h2 id="details-chapters-title">Scenes &amp; chapters</h2>
            </div>
            <span>{hasChapterImages ? `${chapters.length} chapters` : 'Previews not generated in Jellyfin'}</span>
          </div>
          <div className="details-chapter-row">
            {chapters.slice(0, 16).map((chapter, index) => (
              <button
                className="details-chapter-card"
                type="button"
                key={`${chapter.StartPositionTicks ?? index}-${chapter.Name ?? ''}`}
                onClick={() => onPlay({
                  ...item,
                  UserData: {
                    ...item.UserData,
                    PlaybackPositionTicks: chapter.StartPositionTicks ?? 0,
                  },
                })}
              >
                <span className="details-chapter-card__art">
                  <ChapterArtwork
                    chapter={chapter}
                    chapterIndex={index}
                    itemId={item.Id}
                    session={session}
                  />
                  <span>{chapterTime(chapter.StartPositionTicks)}</span>
                </span>
                <strong>{chapter.Name || `Chapter ${index + 1}`}</strong>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {usefulCollections.map(({ collection, items }) => (
        <section className="details-shelf details-collection" key={collection.Id} aria-labelledby={`collection-${collection.Id}`}>
          <div className="details-shelf__heading">
            <div>
              <p className="eyebrow">Part of a collection</p>
              <h2 id={`collection-${collection.Id}`}>{collection.Name}</h2>
            </div>
            <span>{items.length + 1} titles</span>
          </div>
          <div className="details-media-row">
            {items.slice(0, 10).map((collectionItem) => (
              <MediaCard
                item={collectionItem}
                session={session}
                onSelect={onSelect}
                key={collectionItem.Id}
              />
            ))}
          </div>
        </section>
      ))}

      {similarItems.length ? (
        <section className="details-shelf" aria-labelledby="similar-items-title">
          <div className="details-shelf__heading">
            <div>
              <p className="eyebrow">From your library</p>
              <h2 id="similar-items-title">More like this</h2>
            </div>
          </div>
          <div className="details-media-row">
            {similarItems.slice(0, 12).map((similarItem) => (
              <MediaCard
                item={similarItem}
                session={session}
                onSelect={onSelect}
                key={similarItem.Id}
              />
            ))}
          </div>
        </section>
      ) : null}
    </div>
  );
}
