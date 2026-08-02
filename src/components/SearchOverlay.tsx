import { useEffect, useRef, useState } from 'react';
import { searchLibrary, type JellyfinItem, type JellyfinSession } from '../lib/jellyfin';
import Artwork from './Artwork';
import MaterialIcon from './MaterialIcon';

type SearchOverlayProps = {
  session: JellyfinSession;
  onClose: () => void;
  onSelect: (item: JellyfinItem) => void;
};

export default function SearchOverlay({ session, onClose, onSelect }: SearchOverlayProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<JellyfinItem[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    inputRef.current?.focus();
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [onClose]);

  useEffect(() => {
    const trimmed = query.trim();
    if (trimmed.length < 2) {
      setResults([]);
      setLoading(false);
      return;
    }
    const controller = new AbortController();
    const delay = window.setTimeout(() => {
      setLoading(true);
      searchLibrary(session, trimmed, controller.signal)
        .then(setResults)
        .catch(() => {
          if (!controller.signal.aborted) setResults([]);
        })
        .finally(() => {
          if (!controller.signal.aborted) setLoading(false);
        });
    }, 220);
    return () => {
      window.clearTimeout(delay);
      controller.abort();
    };
  }, [query, session]);

  function choose(item: JellyfinItem) {
    onSelect(item);
    onClose();
  }

  return (
    <div className="search-overlay" role="dialog" aria-modal="true" aria-label="Search Jellyfin library" onMouseDown={onClose}>
      <section className="search-panel" onMouseDown={(event) => event.stopPropagation()}>
        <div className="search-input-wrap">
          <MaterialIcon name="search" />
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search movies, series, and episodes"
            aria-label="Search library"
          />
          <button type="button" onClick={onClose}>Esc</button>
        </div>
        <div className="search-results">
          {loading ? <p className="loading-state">Searching Jellyfin…</p> : null}
          {!loading && query.trim().length >= 2 && results.length === 0 ? <p className="loading-state">No matching titles.</p> : null}
          {results.map((item) => (
            <button type="button" className="search-result" key={item.Id} onClick={() => choose(item)}>
              <Artwork item={item} session={session} width={160} />
              <span>
                <strong>{item.Name}</strong>
                <small>{[item.SeriesName, item.Type, item.ProductionYear].filter(Boolean).join(' · ')}</small>
              </span>
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}
