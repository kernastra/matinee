import type { JellyfinSession } from '../lib/jellyfin';
import MaterialIcon from './MaterialIcon';

export type AppView = 'home' | 'movies' | 'series';

type AppNavProps = {
  session: JellyfinSession;
  activeView: AppView;
  onNavigate: (view: AppView) => void;
  onSearch: () => void;
  onSignOut: () => void;
};

export default function AppNav({ session, activeView, onNavigate, onSearch, onSignOut }: AppNavProps) {
  return (
    <nav className="main-nav" data-tauri-drag-region>
      <button className="brand" type="button" onClick={() => onNavigate('home')}>
        <span>M</span> Matinee
      </button>
      <div className="nav-links">
        <button className={activeView === 'home' ? 'active' : ''} type="button" onClick={() => onNavigate('home')}>Home</button>
        <button className={activeView === 'movies' ? 'active' : ''} type="button" onClick={() => onNavigate('movies')}>Movies</button>
        <button className={activeView === 'series' ? 'active' : ''} type="button" onClick={() => onNavigate('series')}>Series</button>
      </div>
      <div className="nav-actions">
        <button className="nav-icon-button" type="button" onClick={onSearch} aria-label="Search library">
          <MaterialIcon name="search" />
        </button>
        <button className="profile-button" type="button" onClick={onSignOut} title="Sign out" aria-label={`Sign out ${session.user.Name}`}>
          {session.user.Name.slice(0, 1).toUpperCase()}
        </button>
      </div>
    </nav>
  );
}
