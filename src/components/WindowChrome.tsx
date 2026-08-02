import { closeWindow, minimizeWindow, toggleMaximizeWindow } from '../lib/window';

type LightProps = {
  label: string;
  tone: 'close' | 'minimize' | 'maximize';
  onClick: () => Promise<void>;
};

function Light({ label, tone, onClick }: LightProps) {
  return (
    <button
      type="button"
      aria-label={label}
      className={`traffic-light traffic-light--${tone}`}
      onClick={() => void onClick()}
    />
  );
}

type WindowChromeProps = {
  integratedNavigation?: boolean;
};

export default function WindowChrome({ integratedNavigation = false }: WindowChromeProps) {
  return (
    <header
      className={`window-chrome${integratedNavigation ? ' window-chrome--integrated' : ''}`}
      data-tauri-drag-region
    >
      <div className="traffic-lights">
        <Light label="Close window" tone="close" onClick={closeWindow} />
        <Light label="Minimize window" tone="minimize" onClick={minimizeWindow} />
        <Light label="Maximize window" tone="maximize" onClick={toggleMaximizeWindow} />
      </div>
    </header>
  );
}
