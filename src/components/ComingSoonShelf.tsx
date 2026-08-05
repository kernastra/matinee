import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import { releaseDateLabel, type UpcomingRelease } from '../lib/integrations';
import MaterialIcon from './MaterialIcon';

type Props = {
  events: UpcomingRelease[];
  onViewCalendar: () => void;
};

export default function ComingSoonShelf({ events, onViewCalendar }: Props) {
  const rowRef = useRef<HTMLDivElement>(null);
  const [canScrollBack, setCanScrollBack] = useState(false);
  const [canScrollForward, setCanScrollForward] = useState(false);

  const updateScrollState = useCallback(() => {
    const row = rowRef.current;
    if (!row) return;
    setCanScrollBack(row.scrollLeft > 2);
    setCanScrollForward(row.scrollWidth - row.clientWidth - row.scrollLeft > 2);
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
  }, [events.length, updateScrollState]);

  function scroll(direction: -1 | 1) {
    const row = rowRef.current;
    const card = row?.querySelector<HTMLElement>('.coming-soon-card');
    if (!row || !card) return;
    const gap = Number.parseFloat(window.getComputedStyle(row).columnGap) || 0;
    row.scrollBy({ left: direction * (card.getBoundingClientRect().width + gap), behavior: 'smooth' });
  }

  if (events.length === 0) return null;
  return (
    <section className="media-section coming-soon-section" id="coming-soon">
      <div className="section-heading">
        <div>
          <p className="eyebrow">From your monitored list</p>
          <h2>Coming soon</h2>
        </div>
        <div className="section-heading__meta">
          <span>{events.length} upcoming</span>
          <div className="shelf-scroll-controls" aria-label="Coming Soon carousel controls">
            <button type="button" aria-label="Scroll Coming Soon backward" disabled={!canScrollBack} onClick={() => scroll(-1)}>
              <MaterialIcon name="arrow_back" />
            </button>
            <button className="shelf-scroll-control--forward" type="button" aria-label="Scroll Coming Soon forward" disabled={!canScrollForward} onClick={() => scroll(1)}>
              <MaterialIcon name="arrow_back" />
            </button>
          </div>
        </div>
      </div>
      <div className="coming-soon-row" ref={rowRef}>
        {events.map((event) => (
          <button
            className="coming-soon-card"
            type="button"
            key={event.id}
            aria-label={`${event.title}, ${releaseDateLabel(event.date)}`}
            onClick={onViewCalendar}
          >
            <span className="coming-soon-card__art">
              {event.imageUrl ? <img src={event.imageUrl} alt="" /> : (
                <span className="coming-soon-card__fallback"><MaterialIcon name={event.source === 'radarr' ? 'movie' : 'tv'} /></span>
              )}
              <span className={`coming-soon-card__source coming-soon-card__source--${event.source}`}>
                {event.source === 'radarr' ? 'Movie' : 'Series'}
              </span>
              <span className="coming-soon-card__copy">
                <strong>{event.title}</strong>
                {event.subtitle ? <small>{event.subtitle}</small> : null}
                <em>{releaseDateLabel(event.date)} · {event.releaseKind}</em>
              </span>
            </span>
          </button>
        ))}
        <button className="coming-soon-card coming-soon-card--calendar" type="button" onClick={onViewCalendar}>
          <span className="coming-soon-card__art">
            <span className="coming-soon-card__calendar-mark">{new Date().getDate()}</span>
            <strong>View full calendar</strong>
            <small>Every monitored release</small>
          </span>
        </button>
      </div>
    </section>
  );
}
