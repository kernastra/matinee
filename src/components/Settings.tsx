import { useEffect, useState } from 'react';
import {
  APP_VERSION,
  getServerInfo,
  userImageUrl,
  type JellyfinServerInfo,
  type JellyfinSession,
} from '../lib/jellyfin';
import { defaultSettings, type AppSettings } from '../lib/settings';
import AppNav, { type AppView } from './AppNav';
import ImageProviderSettings from './ImageProviderSettings';
import MediaIntegrationSettings from './MediaIntegrationSettings';
import { integrationEnabled } from '../lib/integrations';

type SettingsProps = {
  session: JellyfinSession;
  settings: AppSettings;
  onChange: (settings: AppSettings) => void;
  onNavigate: (view: AppView) => void;
  onSearch: () => void;
  onSignOut: () => void;
};

type ToggleRowProps = {
  checked: boolean;
  title: string;
  description: string;
  onChange: (checked: boolean) => void;
};

function ToggleRow({ checked, title, description, onChange }: ToggleRowProps) {
  return (
    <label className="settings-row settings-row--toggle">
      <span>
        <strong>{title}</strong>
        <small>{description}</small>
      </span>
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />
      <span className="settings-toggle" aria-hidden="true"><span /></span>
    </label>
  );
}

export default function Settings({ session, settings, onChange, onNavigate, onSearch, onSignOut }: SettingsProps) {
  const [serverInfo, setServerInfo] = useState<JellyfinServerInfo | null>(null);
  const [connectionState, setConnectionState] = useState<'checking' | 'connected' | 'unavailable'>('checking');
  const [avatarFailed, setAvatarFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setConnectionState('checking');
    getServerInfo(session)
      .then((info) => {
        if (cancelled) return;
        setServerInfo(info);
        setConnectionState('connected');
      })
      .catch(() => {
        if (!cancelled) setConnectionState('unavailable');
      });
    return () => {
      cancelled = true;
    };
  }, [session]);

  function update<K extends keyof AppSettings>(key: K, value: AppSettings[K]) {
    onChange({ ...settings, [key]: value });
  }

  function configureIntegration(provider: 'radarr' | 'sonarr', url: string) {
    update(provider === 'radarr' ? 'radarrUrl' : 'sonarrUrl', url);
  }

  const hasAvatar = Boolean(session.user.PrimaryImageTag) && !avatarFailed;
  const connectionLabel = connectionState === 'checking'
    ? 'Checking connection'
    : connectionState === 'connected'
      ? 'Connected'
      : 'Connection unavailable';

  return (
    <main className="settings-shell">
      <AppNav session={session} activeView="settings" calendarEnabled={integrationEnabled(settings)} onNavigate={onNavigate} onSearch={onSearch} onSignOut={onSignOut} />
      <header className="settings-header">
        <p className="eyebrow">Make Matinee yours</p>
        <h1>Settings</h1>
        <p>Playback, interface, media integrations, image generation, and Jellyfin account preferences.</p>
      </header>

      <div className="settings-content">
        <section className="settings-section" aria-labelledby="account-settings-title">
          <div className="settings-section__heading">
            <span>01</span>
            <div>
              <h2 id="account-settings-title">Account &amp; server</h2>
              <p>Your active Jellyfin connection.</p>
            </div>
          </div>
          <div className="settings-card settings-account-card">
            <div className="settings-account">
              <span className="settings-account__avatar">
                {hasAvatar ? (
                  <img src={userImageUrl(session, 180)} alt="" onError={() => setAvatarFailed(true)} />
                ) : session.user.Name.slice(0, 1).toUpperCase()}
              </span>
              <span>
                <strong>{session.user.Name}</strong>
                <small>{serverInfo?.ServerName || 'Jellyfin user'}</small>
              </span>
            </div>
            <dl className="settings-account-details">
              <div><dt>Server</dt><dd>{session.serverUrl}</dd></div>
              <div><dt>Status</dt><dd className={`connection-status connection-status--${connectionState}`}><span />{connectionLabel}</dd></div>
              <div><dt>Session</dt><dd>Signed in until Matinee closes</dd></div>
            </dl>
            <button className="secondary-button settings-change-server" type="button" onClick={onSignOut}>Change server</button>
          </div>
        </section>

        <section className="settings-section" aria-labelledby="playback-settings-title">
          <div className="settings-section__heading">
            <span>02</span>
            <div>
              <h2 id="playback-settings-title">Playback</h2>
              <p>Defaults used when a title starts.</p>
            </div>
          </div>
          <div className="settings-card settings-list">
            <label className="settings-row">
              <span><strong>Preferred quality</strong><small>Jellyfin can still transcode when the source is incompatible.</small></span>
              <select value={settings.playbackQuality} onChange={(event) => update('playbackQuality', event.target.value as AppSettings['playbackQuality'])}>
                <option value="auto">Auto</option>
                <option value="original">Original</option>
                <option value="1080p">1080p · 8 Mbps</option>
                <option value="720p">720p · 4 Mbps</option>
              </select>
            </label>
            <label className="settings-row">
              <span><strong>Preferred audio</strong><small>Used when a matching track is available.</small></span>
              <select value={settings.audioLanguage} onChange={(event) => update('audioLanguage', event.target.value as AppSettings['audioLanguage'])}>
                <option value="default">Jellyfin default</option>
                <option value="eng">English</option>
                <option value="spa">Spanish</option>
                <option value="jpn">Japanese</option>
              </select>
            </label>
            <label className="settings-row">
              <span><strong>Subtitles</strong><small>Respect Jellyfin’s selection or begin with subtitles off.</small></span>
              <select value={settings.subtitleMode} onChange={(event) => update('subtitleMode', event.target.value as AppSettings['subtitleMode'])}>
                <option value="jellyfin">Jellyfin default</option>
                <option value="off">Always off</option>
              </select>
            </label>
            <ToggleRow
              title="Autoplay next episode"
              description="Continue into the following episode when playback ends."
              checked={settings.autoplayNextEpisode}
              onChange={(checked) => update('autoplayNextEpisode', checked)}
            />
          </div>
        </section>

        <section className="settings-section" aria-labelledby="interface-settings-title">
          <div className="settings-section__heading">
            <span>03</span>
            <div>
              <h2 id="interface-settings-title">Interface</h2>
              <p>Control Matinee’s cinematic motion.</p>
            </div>
          </div>
          <div className="settings-card settings-list">
            <ToggleRow
              title="Rotate home hero"
              description="Move to a different featured title every seven seconds."
              checked={settings.heroRotation}
              onChange={(checked) => update('heroRotation', checked)}
            />
            <ToggleRow
              title="Reduce motion"
              description="Disable decorative fades, slides, and image movement."
              checked={settings.reducedMotion}
              onChange={(checked) => update('reducedMotion', checked)}
            />
            <label className="settings-row">
              <span><strong>Poster metadata</strong><small>Control when ratings and genres appear over poster artwork.</small></span>
              <select value={settings.posterMetadata} onChange={(event) => update('posterMetadata', event.target.value as AppSettings['posterMetadata'])}>
                <option value="always">Always</option>
                <option value="hover">On hover for custom posters</option>
                <option value="never">Never</option>
              </select>
            </label>
          </div>
        </section>

        <section className="settings-section" aria-labelledby="integration-settings-title">
          <div className="settings-section__heading">
            <span>04</span>
            <div>
              <h2 id="integration-settings-title">Coming soon</h2>
              <p>Connect your monitored Radarr and Sonarr releases.</p>
            </div>
          </div>
          <MediaIntegrationSettings
            radarrUrl={settings.radarrUrl}
            sonarrUrl={settings.sonarrUrl}
            onConfigured={configureIntegration}
          />
        </section>

        <section className="settings-section" aria-labelledby="image-generation-settings-title">
          <div className="settings-section__heading">
            <span>05</span>
            <div>
              <h2 id="image-generation-settings-title">Poster generation</h2>
              <p>Choose how the design studio creates artwork.</p>
            </div>
          </div>
          <ImageProviderSettings value={settings.imageProvider} onChange={(provider) => update('imageProvider', provider)} />
        </section>

        <section className="settings-section" aria-labelledby="about-settings-title">
          <div className="settings-section__heading">
            <span>06</span>
            <div>
              <h2 id="about-settings-title">About</h2>
              <p>Application and server information.</p>
            </div>
          </div>
          <div className="settings-card settings-about">
            <div><span>Matinee</span><strong>Version {APP_VERSION}</strong></div>
            <div><span>Jellyfin</span><strong>{serverInfo?.Version || 'Version unavailable'}</strong></div>
            <div><span>Server OS</span><strong>{serverInfo?.OperatingSystem || 'Unavailable'}</strong></div>
            <div><span>Repository</span><strong>github.com/kernastra/matinee</strong></div>
          </div>
        </section>

        <div className="settings-footer">
          <button className="secondary-button" type="button" onClick={() => onChange(defaultSettings)}>Restore defaults</button>
          <p>Preferences are saved automatically. Provider and integration keys stay in your system credential vault.</p>
        </div>
      </div>
    </main>
  );
}
