import { useEffect, useState } from 'react';
import { getLibraryItems, type JellyfinItem, type JellyfinSession } from '../lib/jellyfin';
import AppNav, { type AppView } from './AppNav';
import MediaCard from './MediaCard';

type LibraryProps = {
  type: 'Movie' | 'Series';
  session: JellyfinSession;
  onNavigate: (view: AppView) => void;
  onSearch: () => void;
  onSignOut: () => void;
  onSelect: (item: JellyfinItem) => void;
};

export default function Library({ type, session, onNavigate, onSearch, onSignOut, onSelect }: LibraryProps) {
  const [items, setItems] = useState<JellyfinItem[]>([]);
  const [sortBy, setSortBy] = useState('SortName');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError('');
    getLibraryItems(session, type, sortBy)
      .then((result) => {
        if (!cancelled) setItems(result);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : 'Could not load this library.');
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [session, sortBy, type]);

  const view: AppView = type === 'Movie' ? 'movies' : 'series';
  return (
    <main className="library-shell">
      <AppNav session={session} activeView={view} onNavigate={onNavigate} onSearch={onSearch} onSignOut={onSignOut} />
      <header className="library-header">
        <div>
          <p className="eyebrow">Your library</p>
          <h1>{type === 'Movie' ? 'Movies' : 'Series'}</h1>
          <p>{items.length} titles from Jellyfin</p>
        </div>
        <label>
          Sort by
          <select value={sortBy} onChange={(event) => setSortBy(event.target.value)}>
            <option value="SortName">Title</option>
            <option value="DateCreated">Recently added</option>
            <option value="ProductionYear">Release year</option>
            <option value="CommunityRating">Rating</option>
          </select>
        </label>
      </header>
      {loading ? <div className="library-state">Loading {view}…</div> : null}
      {error ? <div className="library-state form-error">{error}</div> : null}
      {!loading && !error ? (
        items.length ? (
          <section className="library-grid">
            {items.map((item) => <MediaCard key={item.Id} item={item} session={session} onSelect={onSelect} />)}
          </section>
        ) : <div className="library-state">No {view} found in this Jellyfin library.</div>
      ) : null}
    </main>
  );
}
