import { useEffect, useState } from 'react';
import {
  getProviderKeyStatus,
  removeProviderKey,
  saveProviderKey,
  scanLocalImageProvider,
  type KeyImageProvider,
  type LocalImageProvider,
  type LocalProviderStatus,
} from '../lib/imageGeneration';
import type { ImageProvider } from '../lib/settings';
import MaterialIcon from './MaterialIcon';
import type { MaterialIconName } from './MaterialIcon';

type Props = {
  value: ImageProvider;
  onChange: (provider: ImageProvider) => void;
};

type KeyState = Record<KeyImageProvider, boolean | null>;

function errorMessage(error: unknown) {
  return typeof error === 'string' ? error : error instanceof Error ? error.message : 'Something went wrong.';
}

export default function ImageProviderSettings({ value, onChange }: Props) {
  const [keys, setKeys] = useState<KeyState>({ fal: null, higgsfield: null });
  const [keyInputs, setKeyInputs] = useState<Record<KeyImageProvider, string>>({ fal: '', higgsfield: '' });
  const [localStatus, setLocalStatus] = useState<Partial<Record<LocalImageProvider, LocalProviderStatus>>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<Partial<Record<ImageProvider, string>>>({});

  useEffect(() => {
    let cancelled = false;
    Promise.allSettled([getProviderKeyStatus('fal'), getProviderKeyStatus('higgsfield')]).then((results) => {
      if (cancelled) return;
      setKeys({
        fal: results[0].status === 'fulfilled' ? results[0].value.configured : false,
        higgsfield: results[1].status === 'fulfilled' ? results[1].value.configured : false,
      });
    });
    return () => { cancelled = true; };
  }, []);

  async function scan(provider: LocalImageProvider) {
    setBusy(`scan-${provider}`);
    setMessage((current) => ({ ...current, [provider]: undefined }));
    try {
      const result = await scanLocalImageProvider(provider);
      setLocalStatus((current) => ({ ...current, [provider]: result }));
    } catch (error) {
      setMessage((current) => ({ ...current, [provider]: errorMessage(error) }));
    } finally {
      setBusy(null);
    }
  }

  async function saveKey(provider: KeyImageProvider) {
    setBusy(`key-${provider}`);
    setMessage((current) => ({ ...current, [provider]: undefined }));
    try {
      const result = await saveProviderKey(provider, keyInputs[provider]);
      setKeys((current) => ({ ...current, [provider]: result.configured }));
      setKeyInputs((current) => ({ ...current, [provider]: '' }));
      setMessage((current) => ({ ...current, [provider]: 'API key saved securely.' }));
    } catch (error) {
      setMessage((current) => ({ ...current, [provider]: errorMessage(error) }));
    } finally {
      setBusy(null);
    }
  }

  async function removeKey(provider: KeyImageProvider) {
    setBusy(`key-${provider}`);
    setMessage((current) => ({ ...current, [provider]: undefined }));
    try {
      const result = await removeProviderKey(provider);
      setKeys((current) => ({ ...current, [provider]: result.configured }));
      setMessage((current) => ({ ...current, [provider]: 'API key removed.' }));
    } catch (error) {
      setMessage((current) => ({ ...current, [provider]: errorMessage(error) }));
    } finally {
      setBusy(null);
    }
  }

  const providerChoice = (provider: ImageProvider, title: string, detail: string, icon: MaterialIconName) => (
    <label className={`image-provider-card${value === provider ? ' image-provider-card--active' : ''}`}>
      <input type="radio" name="image-provider" value={provider} checked={value === provider} onChange={() => onChange(provider)} />
      <span className="image-provider-card__icon"><MaterialIcon name={icon} /></span>
      <span className="image-provider-card__copy">
        <strong>{title}</strong>
        <small>{detail}</small>
      </span>
      <span className="image-provider-card__check">{value === provider ? <MaterialIcon name="check_circle" /> : <span aria-hidden="true" />}</span>
    </label>
  );

  return (
    <div className="image-provider-settings">
      <div className="image-provider-grid">
        {providerChoice('codex', 'ChatGPT · Codex', 'Use a local Codex CLI signed in with your ChatGPT subscription.', 'movie')}
        {providerChoice('fal', 'fal.ai', 'Use FLUX through your own fal.ai developer account.', 'play_arrow')}
        {providerChoice('higgsfield', 'Higgsfield', 'Prepare a Higgsfield account or private developer credential.', 'tv')}
      </div>

      {value === 'codex' && (
        <div className="image-provider-panel">
          <div>
            <strong>Local Codex CLI</strong>
            <small>Matinee checks this computer for Codex and verifies its ChatGPT sign-in.</small>
          </div>
          <button className="secondary-button" type="button" disabled={busy === 'scan-codex'} onClick={() => scan('codex')}>
            <MaterialIcon name="search" /> {busy === 'scan-codex' ? 'Scanning…' : 'Scan this computer'}
          </button>
          {localStatus.codex && (
            <div className={`provider-status provider-status--${localStatus.codex.authenticated ? 'ready' : 'warning'}`}>
              <MaterialIcon name={localStatus.codex.authenticated ? 'check_circle' : 'info'} />
              <span><strong>{localStatus.codex.authenticated ? 'Ready to generate' : localStatus.codex.found ? 'Sign-in needed' : 'Codex not found'}</strong><small>{localStatus.codex.detail}{localStatus.codex.path ? ` · ${localStatus.codex.path}` : ''}</small></span>
            </div>
          )}
          {message.codex && <p className="provider-message provider-message--error">{message.codex}</p>}
        </div>
      )}

      {value === 'fal' && (
        <KeyPanel provider="fal" label="fal.ai API key" configured={keys.fal} value={keyInputs.fal} busy={busy === 'key-fal'} message={message.fal} onValue={(key) => setKeyInputs((current) => ({ ...current, fal: key }))} onSave={() => saveKey('fal')} onRemove={() => removeKey('fal')} />
      )}

      {value === 'higgsfield' && (
        <div className="image-provider-panel image-provider-panel--stacked">
          <div className="provider-note"><MaterialIcon name="info" /><p><strong>Public Higgsfield access uses account login.</strong><small>Higgsfield currently documents CLI/MCP login rather than a public API-key image endpoint. Matinee can securely hold a private developer key, but generation stays unavailable until an endpoint is documented.</small></p></div>
          <button className="secondary-button provider-scan-inline" type="button" disabled={busy === 'scan-higgsfield'} onClick={() => scan('higgsfield')}><MaterialIcon name="search" /> {busy === 'scan-higgsfield' ? 'Scanning…' : 'Scan for Higgsfield CLI'}</button>
          {localStatus.higgsfield && <div className="provider-status provider-status--warning"><MaterialIcon name="tune" /><span><strong>{localStatus.higgsfield.found ? 'CLI detected' : 'CLI not found'}</strong><small>{localStatus.higgsfield.detail}{localStatus.higgsfield.path ? ` · ${localStatus.higgsfield.path}` : ''}</small></span></div>}
          <KeyPanel provider="higgsfield" label="Private API key" configured={keys.higgsfield} value={keyInputs.higgsfield} busy={busy === 'key-higgsfield'} message={message.higgsfield} onValue={(key) => setKeyInputs((current) => ({ ...current, higgsfield: key }))} onSave={() => saveKey('higgsfield')} onRemove={() => removeKey('higgsfield')} embedded />
        </div>
      )}
    </div>
  );
}

type KeyPanelProps = {
  provider: KeyImageProvider;
  label: string;
  configured: boolean | null;
  value: string;
  busy: boolean;
  message?: string;
  onValue: (value: string) => void;
  onSave: () => void;
  onRemove: () => void;
  embedded?: boolean;
};

function KeyPanel({ provider, label, configured, value, busy, message, onValue, onSave, onRemove, embedded }: KeyPanelProps) {
  const content = (
    <>
      <div className="image-provider-key__heading">
        <span><strong>{label}</strong><small>Stored in your operating system’s secure credential vault.</small></span>
        <span className={`provider-key-state${configured ? ' provider-key-state--ready' : ''}`}><MaterialIcon name={configured ? 'check_circle' : 'info'} />{configured === null ? 'Checking' : configured ? 'Configured' : 'Not configured'}</span>
      </div>
      <div className="image-provider-key__form">
        <input aria-label={label} type="password" autoComplete="off" spellCheck={false} placeholder={configured ? 'Enter a replacement key' : `Paste your ${provider === 'fal' ? 'fal.ai' : 'Higgsfield'} key`} value={value} onChange={(event) => onValue(event.target.value)} />
        <button className="primary-button" type="button" disabled={busy || value.trim().length < 12} onClick={onSave}>{busy ? 'Saving…' : configured ? 'Replace key' : 'Save key'}</button>
        {configured && <button className="secondary-button" type="button" disabled={busy} onClick={onRemove}>Remove</button>}
      </div>
      {message && <p className={`provider-message${message.includes('saved') || message.includes('removed') ? '' : ' provider-message--error'}`}>{message}</p>}
    </>
  );
  return embedded ? <div className="image-provider-key image-provider-key--embedded">{content}</div> : <div className="image-provider-panel image-provider-key">{content}</div>;
}
