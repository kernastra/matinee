import { useEffect, useState } from 'react';
import {
  getArtworkStorageSettings,
  saveArtworkStorageSettings,
  type ArtworkStorageSettings as ArtworkStorageSettingsValue,
} from '../lib/artworkStorage';
import MaterialIcon from './MaterialIcon';

const emptySettings: ArtworkStorageSettingsValue = { libraryRoot: '', exportRoot: '' };

function messageFrom(error: unknown) {
  return typeof error === 'string'
    ? error
    : error instanceof Error
      ? error.message
      : 'Matinee could not save artwork storage.';
}

export default function ArtworkStorageSettings() {
  const [settings, setSettings] = useState(emptySettings);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState('');
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    getArtworkStorageSettings()
      .then((value) => { if (!cancelled) setSettings(value); })
      .catch((error) => {
        if (!cancelled) {
          setMessage(messageFrom(error));
          setFailed(true);
        }
      })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, []);

  function update(key: keyof ArtworkStorageSettingsValue, value: string) {
    setSettings((current) => ({ ...current, [key]: value }));
    setMessage('');
    setFailed(false);
  }

  async function save() {
    setSaving(true);
    setMessage('');
    setFailed(false);
    try {
      const normalized = await saveArtworkStorageSettings({
        libraryRoot: settings.libraryRoot.trim(),
        exportRoot: settings.exportRoot.trim(),
      });
      setSettings(normalized);
      setMessage('Artwork storage saved. New generations will use these folders immediately.');
      window.dispatchEvent(new Event('matinee:artwork-storage-change'));
    } catch (error) {
      setMessage(messageFrom(error));
      setFailed(true);
    } finally {
      setSaving(false);
    }
  }

  if (loading) {
    return <div className="settings-card artwork-storage-settings">Loading artwork storage…</div>;
  }

  return (
    <div className="settings-card artwork-storage-settings">
      <div className="media-path-note">
        <MaterialIcon name="info" />
        <p><strong>Keep working files separate from your Jellyfin library.</strong><small>The artwork library holds version history. The export folder receives copies you deliberately export.</small></p>
      </div>
      <label><span>Artwork library</span><input spellCheck={false} value={settings.libraryRoot} onChange={(event) => update('libraryRoot', event.target.value)} /><small>Changing this location starts a separate library; existing artwork is not moved automatically.</small></label>
      <label><span>Export folder</span><input spellCheck={false} value={settings.exportRoot} onChange={(event) => update('exportRoot', event.target.value)} /><small>Matinee creates the final folder when its parent exists and is safe to use.</small></label>
      <div className="media-path-actions">
        <button className="primary-button" type="button" disabled={saving || !settings.libraryRoot.trim() || !settings.exportRoot.trim()} onClick={save}>{saving ? 'Saving…' : 'Validate & save'}</button>
        <p className={failed ? 'provider-message provider-message--error' : 'provider-message'} role={failed ? 'alert' : 'status'}>{message || 'Use absolute local folder paths.'}</p>
      </div>
    </div>
  );
}
