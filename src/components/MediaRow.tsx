import type { JellyfinItem, JellyfinSession } from '../lib/jellyfin';
import MediaCard from './MediaCard';

type MediaRowProps = {
  title: string;
  items: JellyfinItem[];
  session: JellyfinSession;
  landscape?: boolean;
  onSelect: (item: JellyfinItem) => void;
  sectionId?: string;
};

export default function MediaRow({
  title,
  items,
  session,
  landscape = false,
  onSelect,
  sectionId,
}: MediaRowProps) {
  if (items.length === 0) return null;
  return (
    <section className="media-section" id={sectionId}>
      <div className="section-heading">
        <h2>{title}</h2>
        <span>{items.length} titles</span>
      </div>
      <div className="media-row">
        {items.map((item) => (
          <MediaCard
            key={item.Id}
            item={item}
            session={session}
            landscape={landscape}
            onSelect={onSelect}
          />
        ))}
      </div>
    </section>
  );
}
