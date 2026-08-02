export type MaterialIconName =
  | 'arrow_back'
  | 'audio_track'
  | 'bookmark'
  | 'bookmark_add'
  | 'bookmark_added'
  | 'check_circle'
  | 'fast_forward'
  | 'fast_rewind'
  | 'fullscreen'
  | 'fullscreen_exit'
  | 'home'
  | 'info'
  | 'login'
  | 'logout'
  | 'more_horiz'
  | 'movie'
  | 'pause'
  | 'play_arrow'
  | 'search'
  | 'subtitles'
  | 'tune'
  | 'tv'
  | 'volume_off'
  | 'volume_up';

type MaterialIconProps = {
  name: MaterialIconName;
  filled?: boolean;
  className?: string;
};

export default function MaterialIcon({ name, filled = false, className = '' }: MaterialIconProps) {
  return (
    <span
      className={`material-symbols-rounded${filled ? ' material-symbols-rounded--filled' : ''}${className ? ` ${className}` : ''}`}
      aria-hidden="true"
    >
      {name}
    </span>
  );
}
