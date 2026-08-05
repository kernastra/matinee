import { useState } from 'react';
import Home from './components/Home';
import Login from './components/Login';
import Details from './components/Details';
import Player from './components/Player';
import WindowChrome from './components/WindowChrome';
import Library from './components/Library';
import SearchOverlay from './components/SearchOverlay';
import SeriesDetails from './components/SeriesDetails';
import Settings from './components/Settings';
import PosterStudio from './components/PosterStudio';
import Calendar from './components/Calendar';
import type { AppView } from './components/AppNav';
import {
  clearSession,
  getFollowingEpisode,
  getNextUpEpisode,
  loadSession,
  saveSession,
  type JellyfinItem,
  type JellyfinSession,
} from './lib/jellyfin';
import { loadSettings, saveSettings, type AppSettings } from './lib/settings';
import { integrationEnabled } from './lib/integrations';

export default function App() {
  const [session, setSession] = useState<JellyfinSession | null>(() => loadSession());
  const [selectedItem, setSelectedItem] = useState<JellyfinItem | null>(null);
  const [playingItem, setPlayingItem] = useState<JellyfinItem | null>(null);
  const [view, setView] = useState<AppView>('home');
  const [searchOpen, setSearchOpen] = useState(false);
  const [settings, setSettings] = useState<AppSettings>(() => loadSettings());
  const isNavigationScreen = Boolean(session && !selectedItem && !playingItem);
  const calendarEnabled = integrationEnabled(settings);

  function authenticated(nextSession: JellyfinSession) {
    saveSession(nextSession);
    setSession(nextSession);
  }

  function signOut() {
    clearSession();
    setSelectedItem(null);
    setPlayingItem(null);
    setView('home');
    setSearchOpen(false);
    setSession(null);
  }

  function navigate(nextView: AppView) {
    setSelectedItem(null);
    setView(nextView);
  }

  function changeSettings(nextSettings: AppSettings) {
    saveSettings(nextSettings);
    setSettings(nextSettings);
  }

  async function play(item: JellyfinItem) {
    if (item.Type !== 'Series' || !session) {
      setPlayingItem(item);
      return;
    }
    try {
      const episode = await getNextUpEpisode(session, item.Id);
      if (episode) {
        setPlayingItem(episode);
        return;
      }
    } catch (error) {
      console.warn('[series] could not resolve next episode', error);
    }
    setSelectedItem(item);
  }

  async function playFollowing(item: JellyfinItem) {
    if (!settings.autoplayNextEpisode || !session || item.Type !== 'Episode') return;
    try {
      const nextEpisode = await getFollowingEpisode(session, item);
      if (nextEpisode) setPlayingItem(nextEpisode);
    } catch (error) {
      console.warn('[series] could not resolve following episode', error);
    }
  }

  let content;
  if (!session) {
    content = <Login onAuthenticated={authenticated} />;
  } else if (playingItem) {
    content = (
      <Player
        item={playingItem}
        session={session}
        settings={settings}
        onBack={() => setPlayingItem(null)}
        onFinished={playFollowing}
      />
    );
  } else if (selectedItem) {
    content = selectedItem.Type === 'Series' ? (
      <SeriesDetails
        item={selectedItem}
        session={session}
        onBack={() => setSelectedItem(null)}
        onPlay={play}
      />
    ) : (
      <Details
        item={selectedItem}
        session={session}
        onBack={() => setSelectedItem(null)}
        onPlay={play}
        onSelect={setSelectedItem}
      />
    );
  } else if (view === 'settings') {
    content = (
      <Settings
        session={session}
        settings={settings}
        onChange={changeSettings}
        onNavigate={navigate}
        onSearch={() => setSearchOpen(true)}
        onSignOut={signOut}
      />
    );
  } else if (view === 'studio') {
    content = (
      <PosterStudio
        session={session}
        provider={settings.imageProvider}
        onNavigate={navigate}
        onSearch={() => setSearchOpen(true)}
        onSignOut={signOut}
        calendarEnabled={calendarEnabled}
      />
    );
  } else if (view === 'calendar' && calendarEnabled) {
    content = (
      <Calendar
        session={session}
        settings={settings}
        onNavigate={navigate}
        onSearch={() => setSearchOpen(true)}
        onSignOut={signOut}
      />
    );
  } else if (view === 'movies' || view === 'series') {
    content = (
      <Library
        type={view === 'movies' ? 'Movie' : 'Series'}
        session={session}
        onNavigate={navigate}
        onSearch={() => setSearchOpen(true)}
        onSignOut={signOut}
        onSelect={setSelectedItem}
        calendarEnabled={calendarEnabled}
      />
    );
  } else {
    content = (
      <Home
        session={session}
        settings={settings}
        onSignOut={signOut}
        onNavigate={navigate}
        onSearch={() => setSearchOpen(true)}
        onSelect={setSelectedItem}
        onPlay={play}
      />
    );
  }

  return (
    <div className={`app-surface app-surface--poster-meta-${settings.posterMetadata}${session ? ' app-surface--authenticated' : ''}${settings.reducedMotion ? ' app-surface--reduced-motion' : ''}`}>
      {playingItem ? null : <WindowChrome integratedNavigation={isNavigationScreen} />}
      {content}
      {session && searchOpen ? (
        <SearchOverlay
          session={session}
          onClose={() => setSearchOpen(false)}
          onSelect={setSelectedItem}
        />
      ) : null}
    </div>
  );
}
