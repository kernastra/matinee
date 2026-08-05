import { useEffect, useMemo, useRef, useState, type ChangeEvent } from 'react';
import {
  exportGeneratedImage,
  exportPosterToMediaFolder,
  generatePosterImage,
  type GeneratedImage,
} from '../lib/imageGeneration';
import {
  backdropImageUrl,
  getLibraryItems,
  imageUrl,
  type JellyfinItem,
  type JellyfinSession,
} from '../lib/jellyfin';
import { loadMovieManifest, type MovieManifest } from '../lib/movieManifest';
import {
  artworkAssetTypes,
  buildCustomArtworkPrompt,
  buildPosterPrompt,
  getPosterRecipe,
  getTitlePosterOptions,
  posterFocusOptions,
  posterTextTreatments,
  type ArtworkAssetType,
  type PosterGenre,
  type PosterFocus,
  type PosterTextTreatment,
} from '../lib/posterPrompts';
import type { ImageProvider } from '../lib/settings';
import AppNav, { type AppView } from './AppNav';
import MaterialIcon from './MaterialIcon';
import { useCustomPosters } from './CustomPosterProvider';

type Props = {
  session: JellyfinSession;
  provider: ImageProvider;
  onNavigate: (view: AppView) => void;
  onSearch: () => void;
  onSignOut: () => void;
};

type HistoryItem = GeneratedImage & {
  id: string;
  itemId: string;
  title: string;
  genre: PosterGenre;
  assetType: ArtworkAssetType;
  focus: PosterFocus;
  subject: string;
  textTreatment: PosterTextTreatment;
};

type PromptMode = 'guided' | 'custom';

function messageFrom(error: unknown) {
  return typeof error === 'string' ? error : error instanceof Error ? error.message : 'Poster generation failed.';
}

function initialGenre(item?: JellyfinItem) {
  return getPosterRecipe(item?.Genres).genre;
}

export default function PosterStudio({ session, provider, onNavigate, onSearch, onSignOut }: Props) {
  const { posters, assignPoster } = useCustomPosters();
  const promptFileRef = useRef<HTMLInputElement>(null);
  const [items, setItems] = useState<JellyfinItem[]>([]);
  const [selectedId, setSelectedId] = useState('');
  const [query, setQuery] = useState('');
  const [assetType, setAssetType] = useState<ArtworkAssetType>('Poster');
  const [focus, setFocus] = useState<PosterFocus>('Auto');
  const [subject, setSubject] = useState('');
  const [textTreatment, setTextTreatment] = useState<PosterTextTreatment>('Title');
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [promptMode, setPromptMode] = useState<PromptMode>('guided');
  const [customPrompt, setCustomPrompt] = useState('');
  const [includeMatineeStyle, setIncludeMatineeStyle] = useState(true);
  const [advancedError, setAdvancedError] = useState('');
  const [generated, setGenerated] = useState<HistoryItem | null>(null);
  const [previewFailed, setPreviewFailed] = useState(false);
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [generating, setGenerating] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exportingToMedia, setExportingToMedia] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [movieManifest, setMovieManifest] = useState<MovieManifest | null>(null);
  const [manifestLoading, setManifestLoading] = useState(false);
  const [manifestError, setManifestError] = useState('');

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    Promise.all([getLibraryItems(session, 'Movie'), getLibraryItems(session, 'Series')])
      .then(([movies, series]) => {
        if (cancelled) return;
        const library = [...movies, ...series].sort((left, right) => left.Name.localeCompare(right.Name));
        setItems(library);
        if (library[0]) {
          setSelectedId(library[0].Id);
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) setError(messageFrom(reason));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [session]);

  useEffect(() => {
    if (!advancedOpen) return undefined;
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === 'Escape') setAdvancedOpen(false);
    }
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [advancedOpen]);

  const selectedItem = useMemo(
    () => items.find((item) => item.Id === selectedId),
    [items, selectedId],
  );
  const selectedMediaPath = selectedItem?.Type === 'Movie'
    ? selectedItem.MediaSources?.find((source) => source.Path)?.Path
    : undefined;

  useEffect(() => {
    let cancelled = false;
    setMovieManifest(null);
    setManifestError('');
    if (!selectedMediaPath) {
      setManifestLoading(false);
      return () => { cancelled = true; };
    }

    setManifestLoading(true);
    loadMovieManifest(selectedMediaPath)
      .then((manifest) => {
        if (!cancelled) setMovieManifest(manifest);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setManifestError(messageFrom(reason));
      })
      .finally(() => {
        if (!cancelled) setManifestLoading(false);
      });
    return () => { cancelled = true; };
  }, [selectedMediaPath]);
  const filteredItems = useMemo(() => {
    const term = query.trim().toLowerCase();
    return term ? items.filter((item) => item.Name.toLowerCase().includes(term)).slice(0, 80) : items.slice(0, 80);
  }, [items, query]);
  const genre = initialGenre(selectedItem);
  const titleOptions = useMemo(
    () => selectedItem ? getTitlePosterOptions(selectedItem, genre, movieManifest) : null,
    [genre, movieManifest, selectedItem],
  );
  const subjectOptions = focus === 'Auto' ? [] : titleOptions?.byFocus[focus] || [];
  useEffect(() => {
    if (focus !== 'Auto' && subjectOptions.length && !subjectOptions.includes(subject)) {
      setSubject(subjectOptions[0]);
    }
  }, [focus, subject, subjectOptions]);
  const referenceUrls = useMemo(
    () => selectedItem?.BackdropImageTags?.slice(0, 1).map((_, index) => backdropImageUrl(session, selectedItem, index, 1280)) || [],
    [selectedItem, session],
  );
  const guidedPrompt = useMemo(() => {
    if (!selectedItem) return '';
    return buildPosterPrompt({
      title: selectedItem.Name,
      year: selectedItem.ProductionYear,
      genres: selectedItem.Genres,
      storyContext: selectedItem.Overview,
      tagline: selectedItem.Taglines?.[0],
      movieManifest,
      hasVisualReferences: referenceUrls.length > 0,
      assetType,
      focus,
      subject: focus === 'Auto' ? undefined : subject,
      textTreatment,
    });
  }, [assetType, focus, movieManifest, referenceUrls.length, selectedItem, subject, textTreatment]);
  const prompt = useMemo(() => {
    if (promptMode === 'guided') return guidedPrompt;
    if (!customPrompt.trim()) return '';
    return buildCustomArtworkPrompt({ customPrompt, assetType, includeMatineeStyle });
  }, [assetType, customPrompt, guidedPrompt, includeMatineeStyle, promptMode]);

  function chooseItem(item: JellyfinItem) {
    setSelectedId(item.Id);
    setFocus('Auto');
    setSubject('');
    setPromptMode('guided');
    setCustomPrompt('');
    setGenerated(null);
    setPreviewFailed(false);
    setError('');
    setNotice('');
  }

  function chooseFocus(nextFocus: PosterFocus) {
    setFocus(nextFocus);
    setSubject(nextFocus === 'Auto' ? '' : titleOptions?.byFocus[nextFocus]?.[0] || '');
  }

  function editGuidedPrompt() {
    setCustomPrompt(guidedPrompt);
    setPromptMode('custom');
  }

  async function importPrompt(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    try {
      const contents = await file.text();
      let imported = contents;
      if (file.name.toLowerCase().endsWith('.json')) {
        const value = JSON.parse(contents) as { prompt?: unknown; text?: unknown; customPrompt?: unknown };
        const candidate = value.prompt ?? value.customPrompt ?? value.text;
        if (typeof candidate !== 'string') throw new Error('The JSON file needs a prompt, customPrompt, or text field.');
        imported = candidate;
      }
      if (!imported.trim()) throw new Error('The imported prompt is empty.');
      setCustomPrompt(imported.trim().slice(0, 20_000));
      setPromptMode('custom');
      setAdvancedError('');
    } catch (reason) {
      setAdvancedError(`Prompt import failed: ${messageFrom(reason)}`);
    }
  }

  async function generate() {
    if (!selectedItem || !prompt) return;
    setGenerating(true);
    setError('');
    setNotice('');
    try {
      const result = await generatePosterImage(provider, prompt, referenceUrls, assetType);
      const artwork = {
        ...result,
        id: `${Date.now()}-${result.localPath}`,
        itemId: selectedItem.Id,
        title: selectedItem.Name,
        genre,
        assetType,
        focus,
        subject,
        textTreatment,
      };
      setGenerated(artwork);
      setPreviewFailed(false);
      setHistory((current) => [
        artwork,
        ...current,
      ].slice(0, 6));
      if (assetType === 'Poster') try {
        await assignPoster(selectedItem.Id, result.localPath);
        setNotice('Poster generated and assigned to this title throughout Matinee.');
      } catch (assignmentError) {
        setError(`The poster was generated, but Matinee could not make it persistent: ${messageFrom(assignmentError)}`);
      } else {
        setNotice(`${assetType} generated. Export it to Pictures when you are ready.`);
      }
    } catch (reason) {
      setError(messageFrom(reason));
    } finally {
      setGenerating(false);
    }
  }

  async function exportPoster() {
    const poster = generated?.itemId === selectedId ? generated : posters[selectedId];
    if (!poster) return;
    setExporting(true);
    setError('');
    setNotice('');
    try {
      const path = await exportGeneratedImage(poster.localPath, selectedItem?.Name || generated?.title || 'Matinee poster');
      setNotice(`Exported to ${path}`);
    } catch (reason) {
      setError(messageFrom(reason));
    } finally {
      setExporting(false);
    }
  }

  async function exportToMediaFolder(overwrite = false) {
    const poster = generated?.itemId === selectedId ? generated : posters[selectedId];
    const moviePath = selectedItem?.Type === 'Movie'
      ? selectedItem.MediaSources?.find((source) => source.Path)?.Path
      : undefined;
    if (!poster || !moviePath) return;
    setExportingToMedia(true);
    setError('');
    setNotice('');
    try {
      const path = await exportPosterToMediaFolder(poster.localPath, moviePath, overwrite);
      setNotice(`Saved as ${path}. Jellyfin may need a metadata refresh before it notices the new poster.`);
    } catch (reason) {
      if (reason === 'POSTER_EXISTS' && !overwrite) {
        const replace = window.confirm('poster.jpg already exists in this movie folder. Replace it with the Matinee poster?');
        if (replace) await exportToMediaFolder(true);
      } else {
        setError(messageFrom(reason));
      }
    } finally {
      setExportingToMedia(false);
    }
  }

  const customPoster = selectedItem ? posters[selectedItem.Id] : undefined;
  const activePoster = generated?.itemId === selectedId ? generated : customPoster;
  const previewImage = activePoster?.dataUrl || (selectedItem?.ImageTags?.Primary ? imageUrl(session, selectedItem, 'Primary', 900) : '');
  const mediaPath = selectedMediaPath;
  const providerLabel = provider === 'codex'
    ? 'ChatGPT · Codex'
    : provider === 'fal'
      ? referenceUrls.length ? 'fal.ai · FLUX 2 Edit' : 'fal.ai · FLUX'
      : 'Higgsfield';
  const previewAssetType = generated?.itemId === selectedId ? generated.assetType : 'Poster';

  return (
    <main className="studio-shell">
      <AppNav session={session} activeView="studio" onNavigate={onNavigate} onSearch={onSearch} onSignOut={onSignOut} />
      <header className="studio-header">
        <div>
          <p className="eyebrow">Backstage tools · Poster workshop</p>
          <h1>Poster Studio</h1>
          <p>Reimagine your library in Matinee’s warm neighborhood-cinema style.</p>
        </div>
        <div className="studio-provider">
          <span>Generator</span>
          <strong><MaterialIcon name="movie" /> {providerLabel}</strong>
          <button type="button" onClick={() => onNavigate('settings')}>Configure</button>
        </div>
      </header>

      <div className="studio-workspace">
        <aside className="studio-library" aria-label="Jellyfin titles">
          <div className="studio-panel-heading">
            <div><span>01</span><strong>Choose a title</strong></div>
            <small>{items.length} in Jellyfin</small>
          </div>
          <label className="studio-search">
            <MaterialIcon name="search" />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search your library" aria-label="Search your Jellyfin library" />
          </label>
          <div className="studio-title-list">
            {loading ? <p className="studio-empty">Loading your library…</p> : null}
            {!loading && !filteredItems.length ? <p className="studio-empty">No matching titles.</p> : null}
            {filteredItems.map((item) => (
              <button className={item.Id === selectedId ? 'active' : ''} type="button" key={item.Id} aria-pressed={item.Id === selectedId} onClick={() => chooseItem(item)}>
                <span className="studio-title-thumb">{posters[item.Id] ? <img src={posters[item.Id].dataUrl} alt="" /> : item.ImageTags?.Primary ? <img src={imageUrl(session, item, 'Primary', 100)} alt="" /> : <MaterialIcon name="movie" />}</span>
                <span><strong>{item.Name}</strong><small>{item.ProductionYear || 'Year unknown'} · {item.Type}{posters[item.Id] ? <em> · Matinee poster</em> : null}</small></span>
                {item.Id === selectedId ? <MaterialIcon name="check_circle" /> : null}
              </button>
            ))}
          </div>
        </aside>

        <section className="studio-stage" aria-label="Poster preview">
          <div className="studio-panel-heading studio-panel-heading--stage">
            <div><span>02</span><strong>Preview</strong></div>
            <small>{activePoster ? 'Matinee custom artwork' : 'Current Jellyfin artwork'}</small>
          </div>
          <div className={`studio-poster studio-poster--${previewAssetType.toLowerCase()}${generating ? ' studio-poster--generating' : ''}`}>
            {previewImage && !previewFailed ? <img src={previewImage} alt={`${generated?.title || selectedItem?.Name || 'Selected title'} poster preview`} onError={() => setPreviewFailed(true)} /> : <div className="studio-poster-empty"><span className="loading-mark">M</span><p>{selectedItem ? 'Artwork unavailable. Generate a new poster.' : 'Choose a title to begin.'}</p></div>}
            {generating ? <div className="studio-generating"><span className="loading-mark">M</span><strong>Developing artwork…</strong><small>This can take a few minutes.</small></div> : null}
            <div className="studio-poster-label"><span>{generated?.assetType || assetType} · {generated?.genre || genre}</span><strong>{generated?.title || selectedItem?.Name || 'Matinee'}</strong></div>
          </div>
          <div className="studio-stage-actions">
            <button className="primary-button" type="button" disabled={!selectedItem || generating || manifestLoading || provider === 'higgsfield'} onClick={generate}>
              <MaterialIcon name="movie" /> {generating ? 'Generating…' : generated ? `Generate another ${assetType.toLowerCase()}` : `Generate ${assetType.toLowerCase()}`}
            </button>
            <button className="secondary-button" type="button" disabled={!activePoster || exporting} onClick={exportPoster}>
              {exporting ? 'Exporting…' : 'Export to Pictures'}
            </button>
            <button className="secondary-button" type="button" disabled={!activePoster || assetType !== 'Poster' || !mediaPath || exportingToMedia} title={assetType !== 'Poster' ? 'Only poster artwork can replace poster.jpg' : !mediaPath ? 'Available when Jellyfin exposes a local movie-file path' : 'Save beside the movie as poster.jpg'} onClick={() => exportToMediaFolder()}>
              {exportingToMedia ? 'Saving…' : 'Save as poster.jpg'}
            </button>
          </div>
          {referenceUrls.length ? <p className="studio-reference-status"><MaterialIcon name="check_circle" /> Using {referenceUrls.length} Jellyfin backdrop {referenceUrls.length === 1 ? 'still' : 'stills'} as visual references</p> : <p className="studio-reference-status studio-reference-status--muted"><MaterialIcon name="info" /> No Jellyfin backdrop stills are available; this generation will use metadata only.</p>}
          {manifestLoading ? <p className="studio-reference-status studio-reference-status--muted"><MaterialIcon name="info" /> Loading movie.mf.json…</p> : null}
          {!manifestLoading && movieManifest ? <p className="studio-reference-status"><MaterialIcon name="check_circle" /> Movie-specific creative manifest loaded</p> : null}
          {!manifestLoading && selectedItem?.Type === 'Movie' && selectedMediaPath && !movieManifest && !manifestError ? <p className="studio-reference-status studio-reference-status--muted"><MaterialIcon name="info" /> No movie.mf.json found; using Jellyfin metadata and the genre recipe.</p> : null}
          {manifestError ? <p className="studio-message studio-message--warning"><MaterialIcon name="info" /> {manifestError}</p> : null}
          {provider === 'higgsfield' ? <p className="studio-message studio-message--warning"><MaterialIcon name="info" /> Higgsfield generation is waiting for a documented developer endpoint. Choose Codex or fal.ai in Settings.</p> : null}
          {error ? <p className="studio-message studio-message--error" role="alert">{error}</p> : null}
          {notice ? <p className="studio-message" role="status">{notice}</p> : null}
        </section>

        <aside className="studio-controls" aria-label="Poster art direction">
          <div className="studio-panel-heading">
            <div><span>03</span><strong>Art direction</strong></div>
            <button className="studio-advanced-trigger" type="button" onClick={() => setAdvancedOpen(true)}><MaterialIcon name="tune" /> Advanced</button>
          </div>
          <div className="studio-control-scroll">
            <div className={`studio-guided-intro${promptMode === 'custom' ? ' studio-guided-intro--custom' : ''}`}><MaterialIcon name={promptMode === 'custom' ? 'tune' : 'movie'} /><p><strong>{promptMode === 'custom' ? 'Custom prompt active.' : 'Matinee directs the details.'}</strong><small>{promptMode === 'custom' ? 'Asset type still applies. Restore the Matinee prompt in Advanced to use guided focus and text controls.' : 'Genre, palette, lighting, texture, and composition are chosen automatically.'}</small></p></div>
            <label className="studio-field"><span>Asset type</span><select value={assetType} onChange={(event) => setAssetType(event.target.value as ArtworkAssetType)}>{Object.keys(artworkAssetTypes).map((value) => <option key={value}>{value}</option>)}</select><small className="studio-field-note">{artworkAssetTypes[assetType].description}</small></label>
            <label className="studio-field"><span>Focus</span><select value={focus} disabled={promptMode === 'custom'} onChange={(event) => chooseFocus(event.target.value as PosterFocus)}>{Object.keys(posterFocusOptions).map((value) => <option key={value}>{value}</option>)}</select><small className="studio-field-note">{posterFocusOptions[focus]}</small></label>
            {focus !== 'Auto' ? <label className="studio-field"><span>Subject <small>From this title</small></span><select value={subject} disabled={promptMode === 'custom' || manifestLoading} onChange={(event) => setSubject(event.target.value)}>{subjectOptions.map((value) => <option key={value}>{value}</option>)}</select><small className="studio-field-note">{movieManifest ? 'These choices come from this title’s movie.mf.json creative context.' : 'No movie manifest is loaded, so Matinee is using Jellyfin metadata and the genre recipe.'}</small></label> : null}
            <label className="studio-field"><span>Text treatment</span><select value={textTreatment} disabled={promptMode === 'custom'} onChange={(event) => setTextTreatment(event.target.value as PosterTextTreatment)}>{Object.keys(posterTextTreatments).map((value) => <option key={value}>{value}</option>)}</select><small className="studio-field-note">{posterTextTreatments[textTreatment]}</small></label>
            <div className="studio-style-lock"><MaterialIcon name="check_circle" /><span><strong>Matinee house style</strong><small>{movieManifest ? 'Movie-specific DNA is combined with the locked Matinee palette, typography, print texture, and restraint.' : 'Midnight Navy, Ticket Cream, typography, print texture, and cinematic restraint are baked in.'}</small></span></div>
          </div>
        </aside>
      </div>

      {history.length ? (
        <section className="studio-history">
          <div className="studio-panel-heading"><div><span>04</span><strong>This session</strong></div><small>Recent generations</small></div>
          <div>{history.map((item) => <button type="button" key={item.id} aria-label={`Preview generated artwork for ${item.title}`} onClick={() => { setSelectedId(item.itemId); setAssetType(item.assetType); setFocus(item.focus); setSubject(item.subject); setTextTreatment(item.textTreatment); setGenerated(item); setPreviewFailed(false); }}><img className={`studio-history-art--${item.assetType.toLowerCase()}`} src={item.dataUrl} alt="" /><span>{item.title}</span></button>)}</div>
        </section>
      ) : null}

      {advancedOpen ? (
        <div className="studio-advanced-overlay" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) setAdvancedOpen(false); }}>
          <section className="studio-advanced" role="dialog" aria-modal="true" aria-labelledby="studio-advanced-title">
            <header>
              <div><p className="eyebrow">Prompt workshop</p><h2 id="studio-advanced-title">Advanced</h2><p>Inspect Matinee’s direction or bring your own complete creative brief.</p></div>
              <button className="studio-advanced-close" type="button" aria-label="Close advanced prompt settings" autoFocus onClick={() => setAdvancedOpen(false)}>×</button>
            </header>
            <div className="studio-prompt-modes" role="tablist" aria-label="Prompt mode">
              <button className={promptMode === 'guided' ? 'active' : ''} type="button" role="tab" aria-selected={promptMode === 'guided'} onClick={() => setPromptMode('guided')}>Matinee guided</button>
              <button className={promptMode === 'custom' ? 'active' : ''} type="button" role="tab" aria-selected={promptMode === 'custom'} onClick={() => setPromptMode('custom')}>Custom prompt</button>
            </div>
            <div className="studio-advanced-body">
              {promptMode === 'guided' ? (
                <>
                  <div className="studio-advanced-summary"><MaterialIcon name="movie" /><span><strong>Built from your four selections</strong><small>The generated prompt is read-only until you create an editable copy.</small></span></div>
                  <pre className="studio-advanced-prompt">{guidedPrompt}</pre>
                </>
              ) : (
                <>
                  <label className="studio-field studio-custom-prompt"><span>Custom creative brief</span><textarea rows={16} value={customPrompt} onChange={(event) => setCustomPrompt(event.target.value)} placeholder="Describe the artwork you want, or import an existing prompt…" /></label>
                  <label className="studio-matinee-toggle"><input type="checkbox" checked={includeMatineeStyle} onChange={(event) => setIncludeMatineeStyle(event.target.checked)} /><span><strong>Apply Matinee art direction</strong><small>Append Matinee’s palette, print texture, restraint, and output requirements.</small></span></label>
                </>
              )}
              {advancedError ? <p className="studio-advanced-error" role="alert">{advancedError}</p> : null}
            </div>
            <footer>
              <div>
                <input ref={promptFileRef} className="studio-import-input" type="file" accept=".txt,.md,.json,text/plain,text/markdown,application/json" onChange={importPrompt} />
                <button className="secondary-button" type="button" onClick={() => promptFileRef.current?.click()}><MaterialIcon name="movie" /> Import prompt</button>
                {promptMode === 'guided' ? <button className="secondary-button" type="button" onClick={editGuidedPrompt}><MaterialIcon name="tune" /> Edit a copy</button> : <button className="secondary-button" type="button" onClick={() => { setPromptMode('guided'); setCustomPrompt(''); }}><MaterialIcon name="fast_rewind" /> Restore Matinee prompt</button>}
              </div>
              <button className="primary-button" type="button" disabled={promptMode === 'custom' && !customPrompt.trim()} onClick={() => setAdvancedOpen(false)}>Apply for this generation</button>
            </footer>
          </section>
        </div>
      ) : null}
    </main>
  );
}
