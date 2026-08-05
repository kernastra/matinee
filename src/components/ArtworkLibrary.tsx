import { useMemo } from 'react';
import type { ArtworkRecord } from '../lib/imageGeneration';
import type { JellyfinItem } from '../lib/jellyfin';

type Props = {
  records: ArtworkRecord[];
  items: JellyfinItem[];
  busyVersion: string;
  onPreview: (record: ArtworkRecord) => void;
  onRestore: (record: ArtworkRecord) => void;
  onRemove: (record: ArtworkRecord) => void;
};

export default function ArtworkLibrary({ records, items, busyVersion, onPreview, onRestore, onRemove }: Props) {
  const itemNames = useMemo(
    () => new Map(items.map((item) => [item.Id, item.Name])),
    [items],
  );

  return (
    <section className="studio-artwork-library" aria-labelledby="studio-artwork-library-title">
      <div className="studio-panel-heading">
        <div><span>04</span><strong id="studio-artwork-library-title">Artwork library</strong></div>
        <small>{records.length ? `${records.length} saved ${records.length === 1 ? 'version' : 'versions'}` : 'New generations appear here'}</small>
      </div>
      {records.length ? (
        <div className="studio-artwork-grid">
          {records.map((record) => {
            const displayTitle = itemNames.get(record.itemId) || record.title;
            return (
              <article className={record.active ? 'studio-artwork-card studio-artwork-card--active' : 'studio-artwork-card'} key={`${record.itemId}-${record.versionId}`}>
                <button className="studio-artwork-preview" type="button" aria-label={`Preview saved ${record.assetType.toLowerCase()} for ${displayTitle}`} onClick={() => onPreview(record)}>
                  <span className={`studio-artwork-image studio-artwork-image--${record.assetType.toLowerCase()}`}><img src={record.thumbnailDataUrl} alt="" />{record.active ? <em>In use</em> : null}</span>
                  <span><strong>{displayTitle}</strong><small>{record.assetType} · {new Date(record.createdAt).toLocaleDateString()}</small></span>
                </button>
                <div className="studio-artwork-actions">
                  {record.assetType === 'Poster' && !record.active ? <button type="button" disabled={Boolean(busyVersion)} onClick={() => onRestore(record)}>Use poster</button> : <span>{record.active ? 'Current poster' : 'Saved asset'}</span>}
                  <button type="button" disabled={Boolean(busyVersion)} onClick={() => onRemove(record)}>Remove</button>
                </div>
              </article>
            );
          })}
        </div>
      ) : <p className="studio-artwork-empty">Generate artwork to start a persistent, restorable history for your library.</p>}
    </section>
  );
}
