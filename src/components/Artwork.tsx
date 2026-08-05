import { useState } from 'react';
import { imageUrl, type JellyfinItem, type JellyfinSession } from '../lib/jellyfin';
import MaterialIcon from './MaterialIcon';
import { useCustomPosters } from './CustomPosterProvider';

type ArtworkProps = {
  item: JellyfinItem;
  session: JellyfinSession;
  width: number;
  className?: string;
  loading?: 'eager' | 'lazy';
};

export default function Artwork({
  item,
  session,
  width,
  className = '',
  loading = 'lazy',
}: ArtworkProps) {
  const { posters } = useCustomPosters();
  const customPoster = posters[item.Id];
  const source = customPoster?.dataUrl || imageUrl(session, item, 'Primary', width);
  const [failedSource, setFailedSource] = useState('');
  const failed = (!customPoster && !item.ImageTags?.Primary) || failedSource === source;

  if (failed) {
    return (
      <span className={`artwork-fallback ${className}`} aria-label={`${item.Name} artwork unavailable`}>
        <MaterialIcon name={item.Type === 'Series' ? 'tv' : 'movie'} />
        <small>{item.Name}</small>
      </span>
    );
  }

  return (
    <img
      className={className}
      src={source}
      alt=""
      loading={loading}
      decoding="async"
      onError={() => setFailedSource(source)}
    />
  );
}
