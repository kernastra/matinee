import { useState, type FormEvent } from 'react';
import { authenticate, DEFAULT_JELLYFIN_URL, normalizeServerUrl, type JellyfinSession } from '../lib/jellyfin';

type LoginProps = { onAuthenticated: (session: JellyfinSession) => void };

export default function Login({ onAuthenticated }: LoginProps) {
  const [serverUrl, setServerUrl] = useState(DEFAULT_JELLYFIN_URL);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  let insecureConnection = false;
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
      onAuthenticated(await authenticate(serverUrl, username, password));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not sign in.');
    } finally {
      setLoading(false);
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
          <button className="primary-button login-button" type="submit" disabled={loading}>
            {loading ? 'Connecting…' : 'Enter Matinee'}
          </button>
        </form>
      </section>
    </main>
  );
}
