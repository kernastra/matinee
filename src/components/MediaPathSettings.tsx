import { useEffect, useState } from 'react';
import {
  getMediaPathSettings,
  saveMediaPathSettings,
  type MediaPathSettings as MediaPathSettingsValue,
} from '../lib/mediaPaths';
import MaterialIcon from './MaterialIcon';

const emptySettings: MediaPathSettingsValue = { mappings: [], trustedRoots: [] };

function errorMessage(error: unknown) {
  return typeof error === 'string'
    ? error
    : error instanceof Error
      ? error.message
      : 'Matinee could not save these media folders.';
}

export default function MediaPathSettings() {
  const [settings, setSettings] = useState<MediaPathSettingsValue>(emptySettings);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState('');
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    getMediaPathSettings()
      .then((value) => {
        if (!cancelled) setSettings(value);
      })
      .catch((error) => {
        if (cancelled) return;
        setMessage(errorMessage(error));
        setFailed(true);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, []);

  function updateMapping(index: number, key: 'jellyfinPrefix' | 'localRoot', value: string) {
    setSettings((current) => ({
      ...current,
      mappings: current.mappings.map((mapping, mappingIndex) => (
        mappingIndex === index ? { ...mapping, [key]: value } : mapping
      )),
    }));
    setMessage('');
    setFailed(false);
  }

  function updateRoot(index: number, value: string) {
    setSettings((current) => ({
      ...current,
      trustedRoots: current.trustedRoots.map((root, rootIndex) => (
        rootIndex === index ? value : root
      )),
    }));
    setMessage('');
    setFailed(false);
  }

  async function save() {
    setSaving(true);
    setMessage('');
    setFailed(false);
    try {
      const normalized = await saveMediaPathSettings({
        mappings: settings.mappings.map((mapping) => ({
          jellyfinPrefix: mapping.jellyfinPrefix.trim(),
          localRoot: mapping.localRoot.trim(),
        })),
        trustedRoots: settings.trustedRoots.map((root) => root.trim()),
      });
      setSettings(normalized);
      setMessage('Media folders saved. Poster Studio will use these mappings immediately.');
    } catch (error) {
      setMessage(errorMessage(error));
      setFailed(true);
    } finally {
      setSaving(false);
    }
  }

  const incomplete = settings.mappings.some((mapping) => (
    !mapping.jellyfinPrefix.trim() || !mapping.localRoot.trim()
  )) || settings.trustedRoots.some((root) => !root.trim());

  if (loading) {
    return <div className="settings-card media-path-settings media-path-settings--loading">Loading media folders…</div>;
  }

  return (
    <div className="settings-card media-path-settings">
      <div className="media-path-note">
        <MaterialIcon name="info" />
        <p>
          <strong>Translate Jellyfin paths to folders on this computer.</strong>
          <small>For Docker, map a server prefix such as <code>/media</code> to its host folder. Matinee only reads manifests and writes approved artwork inside trusted roots.</small>
        </p>
      </div>

      <div className="media-path-group">
        <div className="media-path-group__heading">
          <span><strong>Path mappings</strong><small>Jellyfin or container prefix → local host folder</small></span>
          <button
            className="secondary-button"
            type="button"
            onClick={() => setSettings((current) => ({
              ...current,
              mappings: [...current.mappings, { jellyfinPrefix: '', localRoot: '' }],
            }))}
          >
            Add mapping
          </button>
        </div>
        {settings.mappings.length ? (
          <div className="media-path-rows">
            {settings.mappings.map((mapping, index) => (
              <div className="media-path-row" key={index}>
                <label>
                  <span>Jellyfin prefix</span>
                  <input aria-label={`Jellyfin prefix ${index + 1}`} spellCheck={false} placeholder="/media" value={mapping.jellyfinPrefix} onChange={(event) => updateMapping(index, 'jellyfinPrefix', event.target.value)} />
                </label>
                <span className="media-path-arrow" aria-hidden="true">→</span>
                <label>
                  <span>Local folder</span>
                  <input aria-label={`Local folder ${index + 1}`} spellCheck={false} placeholder="/mnt/media" value={mapping.localRoot} onChange={(event) => updateMapping(index, 'localRoot', event.target.value)} />
                </label>
                <button className="media-path-remove" type="button" aria-label={`Remove path mapping ${index + 1}`} onClick={() => setSettings((current) => ({ ...current, mappings: current.mappings.filter((_, mappingIndex) => mappingIndex !== index) }))}>Remove</button>
              </div>
            ))}
          </div>
        ) : <p className="media-path-empty">No container mappings configured. Direct Jellyfin paths can still work when they live inside a trusted root.</p>}
      </div>

      <div className="media-path-group">
        <div className="media-path-group__heading">
          <span><strong>Trusted local roots</strong><small>Additional direct paths; every mapped local folder is trusted automatically</small></span>
          <button
            className="secondary-button"
            type="button"
            onClick={() => setSettings((current) => ({
              ...current,
              trustedRoots: [...current.trustedRoots, ''],
            }))}
          >
            Add folder
          </button>
        </div>
        {settings.trustedRoots.length ? (
          <div className="trusted-root-list">
            {settings.trustedRoots.map((root, index) => (
              <div className="trusted-root-row" key={index}>
                <input aria-label={`Trusted local media root ${index + 1}`} spellCheck={false} placeholder="/mnt/media" value={root} onChange={(event) => updateRoot(index, event.target.value)} />
                <button className="media-path-remove" type="button" aria-label={`Remove trusted root ${index + 1}`} onClick={() => setSettings((current) => ({ ...current, trustedRoots: current.trustedRoots.filter((_, rootIndex) => rootIndex !== index) }))}>Remove</button>
              </div>
            ))}
          </div>
        ) : <p className="media-path-empty">Add at least one trusted root to enable local manifests and direct artwork export.</p>}
      </div>

      <div className="media-path-actions">
        <button className="primary-button" type="button" disabled={saving || incomplete} onClick={save}>{saving ? 'Saving…' : 'Validate & save'}</button>
        <p className={failed ? 'provider-message provider-message--error' : 'provider-message'} role={failed ? 'alert' : 'status'}>{message || 'Folders must already exist and be accessible from this computer.'}</p>
      </div>
    </div>
  );
}
