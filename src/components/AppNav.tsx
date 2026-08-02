import { useEffect, useRef, useState } from 'react';
import { userImageUrl, type JellyfinSession } from '../lib/jellyfin';
import MaterialIcon from './MaterialIcon';

export type AppView = 'home' | 'movies' | 'series' | 'settings';

type AppNavProps = {
  session: JellyfinSession;
  activeView: AppView;
  onNavigate: (view: AppView) => void;
  onSearch: () => void;
  onSignOut: () => void;
};

export default function AppNav({ session, activeView, onNavigate, onSearch, onSignOut }: AppNavProps) {
  const [avatarFailed, setAvatarFailed] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const hasAvatar = Boolean(session.user.PrimaryImageTag) && !avatarFailed;

  useEffect(() => {
    setAvatarFailed(false);
  }, [session.user.Id, session.user.PrimaryImageTag]);

  useEffect(() => {
    if (!menuOpen) return;
    const closeOnOutsideClick = (event: MouseEvent) => {
      if (event.target instanceof Node && !menuRef.current?.contains(event.target)) setMenuOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setMenuOpen(false);
    };
    document.addEventListener('mousedown', closeOnOutsideClick);
    window.addEventListener('keydown', closeOnEscape);
    return () => {
      document.removeEventListener('mousedown', closeOnOutsideClick);
      window.removeEventListener('keydown', closeOnEscape);
    };
  }, [menuOpen]);

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
        <div className="profile-menu-wrap" ref={menuRef}>
          <button
            className="profile-button"
            type="button"
            onClick={() => setMenuOpen((current) => !current)}
            title={session.user.Name}
            aria-label={`Open profile menu for ${session.user.Name}`}
            aria-haspopup="menu"
            aria-expanded={menuOpen}
          >
            {hasAvatar ? (
              <img src={userImageUrl(session, 96)} alt="" onError={() => setAvatarFailed(true)} />
            ) : session.user.Name.slice(0, 1).toUpperCase()}
          </button>
          {menuOpen ? (
            <div className="profile-menu" role="menu">
              <div className="profile-menu__account">
                <span className="profile-menu__avatar">
                  {hasAvatar ? <img src={userImageUrl(session, 96)} alt="" /> : session.user.Name.slice(0, 1).toUpperCase()}
                </span>
                <span>
                  <strong>{session.user.Name}</strong>
                  <small>Jellyfin account</small>
                </span>
              </div>
              <button type="button" role="menuitem" onClick={() => { setMenuOpen(false); onNavigate('settings'); }}>
                <MaterialIcon name="tune" /> Settings
              </button>
              <span className="profile-menu__divider" />
              <button type="button" role="menuitem" onClick={onSignOut}>Sign out</button>
            </div>
          ) : null}
        </div>
      </div>
    </nav>
  );
}
