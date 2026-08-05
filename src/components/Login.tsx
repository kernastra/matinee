import { useEffect, useState, type FormEvent } from 'react';
import {
  authenticate,
  DEFAULT_JELLYFIN_URL,
  normalizeServerUrl,
  refreshSession,
  type JellyfinSession,
} from '../lib/jellyfin';
import {
  forgetJellyfinProfile,
  listJellyfinProfiles,
  rememberJellyfinProfile,
  restoreJellyfinProfile,
  type JellyfinProfile,
} from '../lib/jellyfinProfiles';
import { debugWarn } from '../lib/logger';
import MaterialIcon from './MaterialIcon';

type LoginProps = { onAuthenticated: (session: JellyfinSession) => void };

export default function Login({ onAuthenticated }: LoginProps) {
  const [serverUrl, setServerUrl] = useState(DEFAULT_JELLYFIN_URL);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [profiles, setProfiles] = useState<JellyfinProfile[]>([]);
  const [restoringId, setRestoringId] = useState('');

  useEffect(() => {
    let cancelled = false;
    listJellyfinProfiles()
      .then((remembered) => {
        if (!cancelled) setProfiles(remembered);
      })
      .catch((reason) => debugWarn('[login] could not load remembered Jellyfin profiles', reason));
    return () => { cancelled = true; };
  }, []);
  let insecureConnection = false;

  function serverName(value: string) {
    try {
      return new URL(value).host;
    } catch {
      return value;
    }
  }
  try {
    const parsed = new URL(normalizeServerUrl(serverUrl));
    insecureConnection = parsed.protocol === 'http:' && !['localhost', '127.0.0.1', '::1'].includes(parsed.hostname);
  } catch {
    // Validation stays with submit; an incomplete address should not show two errors.
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    setError('');
    setLoading(true);
    try {
      const session = await authenticate(serverUrl, username, password);
      try {
        await rememberJellyfinProfile(session);
      } catch (reason) {
        debugWarn('[login] session started but could not be remembered securely', reason);
      }
      onAuthenticated(session);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not sign in.');
    } finally {
      setLoading(false);
    }
  }

  async function resume(profile: JellyfinProfile) {
    setError('');
    setRestoringId(profile.id);
    try {
      const session = await refreshSession(await restoreJellyfinProfile(profile.id));
      await rememberJellyfinProfile(session);
      onAuthenticated(session);
    } catch (reason) {
      setServerUrl(profile.serverUrl);
      setUsername(profile.userName);
      setError('That saved session is no longer accepted. Enter your password to reconnect it.');
      debugWarn('[login] remembered Jellyfin session could not be restored', reason);
    } finally {
      setRestoringId('');
    }
  }

  async function forget(profileId: string) {
    setError('');
    try {
      await forgetJellyfinProfile(profileId);
      setProfiles((current) => current.filter((profile) => profile.id !== profileId));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  return (
    <main className="login-shell">
      <div className="login-atmosphere" aria-hidden="true" />
      <section className="login-card">
        <p className="eyebrow">Your library, reimagined</p>
        <h1>Movie night starts here.</h1>
        <p className="login-copy">
          Connect to your Jellyfin server to browse your library and pick up exactly where you left off.
        </p>

        {profiles.length ? (
          <div className="remembered-profiles" aria-label="Remembered Jellyfin accounts">
            <span className="remembered-profiles__label">Continue as</span>
            {profiles.map((profile) => (
              <div className="remembered-profile" key={profile.id}>
                <button className="remembered-profile__account" type="button" disabled={Boolean(restoringId)} onClick={() => void resume(profile)}>
                  <span className="remembered-profile__avatar">{profile.userName.slice(0, 1).toUpperCase()}</span>
                  <span><strong>{profile.userName}</strong><small>{serverName(profile.serverUrl)}</small></span>
                  <MaterialIcon name={restoringId === profile.id ? 'more_horiz' : 'login'} />
                </button>
                <button className="remembered-profile__forget" type="button" disabled={Boolean(restoringId)} aria-label={`Forget ${profile.userName} on ${profile.serverUrl}`} onClick={() => void forget(profile.id)}>
                  <MaterialIcon name="logout" />
                </button>
              </div>
            ))}
            <div className="remembered-profiles__divider"><span>or use another account</span></div>
          </div>
        ) : null}

        <form onSubmit={(event) => void submit(event)}>
          <label>
            <span>Jellyfin server</span>
            <input
              value={serverUrl}
              onChange={(event) => setServerUrl(event.target.value)}
              autoComplete="url"
              placeholder="http://jellyfin.local:8096"
              required
            />
          </label>
          {insecureConnection ? <p className="form-warning">This server uses unencrypted HTTP. Prefer HTTPS when connecting beyond this computer.</p> : null}
          <label>
            <span>Username</span>
            <input
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              autoComplete="username"
              autoFocus
              required
            />
          </label>
          <label>
            <span>Password</span>
            <input
              type="password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              autoComplete="current-password"
            />
          </label>
          {error ? <p className="form-error">{error}</p> : null}
          <button className="primary-button login-button" type="submit" disabled={loading || Boolean(restoringId)}>
            {loading ? 'Connecting…' : 'Enter Matinee'}
          </button>
        </form>
      </section>
    </main>
  );
}
