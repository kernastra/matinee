import type { JellyfinItem, MediaSourceInfo, MediaStream } from '../lib/jellyfin';
import MaterialIcon from './MaterialIcon';

export type DetailsModalMode = 'media' | 'more';

type DetailsModalProps = {
  item: JellyfinItem;
  mode: DetailsModalMode;
  actionStatus: string;
  busy: boolean;
  onClearProgress: () => void;
  onClose: () => void;
  onCopyTitle: () => void;
  onMediaInfo: () => void;
  onPlayFromBeginning: () => void;
  onToggleFavorite: () => void;
  onTogglePlayed: () => void;
};

function formatBytes(bytes?: number) {
  if (!bytes) return null;
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const unit = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / (1024 ** unit)).toFixed(unit > 2 ? 1 : 0)} ${units[unit]}`;
}

function formatBitrate(value?: number) {
  if (!value) return null;
  return value >= 1_000_000
    ? `${(value / 1_000_000).toFixed(1)} Mbps`
    : `${Math.round(value / 1000)} kbps`;
}

function sourceName(source: MediaSourceInfo, index: number) {
  if (source.Name) return source.Name;
  if (source.Path) return source.Path.split(/[\\/]/).pop() || `Version ${index + 1}`;
  return `Version ${index + 1}`;
}

function streamDetails(stream: MediaStream) {
  const details = [
    stream.Codec?.toUpperCase(),
    stream.Profile,
    stream.Type === 'Video' && stream.Width && stream.Height ? `${stream.Width}×${stream.Height}` : null,
    stream.VideoRangeType || stream.VideoRange,
    stream.Type === 'Video' && (stream.AverageFrameRate || stream.RealFrameRate)
      ? `${(stream.AverageFrameRate || stream.RealFrameRate)?.toFixed(2)} fps`
      : null,
    stream.BitDepth ? `${stream.BitDepth}-bit` : null,
    stream.ChannelLayout || (stream.Channels ? `${stream.Channels} channels` : null),
    stream.SampleRate ? `${Math.round(stream.SampleRate / 1000)} kHz` : null,
    formatBitrate(stream.BitRate),
    stream.Language?.toUpperCase(),
    stream.IsDefault ? 'Default' : null,
    stream.IsForced ? 'Forced' : null,
    stream.IsExternal ? 'External' : null,
  ].filter(Boolean);
  return details.join(' · ');
}

function MediaInfo({ item }: { item: JellyfinItem }) {
  const sources: MediaSourceInfo[] = item.MediaSources?.length
    ? item.MediaSources
    : [{ Name: 'Media', MediaStreams: item.MediaStreams }];

  return (
    <div className="details-media-info">
      {sources.map((source, sourceIndex) => {
        const streams = source.MediaStreams ?? item.MediaStreams ?? [];
        return (
          <section className="details-media-source" key={source.Id ?? `${source.Name ?? 'source'}-${sourceIndex}`}>
            <div className="details-media-source__heading">
              <div>
                <p>Media source {sources.length > 1 ? sourceIndex + 1 : ''}</p>
                <h3>{sourceName(source, sourceIndex)}</h3>
              </div>
              <span>{[source.Container?.toUpperCase(), formatBytes(source.Size), formatBitrate(source.Bitrate)].filter(Boolean).join(' · ')}</span>
            </div>
            {streams.length ? (
              <div className="details-streams">
                {streams.map((stream) => (
                  <div className="details-stream" key={`${stream.Type}-${stream.Index}`}>
                    <span>{stream.Type}</span>
                    <div>
                      <strong>{stream.DisplayTitle || stream.Title || `${stream.Type} ${stream.Index + 1}`}</strong>
                      <small>{streamDetails(stream) || 'No additional stream information'}</small>
                    </div>
                  </div>
                ))}
              </div>
            ) : <p className="details-modal__empty">Jellyfin did not return extended stream information for this file.</p>}
          </section>
        );
      })}
    </div>
  );
}

export default function DetailsModal({
  item,
  mode,
  actionStatus,
  busy,
  onClearProgress,
  onClose,
  onCopyTitle,
  onMediaInfo,
  onPlayFromBeginning,
  onToggleFavorite,
  onTogglePlayed,
}: DetailsModalProps) {
  const isFavorite = Boolean(item.UserData?.IsFavorite);
  const isPlayed = Boolean(item.UserData?.Played);
  const hasProgress = Boolean(item.UserData?.PlaybackPositionTicks);

  return (
    <div className="details-modal-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <section className="details-modal" role="dialog" aria-modal="true" aria-labelledby="details-modal-title">
        <header>
          <div>
            <p className="eyebrow">{mode === 'media' ? 'Playback details' : item.Name}</p>
            <h2 id="details-modal-title">{mode === 'media' ? 'Media information' : 'More actions'}</h2>
          </div>
          <button className="details-modal__close" type="button" onClick={onClose} aria-label="Close" autoFocus>×</button>
        </header>

        {mode === 'media' ? <MediaInfo item={item} /> : (
          <div className="details-more-actions">
            <button type="button" onClick={onPlayFromBeginning}>
              <MaterialIcon name="fast_rewind" />
              <span><strong>Play from beginning</strong><small>Ignore the saved resume position</small></span>
            </button>
            <button type="button" onClick={onMediaInfo}>
              <MaterialIcon name="info" />
              <span><strong>Media information</strong><small>Video, audio, subtitle, and file details</small></span>
            </button>
            <button type="button" onClick={onToggleFavorite} disabled={busy}>
              <MaterialIcon name={isFavorite ? 'bookmark_added' : 'bookmark'} />
              <span><strong>{isFavorite ? 'Remove from watchlist' : 'Add to watchlist'}</strong><small>Update this title in Jellyfin</small></span>
            </button>
            <button type="button" onClick={onTogglePlayed} disabled={busy}>
              <MaterialIcon name="check_circle" filled={isPlayed} />
              <span><strong>{isPlayed ? 'Mark as unplayed' : 'Mark as played'}</strong><small>Update your watched status</small></span>
            </button>
            <button type="button" onClick={onClearProgress} disabled={!hasProgress || busy}>
              <MaterialIcon name="fast_rewind" />
              <span><strong>Remove from Continue Watching</strong><small>{hasProgress ? 'Clear the saved playback position' : 'No saved progress for this title'}</small></span>
            </button>
            <button type="button" onClick={onCopyTitle}>
              <MaterialIcon name="more_horiz" />
              <span><strong>Copy title</strong><small>Copy “{item.Name}” to the clipboard</small></span>
            </button>
          </div>
        )}
        {actionStatus ? <p className="details-modal__status" role="status">{actionStatus}</p> : null}
      </section>
    </div>
  );
}
