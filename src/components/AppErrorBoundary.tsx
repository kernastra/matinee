import { Component, type ErrorInfo, type ReactNode } from 'react';
import { clearSession } from '../lib/jellyfin';
import { debugError } from '../lib/logger';
import WindowChrome from './WindowChrome';

type Props = { children: ReactNode };
type State = { failed: boolean };

export default class AppErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    debugError('[app] unrecoverable render error', error, info.componentStack);
  }

  private resetSession = () => {
    clearSession();
    window.location.reload();
  };

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <div className="app-surface app-error-shell">
        <WindowChrome />
        <main className="app-error-card">
          <p className="eyebrow">Matinee hit an unexpected problem</p>
          <h1>Let’s get you back to movie night.</h1>
          <p>Your library and settings are safe. Reload Matinee first, or reset only the current Jellyfin sign-in if the problem continues.</p>
          <div>
            <button className="primary-button" type="button" onClick={() => window.location.reload()}>Reload Matinee</button>
            <button className="secondary-button" type="button" onClick={this.resetSession}>Reset sign-in</button>
          </div>
        </main>
      </div>
    );
  }
}
