import { useEffect, useState } from 'react';
import {
  clearUpcomingCache,
  getIntegrationKeyStatus,
  removeIntegration,
  testAndSaveIntegration,
  type MediaIntegration,
} from '../lib/integrations';
import MaterialIcon from './MaterialIcon';

type Props = {
  radarrUrl: string;
  sonarrUrl: string;
  onConfigured: (provider: MediaIntegration, url: string) => void;
};

type ProviderState = {
  keyConfigured: boolean | null;
  message?: string;
  ready: boolean;
};

const providerDetails = {
  radarr: {
    title: 'Radarr',
    detail: 'Monitored movie release dates',
    placeholder: 'http://radarr.local:7878',
    icon: 'movie' as const,
  },
  sonarr: {
    title: 'Sonarr',
    detail: 'Monitored series and episode air dates',
    placeholder: 'http://sonarr.local:8989',
    icon: 'tv' as const,
  },
};

function errorMessage(error: unknown) {
  return typeof error === 'string'
    ? error
    : error instanceof Error
      ? error.message
      : 'Matinee could not update this integration.';
}

export default function MediaIntegrationSettings({ radarrUrl, sonarrUrl, onConfigured }: Props) {
  const [urls, setUrls] = useState({ radarr: radarrUrl, sonarr: sonarrUrl });
  const [keys, setKeys] = useState({ radarr: '', sonarr: '' });
  const [states, setStates] = useState<Record<MediaIntegration, ProviderState>>({
    radarr: { keyConfigured: null, ready: Boolean(radarrUrl) },
    sonarr: { keyConfigured: null, ready: Boolean(sonarrUrl) },
  });
  const [busy, setBusy] = useState<MediaIntegration | null>(null);

  useEffect(() => {
    setUrls({ radarr: radarrUrl, sonarr: sonarrUrl });
    setStates((current) => ({
      radarr: { ...current.radarr, ready: Boolean(radarrUrl) },
      sonarr: { ...current.sonarr, ready: Boolean(sonarrUrl) },
    }));
  }, [radarrUrl, sonarrUrl]);

  useEffect(() => {
    let cancelled = false;
    Promise.allSettled([getIntegrationKeyStatus('radarr'), getIntegrationKeyStatus('sonarr')])
      .then((results) => {
        if (cancelled) return;
        setStates((current) => ({
          radarr: {
            ...current.radarr,
            keyConfigured: results[0].status === 'fulfilled' && results[0].value.configured,
          },
          sonarr: {
            ...current.sonarr,
            keyConfigured: results[1].status === 'fulfilled' && results[1].value.configured,
          },
        }));
      });
    return () => { cancelled = true; };
  }, []);

  async function connect(provider: MediaIntegration) {
    setBusy(provider);
    setStates((current) => ({
      ...current,
      [provider]: { ...current[provider], message: undefined },
    }));
    try {
      const result = await testAndSaveIntegration(provider, urls[provider], keys[provider]);
      setUrls((current) => ({ ...current, [provider]: result.serverUrl }));
      setKeys((current) => ({ ...current, [provider]: '' }));
      setStates((current) => ({
        ...current,
        [provider]: {
          keyConfigured: true,
          ready: true,
          message: `${providerDetails[provider].title} ${result.version || ''} connected.`.replace('  ', ' '),
        },
      }));
      clearUpcomingCache();
      onConfigured(provider, result.serverUrl);
    } catch (error) {
      setStates((current) => ({
        ...current,
        [provider]: { ...current[provider], ready: false, message: errorMessage(error) },
      }));
    } finally {
      setBusy(null);
    }
  }

  async function disconnect(provider: MediaIntegration) {
    setBusy(provider);
    try {
      await removeIntegration(provider);
      setKeys((current) => ({ ...current, [provider]: '' }));
      setUrls((current) => ({ ...current, [provider]: '' }));
      setStates((current) => ({
        ...current,
        [provider]: { keyConfigured: false, ready: false, message: 'Integration removed.' },
      }));
      clearUpcomingCache();
      onConfigured(provider, '');
    } catch (error) {
      setStates((current) => ({
        ...current,
        [provider]: { ...current[provider], message: errorMessage(error) },
      }));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="media-integration-grid">
      {(['radarr', 'sonarr'] as const).map((provider) => {
        const detail = providerDetails[provider];
        const state = states[provider];
        const configured = state.ready && state.keyConfigured;
        const needsKey = state.keyConfigured === false && keys[provider].trim().length < 8;
        return (
          <article className={`settings-card media-integration-card${configured ? ' media-integration-card--ready' : ''}`} key={provider}>
            <div className="media-integration-card__heading">
              <span className="media-integration-card__icon"><MaterialIcon name={detail.icon} /></span>
              <span><strong>{detail.title}</strong><small>{detail.detail}</small></span>
              <span className={`integration-state${configured ? ' integration-state--ready' : ''}`}>
                <MaterialIcon name={configured ? 'check_circle' : 'info'} />
                {state.keyConfigured === null ? 'Checking' : configured ? 'Configured' : 'Not configured'}
              </span>
            </div>
            <label>
              Server address
              <input
                type="url"
                inputMode="url"
                placeholder={detail.placeholder}
                value={urls[provider]}
                onChange={(event) => setUrls((current) => ({ ...current, [provider]: event.target.value }))}
              />
            </label>
            <label>
              API key
              <input
                type="password"
                autoComplete="off"
                spellCheck={false}
                placeholder={state.keyConfigured ? 'Leave blank to use the saved key' : `Paste your ${detail.title} API key`}
                value={keys[provider]}
                onChange={(event) => setKeys((current) => ({ ...current, [provider]: event.target.value }))}
              />
            </label>
            <div className="media-integration-card__actions">
              <button
                className="primary-button"
                type="button"
                disabled={busy === provider || !urls[provider].trim() || needsKey}
                onClick={() => connect(provider)}
              >
                {busy === provider ? 'Testing…' : configured ? 'Test connection' : 'Test & save'}
              </button>
              {state.keyConfigured ? (
                <button className="secondary-button" type="button" disabled={busy === provider} onClick={() => disconnect(provider)}>
                  Remove
                </button>
              ) : null}
            </div>
            <p className={`provider-message${state.message && !state.ready && state.message !== 'Integration removed.' ? ' provider-message--error' : ''}`}>
              {state.message || 'Only monitored future releases are read. Matinee never manages downloads.'}
            </p>
          </article>
        );
      })}
    </div>
  );
}
