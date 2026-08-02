import { useState, type FormEvent } from 'react';
import { authenticate, type JellyfinSession } from '../lib/jellyfin';

type LoginProps = { onAuthenticated: (session: JellyfinSession) => void };

export default function Login({ onAuthenticated }: LoginProps) {
  const [serverUrl, setServerUrl] = useState('http://192.168.1.249:8096');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

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
          Connect to Andromeda to browse your library and pick up exactly where you left off.
        </p>

        <form onSubmit={(event) => void submit(event)}>
          <label>
            <span>Jellyfin server</span>
            <input
              value={serverUrl}
              onChange={(event) => setServerUrl(event.target.value)}
              autoComplete="url"
              required
            />
          </label>
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
