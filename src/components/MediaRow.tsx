import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import type { JellyfinItem, JellyfinSession } from '../lib/jellyfin';
import MaterialIcon from './MaterialIcon';
import MediaCard from './MediaCard';

type MediaRowProps = {
  title: string;
  items: JellyfinItem[];
  session: JellyfinSession;
  landscape?: boolean;
  onSelect: (item: JellyfinItem) => void;
  sectionId?: string;
  browseAllLabel?: string;
  onBrowseAll?: () => void;
};

export default function MediaRow({
  title,
  items,
  session,
  landscape = false,
  onSelect,
  sectionId,
  browseAllLabel,
  onBrowseAll,
}: MediaRowProps) {
  const rowRef = useRef<HTMLDivElement>(null);
  const [canScrollBack, setCanScrollBack] = useState(false);
  const [canScrollForward, setCanScrollForward] = useState(false);
  const hasBrowseAll = Boolean(browseAllLabel && onBrowseAll);
  const visibleItems = hasBrowseAll ? items.slice(0, 12) : items;

  const updateScrollState = useCallback(() => {
    const row = rowRef.current;
    if (!row) return;
    const remaining = row.scrollWidth - row.clientWidth - row.scrollLeft;
    setCanScrollBack(row.scrollLeft > 2);
    setCanScrollForward(remaining > 2);
  }, []);

  useLayoutEffect(() => {
    const row = rowRef.current;
    if (!row) return;
    updateScrollState();
    row.addEventListener('scroll', updateScrollState, { passive: true });
    const observer = new ResizeObserver(updateScrollState);
    observer.observe(row);
    return () => {
      row.removeEventListener('scroll', updateScrollState);
      observer.disconnect();
    };
  }, [items.length, hasBrowseAll, updateScrollState]);

  function scroll(direction: -1 | 1) {
    const row = rowRef.current;
    if (!row) return;
    const card = row.querySelector<HTMLElement>('.media-card');
    if (!card) return;
    const gap = Number.parseFloat(window.getComputedStyle(row).columnGap) || 0;
    row.scrollBy({ left: direction * (card.getBoundingClientRect().width + gap), behavior: 'smooth' });
  }

  if (items.length === 0) return null;
  return (
    <section className="media-section" id={sectionId}>
      <div className="section-heading">
        <h2>{title}</h2>
        <div className="section-heading__meta">
          <span>{visibleItems.length} titles</span>
          {hasBrowseAll ? (
            <div className="shelf-scroll-controls" aria-label={`${title} carousel controls`}>
              <button
                type="button"
                aria-label={`Scroll ${title} backward`}
                disabled={!canScrollBack}
                onClick={() => scroll(-1)}
              >
                <MaterialIcon name="arrow_back" />
              </button>
              <button
                className="shelf-scroll-control--forward"
                type="button"
                aria-label={`Scroll ${title} forward`}
                disabled={!canScrollForward}
                onClick={() => scroll(1)}
              >
                <MaterialIcon name="arrow_back" />
              </button>
            </div>
          ) : null}
        </div>
      </div>
      <div className="media-row" ref={rowRef}>
        {visibleItems.map((item) => (
          <MediaCard
            key={item.Id}
            item={item}
            session={session}
            landscape={landscape}
            onSelect={onSelect}
          />
        ))}
        {hasBrowseAll ? (
          <button
            className="media-card media-row__browse-all"
            type="button"
            aria-label={browseAllLabel}
            onClick={onBrowseAll}
          >
            <span className="media-card__art">
              <span className="media-row__browse-all-icon" aria-hidden="true">
                <MaterialIcon name="arrow_back" />
              </span>
              <strong>{browseAllLabel}</strong>
              <small>Full library</small>
            </span>
          </button>
        ) : null}
      </div>
    </section>
  );
}
