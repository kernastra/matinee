import matineePosterStyle from '../data/matinee-poster-style.json';
import { getMovieManifestFocusOptions, type MovieManifest } from './movieManifest';

export type PosterGenre =
  | 'Action'
  | 'Adventure'
  | 'Animation'
  | 'Comedy'
  | 'Crime'
  | 'Documentary'
  | 'Drama'
  | 'Family'
  | 'Fantasy'
  | 'Horror'
  | 'Music'
  | 'Mystery'
  | 'Romance'
  | 'Science Fiction'
  | 'Thriller'
  | 'War'
  | 'Western';

export type PosterRecipe = {
  genre: PosterGenre;
  mood: string;
  focalSubjects: string[];
  settings: string[];
  lighting: string;
  palette: string;
  composition: string;
  titleTreatment: string;
  avoid: string[];
};

export type PosterPromptInput = {
  title: string;
  year?: number;
  genres?: string[];
  storyContext?: string;
  tagline?: string;
  hasVisualReferences?: boolean;
  assetType?: ArtworkAssetType;
  focus?: PosterFocus;
  subject?: string;
  textTreatment?: PosterTextTreatment;
  includeMatineeStyle?: boolean;
  visualTreatment?: PosterVisualTreatment;
  compositionStyle?: PosterComposition;
  focalSubject?: string;
  setting?: string;
  mood?: string;
  colorHint?: string;
  titleTreatment?: string;
  movieManifest?: MovieManifest | null;
};

export type PosterSourceMetadata = {
  Name: string;
  Overview?: string;
  Taglines?: string[];
  ProductionLocations?: string[];
  People?: Array<{ Name: string; Type?: string; Role?: string }>;
};

export type TitlePosterOptions = {
  specificFocalSubjects: string[];
  specificSettings: string[];
  focalSubjects: string[];
  settings: string[];
  byFocus: Record<Exclude<PosterFocus, 'Auto'>, string[]>;
};

export type ArtworkAssetType = 'Poster' | 'Backdrop' | 'Banner' | 'Thumbnail';
export type PosterFocus = 'Auto' | 'Character' | 'Signature Element' | 'Scene' | 'Environment';
export type PosterTextTreatment = 'Title' | 'Title + Tagline' | 'No Text';

export const artworkAssetTypes: Record<ArtworkAssetType, { description: string; direction: string; aspectRatio: string }> = {
  Poster: {
    description: 'Vertical collector artwork for library cards and details pages.',
    direction: 'Portrait orientation at a 2:3 aspect ratio. Reserve calm negative space near the lower third for optional typography.',
    aspectRatio: '2 / 3',
  },
  Backdrop: {
    description: 'Wide cinematic artwork for heroes and wallpaper.',
    direction: 'Wide landscape orientation at a 16:9 aspect ratio. Build environmental depth and keep important imagery away from the outer crop edges.',
    aspectRatio: '16 / 9',
  },
  Banner: {
    description: 'A restrained panoramic title treatment.',
    direction: 'Ultra-wide banner orientation at a 12:5 aspect ratio. Use one horizontal visual gesture, generous breathing room, and strong readability at a shallow height.',
    aspectRatio: '12 / 5',
  },
  Thumbnail: {
    description: 'Compact landscape artwork with strong small-size readability.',
    direction: 'Landscape orientation at a 16:9 aspect ratio. Keep the dominant subject large, simple, and readable at thumbnail size.',
    aspectRatio: '16 / 9',
  },
};

export const posterFocusOptions: Record<PosterFocus, string> = {
  Auto: 'Let Matinee choose the strongest symbolic idea.',
  Character: 'Center an illustrated, non-photorealistic character interpretation.',
  'Signature Element': 'Build around one iconic object or story symbol.',
  Scene: 'Reinterpret one memorable sequence as an original composition.',
  Environment: 'Let the story world and atmosphere carry the artwork.',
};

export const posterTextTreatments: Record<PosterTextTreatment, string> = {
  Title: 'Include only the exact movie title, set with elegant widely spaced display lettering.',
  'Title + Tagline': 'Include the exact movie title and one short supplied tagline; do not invent additional copy.',
  'No Text': 'Do not include any lettering, title, tagline, credits, logos, or typographic marks in the artwork.',
};

export type PosterVisualTreatment =
  | 'Matinee House Style'
  | 'Monumental Editorial'
  | 'Archival Ticket Print'
  | 'Graphic Geometry'
  | 'Expressive Ink Portrait'
  | 'Painterly Spectacle';

export type PosterComposition =
  | 'Genre Led'
  | 'Monument and Witness'
  | 'Solitary Figure'
  | 'Symbolic Geometry'
  | 'Split-Field Composition'
  | 'Cropped Character Study'
  | 'Landscape Threshold';

type PromptPreset = { description: string; direction: string };

export const defaultVisualTreatment: PosterVisualTreatment = 'Monumental Editorial';
export const defaultPosterComposition: PosterComposition = 'Monument and Witness';

export const posterVisualTreatments: Record<PosterVisualTreatment, PromptPreset> = {
  'Matinee House Style': {
    description: 'Warm, painterly, nostalgic neighborhood-cinema artwork.',
    direction: 'Use a premium painterly cinematic illustration with soft practical light, tactile film grain, handcrafted warmth, and restrained storybook elegance.',
  },
  'Monumental Editorial': {
    description: 'Extreme scale, negative space, and tactile editorial printmaking.',
    direction: 'Use a monumental editorial poster treatment inspired by premium mid-century print design: bold negative space, simplified geometry, dramatic vertical movement, lithographic ink, dry-brush texture, subtle paper grain, imperfect edges, and restrained screen-print color separation.',
  },
  'Archival Ticket Print': {
    description: 'Cream paper, dark ink, and a collectible theater-program finish.',
    direction: 'Render the artwork like a rare archival theater print on warm ticket-cream stock, using dark ink, worn letterpress texture, limited color registration, small amber accents, and elegant collectible-program restraint.',
  },
  'Graphic Geometry': {
    description: 'Iconic circles, portals, divided fields, and controlled symmetry.',
    direction: 'Reduce the story to bold graphic geometry, divided tonal fields, circles, portals, paths, or architectural shapes. Keep the imagery iconic and emotionally legible with precise negative space and tactile printed imperfections.',
  },
  'Expressive Ink Portrait': {
    description: 'An anonymous character study formed from ink and negative space.',
    direction: 'Use an expressive illustrated character study with a dramatically cropped profile or silhouette, broken ink edges, dry-brush shadows, paper showing through, and abundant negative space. Translate any referenced character into graphic illustration rather than photorealism.',
  },
  'Painterly Spectacle': {
    description: 'A sweeping environment with handcrafted scale and atmosphere.',
    direction: 'Create a sweeping painterly environment with one restrained human-scale anchor, layered atmospheric depth, visible brush texture, cinematic light, and a handcrafted illustrated finish rather than glossy concept art.',
  },
};

export const posterCompositions: Record<PosterComposition, PromptPreset> = {
  'Genre Led': {
    description: 'Use the composition supplied by the selected genre recipe.',
    direction: '',
  },
  'Monument and Witness': {
    description: 'One enormous story symbol with a tiny human-scale observer.',
    direction: 'Let one enormous symbolic subject or environment occupy roughly 55–75% of the frame. Place one small anonymous human silhouette as a witness to communicate scale, leaving deliberate negative space for typography.',
  },
  'Solitary Figure': {
    description: 'A lone silhouette surrounded by purposeful breathing room.',
    direction: 'Center or slightly offset one anonymous full-body silhouette, surrounded by a large field of atmospheric negative space. Let posture, light, and environment carry the emotion without a recognizable face.',
  },
  'Symbolic Geometry': {
    description: 'A single emblem organized around strong geometric structure.',
    direction: 'Organize the poster around one large symbolic geometric form with restrained symmetry, a clear visual axis, and small story details used only for scale and meaning.',
  },
  'Split-Field Composition': {
    description: 'Two contrasting visual fields connected by one story element.',
    direction: 'Divide the poster into two contrasting tonal or textural fields, joined by one central story symbol. Keep the division graphic, simple, and readable from thumbnail size.',
  },
  'Cropped Character Study': {
    description: 'An illustrated profile balanced against open negative space.',
    direction: 'Use a dramatically cropped illustrated profile or silhouette on one side of the frame, balanced by open negative space and one small symbolic counterpoint. Keep the treatment expressive and non-photorealistic.',
  },
  'Landscape Threshold': {
    description: 'A figure approaching an environment, boundary, or impossible event.',
    direction: 'Build the composition around a horizon, doorway, portal, shoreline, road, or architectural threshold, with a small figure approaching it and the environment carrying most of the narrative weight.',
  },
};

const matineePalette = Object.entries(matineePosterStyle.theme.palette)
  .map(([name, color]) => `${name}: ${color.hex} — ${color.role} (${color.usage})`);

const globalMatineeStyle = [
  matineePosterStyle.brand.creativeIntent,
  matineePosterStyle.artDirection.medium,
  `Print finish: ${matineePosterStyle.artDirection.finish.join(', ')}.`,
  matineePosterStyle.theme.colorStrategy,
  `House palette: ${matineePalette.join('; ')}.`,
  matineePosterStyle.theme.moviePalettePolicy.rule,
  matineePosterStyle.artDirection.composition.primaryRule,
  `Use ${matineePosterStyle.artDirection.composition.negativeSpace} negative space. ${matineePosterStyle.artDirection.composition.thumbnailReadability}`,
  matineePosterStyle.artDirection.lighting.default,
  matineePosterStyle.artDirection.lighting.rule,
];

const globalRestrictions = matineePosterStyle.artDirection.restrictions;

export const posterRecipes: Record<PosterGenre, PosterRecipe> = {
  Action: {
    genre: 'Action',
    mood: 'tense, bold, kinetic, controlled',
    focalSubjects: ['a lone figure in motion', 'a damaged vehicle', 'a symbolic weapon', 'a city street under pressure'],
    settings: ['rainy streets', 'industrial corridors', 'night highways', 'smoke-filled rooftops'],
    lighting: 'hard amber rim light cutting through smoke or rain',
    palette: 'steel blue, ember orange, charcoal, muted crimson',
    composition: 'single strong diagonal or centered silhouette with restrained motion cues',
    titleTreatment: 'heavy condensed serif or engraved block serif title',
    avoid: ['explosions filling the frame', 'tactical gear clutter', 'generic action collage'],
  },
  Adventure: {
    genre: 'Adventure',
    mood: 'curious, open, hopeful, windswept',
    focalSubjects: ['a trail marker', 'a small traveler silhouette', 'a map', 'a distant landmark'],
    settings: ['mountain passes', 'forest roads', 'desert overlooks', 'misty coastlines'],
    lighting: 'golden-hour horizon light with soft atmospheric depth',
    palette: 'marquee amber, moss green, faded teal, warm stone',
    composition: 'wide scenic depth with one clear foreground symbol',
    titleTreatment: 'storybook serif title with subtle weathered texture',
    avoid: ['theme-park spectacle', 'overcrowded vistas', 'oversaturated travel poster colors'],
  },
  Animation: {
    genre: 'Animation',
    mood: 'playful, gentle, bright, storybook',
    focalSubjects: ['a charming object', 'a small character silhouette', 'a glowing doorway', 'a handmade toy'],
    settings: ['cozy bedrooms', 'painted neighborhoods', 'sunlit parks', 'whimsical interiors'],
    lighting: 'soft morning or lamplight glow with rounded shadows',
    palette: 'ticket cream, warm amber, faded teal, soft coral',
    composition: 'simple graphic framing with rounded forms and clear readable shapes',
    titleTreatment: 'friendly serif title with handmade warmth',
    avoid: ['plastic 3D render look', 'hyperactive colors', 'toy-commercial clutter'],
  },
  Comedy: {
    genre: 'Comedy',
    mood: 'warm, awkward, human, bright',
    focalSubjects: ['a misplaced object', 'an empty chair', 'a dinner table detail', 'a single comic silhouette'],
    settings: ['apartments', 'diners', 'suburban streets', 'workplace corners'],
    lighting: 'warm practical light with clean readable faces or silhouettes',
    palette: 'ticket cream, amber, faded teal, soft burgundy',
    composition: 'centered visual joke or object with lots of breathing room',
    titleTreatment: 'elegant serif title with a slightly playful rhythm',
    avoid: ['wacky collage expressions', 'loud sitcom poster styling', 'cheap gag typography'],
  },
  Crime: {
    genre: 'Crime',
    mood: 'noir, restrained, suspicious, elegant',
    focalSubjects: ['a clue on a table', 'a lone car at night', 'a window silhouette', 'a city map detail'],
    settings: ['dim offices', 'wet alleys', 'motel rooms', 'late-night city blocks'],
    lighting: 'slatted noir light, practical lamps, and deep navy shadows',
    palette: 'midnight navy, amber, tobacco brown, muted red',
    composition: 'asymmetric noir framing with one evidence-like focal point',
    titleTreatment: 'sharp serif title with quiet noir spacing',
    avoid: ['police tape overload', 'weapon-first sensationalism', 'gritty over-sharpening'],
  },
  Documentary: {
    genre: 'Documentary',
    mood: 'observational, honest, thoughtful, grounded',
    focalSubjects: ['an archival object', 'a landscape detail', 'a worktable', 'a single symbolic artifact'],
    settings: ['real environments rendered as painterly scenes', 'archives', 'workshops', 'quiet public spaces'],
    lighting: 'natural window light or soft practical light',
    palette: 'ticket cream, charcoal, muted amber, faded teal',
    composition: 'editorial still-life framing with documentary restraint',
    titleTreatment: 'clean serif title with understated subtitle space',
    avoid: ['fake news graphics', 'sensational montage', 'overdramatic lighting'],
  },
  Drama: {
    genre: 'Drama',
    mood: 'intimate, reflective, emotionally warm',
    focalSubjects: ['a dinner table', 'a window-lit room', 'a keepsake object', 'a solitary figure'],
    settings: ['living rooms', 'backyards', 'quiet streets', 'bedrooms at dusk'],
    lighting: 'soft amber lamplight or late-afternoon window light',
    palette: 'midnight navy, ticket cream, amber, burgundy, faded teal',
    composition: 'quiet centered subject with room for silence and title',
    titleTreatment: 'elegant serif title, premium literary spacing',
    avoid: ['melodramatic tears', 'soap-opera lighting', 'busy family collage'],
  },
  Family: {
    genre: 'Family',
    mood: 'tender, nostalgic, safe, wonder-filled',
    focalSubjects: ['a backyard light', 'a family table', 'a childlike keepsake', 'a glowing window'],
    settings: ['homes at dusk', 'tree-lined streets', 'parks', 'cozy kitchens'],
    lighting: 'golden domestic light with gentle bloom',
    palette: 'warm amber, ticket cream, faded teal, soft green',
    composition: 'centered symbol of belonging, small human scale',
    titleTreatment: 'warm serif title with storybook softness',
    avoid: ['saccharine greeting-card imagery', 'cartoonish family pileups', 'overly glossy smiles'],
  },
  Fantasy: {
    genre: 'Fantasy',
    mood: 'enchanted, ancient, luminous, mysterious',
    focalSubjects: ['a glowing relic', 'a doorway', 'a crown', 'a tree or tower silhouette'],
    settings: ['old forests', 'candlelit halls', 'misty ruins', 'storybook villages'],
    lighting: 'magical amber glow grounded by deep navy shadows',
    palette: 'midnight navy, antique gold, moss, faded teal',
    composition: 'mythic centered icon with painterly atmosphere',
    titleTreatment: 'ornate but readable serif title',
    avoid: ['generic dragon chaos', 'high-gloss game art', 'overdesigned fantasy runes'],
  },
  Horror: {
    genre: 'Horror',
    mood: 'quiet dread, eerie, restrained, haunted',
    focalSubjects: ['a half-open door', 'a candle', 'an empty chair', 'a distant silhouette'],
    settings: ['old houses', 'dark woods', 'basements', 'fogged windows'],
    lighting: 'low amber practical light swallowed by blue-black shadow',
    palette: 'midnight navy, sickly amber, bone cream, dried burgundy',
    composition: 'large negative space with one unsettling focal point',
    titleTreatment: 'elegant serif title with subtle unease',
    avoid: ['gore', 'jump-scare faces', 'monster closeups', 'cheap slasher poster tropes'],
  },
  Music: {
    genre: 'Music',
    mood: 'rhythmic, intimate, luminous, soulful',
    focalSubjects: ['a microphone', 'a stage light', 'a record', 'an instrument silhouette'],
    settings: ['small theaters', 'recording rooms', 'backstage corners', 'nightclubs'],
    lighting: 'warm stage practicals and soft haze',
    palette: 'marquee amber, midnight navy, burgundy, brass',
    composition: 'spotlit musical object or performer silhouette with negative space',
    titleTreatment: 'serif title with concert-bill elegance',
    avoid: ['festival poster chaos', 'celebrity likenesses', 'overloaded music notes'],
  },
  Mystery: {
    genre: 'Mystery',
    mood: 'curious, rain-soaked, intimate, suspenseful',
    focalSubjects: ['a key', 'a magnifying glass', 'a book', 'a lit window'],
    settings: ['libraries', 'rainy streets', 'old hotels', 'train compartments'],
    lighting: 'lamplight through rain or fog, soft edge highlights',
    palette: 'midnight navy, amber, faded teal, oxblood',
    composition: 'single clue-like object in a centered or slightly off-center frame',
    titleTreatment: 'classic serif title with literary mystery restraint',
    avoid: ['detective cliches piled together', 'crime-scene clutter', 'harsh thriller grit'],
  },
  Romance: {
    genre: 'Romance',
    mood: 'soft, longing, nostalgic, intimate',
    focalSubjects: ['two cups on a table', 'a handwritten note', 'a theater seat', 'a rainy window'],
    settings: ['dusk streets', 'cozy cafes', 'train stations', 'warm apartments'],
    lighting: 'soft amber window light with gentle haze',
    palette: 'ticket cream, amber, dusty rose, faded teal',
    composition: 'symbolic intimacy without floating heads or posed glamour',
    titleTreatment: 'romantic serif title, elegant and readable',
    avoid: ['glossy rom-com collage', 'stock-photo couples', 'oversweet pink palettes'],
  },
  'Science Fiction': {
    genre: 'Science Fiction',
    mood: 'awe, isolation, cerebral wonder',
    focalSubjects: ['a spacecraft silhouette', 'a portal', 'a planet horizon', 'a strange artifact'],
    settings: ['quiet space vistas', 'minimal control rooms', 'alien coastlines', 'moonlit cities'],
    lighting: 'cool cosmic glow balanced with warm human practical light',
    palette: 'midnight navy, faded teal, pale amber, cool white',
    composition: 'large negative space with one iconic technological or cosmic symbol',
    titleTreatment: 'elegant serif title with restrained futuristic spacing',
    avoid: ['neon cyberpunk overload', 'busy spaceship battles', 'generic laser action'],
  },
  Thriller: {
    genre: 'Thriller',
    mood: 'taut, paranoid, shadowed, urgent',
    focalSubjects: ['a phone', 'a train platform', 'a hallway light', 'a lone figure watched from afar'],
    settings: ['parking garages', 'subway stations', 'hotel corridors', 'night streets'],
    lighting: 'thin amber light against deep shadows, high contrast but not harsh',
    palette: 'charcoal, midnight navy, warning amber, muted red',
    composition: 'compressed negative space and one suspenseful focal subject',
    titleTreatment: 'tight serif title with controlled tension',
    avoid: ['generic gun-and-face layouts', 'oversharpened realism', 'explosive action styling'],
  },
  War: {
    genre: 'War',
    mood: 'somber, human, weathered, reverent',
    focalSubjects: ['a helmet', 'a letter', 'a field marker', 'a distant soldier silhouette'],
    settings: ['foggy fields', 'barracks interiors', 'ruined streets', 'quiet shorelines'],
    lighting: 'muted dawn light, smoke haze, restrained amber highlights',
    palette: 'olive drab, charcoal, ticket cream, muted amber',
    composition: 'respectful symbolic framing with human scale',
    titleTreatment: 'classic serif title with memorial restraint',
    avoid: ['battle chaos spectacle', 'flag-waving propaganda', 'graphic violence'],
  },
  Western: {
    genre: 'Western',
    mood: 'lonely, sun-worn, mythic, quiet',
    focalSubjects: ['a hat on a table', 'a horse silhouette', 'a dusty road', 'a weathered doorway'],
    settings: ['desert towns', 'sunset plains', 'wooden interiors', 'rail stops'],
    lighting: 'low desert sun, long shadows, warm dust haze',
    palette: 'burnt amber, charcoal, faded teal sky, dry ochre',
    composition: 'wide negative space with a solitary icon or silhouette',
    titleTreatment: 'weathered serif title, elegant rather than rustic novelty',
    avoid: ['cartoon cowboy tropes', 'overdone sepia', 'shootout clutter'],
  },
};

const genreAliases: Record<string, PosterGenre> = {
  'sci-fi': 'Science Fiction',
  scifi: 'Science Fiction',
  sciencefiction: 'Science Fiction',
  science: 'Science Fiction',
  kids: 'Family',
  children: 'Family',
  musical: 'Music',
  suspense: 'Thriller',
};

function normalizeGenre(value: string) {
  return value.toLowerCase().replace(/[^a-z]/g, '');
}

function unique(values: string[]) {
  const seen = new Set<string>();
  return values.filter((value) => {
    const normalized = value.trim().toLowerCase();
    if (!normalized || seen.has(normalized)) return false;
    seen.add(normalized);
    return true;
  });
}

function concise(value: string, maximum = 84) {
  const normalized = value.replace(/[\u0000-\u001f\u007f]/g, ' ').replace(/\s+/g, ' ').trim().replace(/[.;,:]+$/, '');
  return normalized.length <= maximum ? normalized : `${normalized.slice(0, maximum).replace(/\s+\S*$/, '')}…`;
}

function exactText(value: string, maximum = 160) {
  const normalized = value.replace(/[\u0000-\u001f\u007f]/g, ' ').replace(/\s+/g, ' ').trim();
  return normalized.length <= maximum ? normalized : `${normalized.slice(0, maximum).replace(/\s+\S*$/, '')}…`;
}

function namedStoryElements(overview = '') {
  return unique(Array.from(overview.matchAll(/\b[A-Z][a-z'’-]+(?:\s+[A-Z][a-z'’-]+){1,3}\b/g), (match) => match[0]))
    .filter((value) => !/^(United States|New York Times|World War)$/i.test(value))
    .slice(0, 3);
}

function storySettings(overview = '') {
  const environments: string[] = [];
  const periods: string[] = [];

  const pattern = /\b(?:in|inside|within|across|through|at|near|beneath|aboard|around|along)\s+((?:(?:the|an?|his|her|their)\s+)?[A-Za-z0-9'’-]+(?:\s+[A-Za-z0-9'’-]+){0,6})/gi;
  for (const match of overview.matchAll(pattern)) {
    const phrase = concise(match[1]
      .split(/\s+(?:who|where|when|while|after|before|because|but|and|with|must|has|have|is|are|was|were)\b/i)[0], 64);
    if (/^(?:18|19|20)\d{2}$/.test(phrase)) {
      periods.push(`a period-authentic ${phrase} story-world environment`);
    } else if (phrase.split(/\s+/).length > 1) {
      environments.push(phrase);
    }
    if (environments.length + periods.length >= 5) break;
  }
  const years = overview.match(/\b(?:18|19|20)\d{2}\b/g) || [];
  periods.push(...years.slice(0, 2).map((year) => `a period-authentic ${year} story-world environment`));
  return unique([...environments, ...periods]);
}

export function getTitlePosterOptions(
  item: PosterSourceMetadata,
  genre: PosterGenre = getPosterRecipe([]).genre,
  movieManifest?: MovieManifest | null,
): TitlePosterOptions {
  const recipe = posterRecipes[genre];
  const manifestOptions = getMovieManifestFocusOptions(movieManifest);
  const roles = unique((item.People || [])
    .filter((person) => person.Role && (!person.Type || person.Type.toLowerCase() === 'actor'))
    .map((person) => person.Role!.trim())
    .filter((role) => !/^(self|himself|herself|themselves|narrator)$/i.test(role)))
    .slice(0, 4);
  const boundedOverview = concise(item.Overview || '', 4_000);
  const boundedTitle = concise(item.Name, 240);
  const storyElements = namedStoryElements(boundedOverview);
  const tagline = item.Taglines?.map((value) => concise(value)).find(Boolean);

  const specificFocalSubjects = unique([
    ...roles.map((role) => `a stylized illustrated portrait, silhouette, or keepsake associated with ${role}`),
    ...storyElements.map((element) => `a single emblematic object associated with ${element}`),
    ...(tagline ? [`a visual metaphor for “${tagline}”`] : []),
    `a unique central symbol drawn from the story of ${boundedTitle}`,
  ]).slice(0, 7);
  const specificSettings = unique([
    ...storySettings(boundedOverview),
    ...(item.ProductionLocations || []).slice(0, 3).map((location) => `a cinematic story-world landscape inspired by ${location}`),
    `an atmospheric location drawn specifically from the story of ${boundedTitle}`,
  ]).slice(0, 7);
  const fallbackCharacters = roles.length ? roles : [`A central character from ${boundedTitle}`];
  const fallbackSignatureElements = unique([
    ...storyElements,
    ...(tagline ? [`A visual metaphor for “${tagline}”`] : []),
    ...recipe.focalSubjects,
  ]).slice(0, 8);
  const fallbackScenes = unique([
    ...storySettings(boundedOverview),
    `A defining story moment from ${boundedTitle}`,
    ...recipe.settings.map((value) => `A story moment set in ${value}`),
  ]).slice(0, 8);
  const fallbackEnvironments = unique([...specificSettings, ...recipe.settings]).slice(0, 8);
  const hasManifest = Boolean(movieManifest);
  const characters = manifestOptions.Character.length ? manifestOptions.Character : fallbackCharacters;
  const signatureElements = manifestOptions['Signature Element'].length
    ? manifestOptions['Signature Element']
    : fallbackSignatureElements;
  const scenes = manifestOptions.Scene.length ? manifestOptions.Scene : fallbackScenes;
  const environments = manifestOptions.Environment.length
    ? manifestOptions.Environment
    : fallbackEnvironments;
  const manifestSubjects = unique([
    ...characters,
    ...signatureElements,
    ...(movieManifest?.artworkBrief?.primarySymbols || []),
  ]);
  const manifestSettings = unique([
    ...scenes,
    ...environments,
  ]);

  return {
    specificFocalSubjects: hasManifest ? manifestSubjects : specificFocalSubjects,
    specificSettings: hasManifest ? manifestSettings : specificSettings,
    focalSubjects: hasManifest ? manifestSubjects : unique([...specificFocalSubjects, ...recipe.focalSubjects]),
    settings: hasManifest ? manifestSettings : unique([...specificSettings, ...recipe.settings]),
    byFocus: {
      Character: characters,
      'Signature Element': signatureElements,
      Scene: scenes,
      Environment: environments,
    },
  };
}

export function getPosterRecipe(genres: string[] = []) {
  for (const genre of genres) {
    const normalized = normalizeGenre(genre);
    const match = Object.keys(posterRecipes).find((key) => normalizeGenre(key) === normalized) as PosterGenre | undefined;
    if (match) return posterRecipes[match];
    const alias = genreAliases[normalized];
    if (alias) return posterRecipes[alias];
  }
  return posterRecipes.Drama;
}

function automaticTreatment(focus: PosterFocus): PosterVisualTreatment {
  if (focus === 'Character') return 'Expressive Ink Portrait';
  if (focus === 'Signature Element') return 'Graphic Geometry';
  if (focus === 'Scene' || focus === 'Environment') return 'Painterly Spectacle';
  return defaultVisualTreatment;
}

function automaticComposition(focus: PosterFocus): PosterComposition {
  if (focus === 'Character') return 'Cropped Character Study';
  if (focus === 'Signature Element') return 'Symbolic Geometry';
  if (focus === 'Scene' || focus === 'Environment') return 'Landscape Threshold';
  return defaultPosterComposition;
}

function focusDirection(focus: PosterFocus, subject: string | undefined, title: string) {
  if (focus === 'Character') {
    return `Character focus: create a stylized illustrated interpretation associated with ${subject || `a central character from ${title}`}. Use silhouette, posture, costume color rhythm, or a symbolic keepsake; do not reproduce an actor's face or exact branded costume.`;
  }
  if (focus === 'Signature Element') {
    return `Signature-element focus: make ${subject || `one unmistakable symbol from ${title}`} the single dominant visual metaphor. Simplify it into original graphic geometry rather than reproducing branded iconography.`;
  }
  if (focus === 'Scene') {
    return `Scene focus: reinterpret ${subject || `a defining moment from ${title}`} as a new illustrated composition. Preserve its emotional idea without recreating a frame or official poster.`;
  }
  if (focus === 'Environment') {
    return `Environment focus: let ${subject || `the distinctive story world of ${title}`} carry the image, using a small anonymous human-scale anchor only when useful.`;
  }
  return `Automatic focus: choose the single clearest symbolic idea from ${title}${subject ? `, using ${subject} as the lead candidate` : ''}. Prefer an object, environment, silhouette, or visual metaphor over a recognizable face or multi-character collage.`;
}

function outputInstructions(assetType: ArtworkAssetType) {
  return [
    `Create one finished ${assetType.toLowerCase()} asset for the Matinee streaming interface.`,
    artworkAssetTypes[assetType].direction,
    'Deliver flat artwork only—not a framed print, physical mockup, interface, grid, or mood board.',
    'Produce one final image. Do not create alternates, contact sheets, or multiple panels.',
  ];
}

function matineeStyleSection() {
  return [
    `MATINEE HOUSE LANGUAGE · ${matineePosterStyle.styleVersion}`,
    ...globalMatineeStyle.map((line) => `- ${line}`),
  ];
}

function promptList(label: string, values: string[] = [], maximum = 10) {
  const entries = unique(values).slice(0, maximum).map((value) => concise(value, 220));
  return entries.length ? [`- ${label}: ${entries.join(' | ')}.`] : [];
}

function movieManifestSection(manifest?: MovieManifest | null) {
  if (!manifest) return [];
  const official = manifest.official;
  const context = manifest.creativeContext;
  const brief = manifest.artworkBrief;
  return [
    '',
    `MOVIE-SPECIFIC CREATIVE MANIFEST · v${manifest.manifestVersion}`,
    '- Treat this local movie manifest as the source of truth for what belongs to the selected title.',
    ...promptList('Official genres', official?.genres),
    ...promptList('Themes', context?.themes),
    ...promptList('Production context', context?.productionContext),
    ...promptList('Characters', context?.characters),
    ...promptList('Locations', context?.locations),
    ...promptList('Signature objects', context?.signatureObjects),
    ...promptList('Artifacts', context?.artifacts),
    ...promptList('Vehicles', context?.vehicles),
    ...promptList('Organizations', context?.organizations),
    ...promptList('Iconic scenes', context?.iconicScenes),
    ...promptList('Visual motifs', context?.visualMotifs),
    ...promptList('Primary symbols', brief?.primarySymbols),
    ...promptList('Movie palette hints', brief?.paletteHints),
    ...promptList('Movie composition hints', brief?.compositionHints),
    ...promptList('Avoid spoilers', brief?.avoidSpoilers),
    ...promptList('Movie-specific exclusions', brief?.negativePrompts),
    ...(official?.tagline ? [`- Official tagline: “${exactText(official.tagline)}”`] : []),
    '- Movie facts control what is depicted. The locked Matinee house manifest controls how it is depicted.',
  ];
}

function matineeTypographySection(textTreatment: PosterTextTreatment) {
  const families = matineePosterStyle.typography.families;
  const hierarchy = matineePosterStyle.typography.posterHierarchy;
  return [
    posterTextTreatments[textTreatment],
    `Title system: ${families.display.name} ${families.display.weight}; ${hierarchy.title.case}; no more than ${hierarchy.title.maximumLines} lines.`,
    `Tagline system: ${families.interface.name} ${hierarchy.tagline.weight}; ${hierarchy.tagline.case}; no more than ${hierarchy.tagline.maximumLines} lines.`,
    `Metadata system, only when explicitly enabled: ${families.metadata.name} ${hierarchy.metadata.weight}.`,
    ...matineePosterStyle.typography.layoutRules.map((rule) => `- ${rule}`),
  ];
}

function technicalEnding(assetType: ArtworkAssetType) {
  return [
    'OUTPUT REQUIREMENTS',
    `- ${artworkAssetTypes[assetType].direction}`,
    '- Produce premium collector artwork that feels timeless, cinematic, elegant, and collectible.',
    '- Communicate the selected title through symbolic visual storytelling rather than literal scene recreation.',
    '- Make exactly one image-generation attempt. If generation is rejected or fails, stop and return the error; do not retry and do not construct a local fallback image.',
  ];
}

export function buildCustomArtworkPrompt(input: {
  customPrompt: string;
  assetType?: ArtworkAssetType;
  includeMatineeStyle?: boolean;
}) {
  const assetType = input.assetType ?? 'Poster';
  return [
    'CUSTOM CREATIVE BRIEF',
    input.customPrompt.trim(),
    ...(input.includeMatineeStyle === false ? [] : ['', ...matineeStyleSection()]),
    '',
    ...technicalEnding(assetType),
  ].join('\n');
}

export function buildPosterPrompt(input: PosterPromptInput) {
  const title = concise(input.title, 240);
  const recipe = getPosterRecipe(input.genres);
  const manifestOptions = getMovieManifestFocusOptions(input.movieManifest);
  const assetType = input.assetType ?? 'Poster';
  const focus = input.focus ?? 'Auto';
  const textTreatment = input.textTreatment ?? 'Title';
  const visualTreatment = input.visualTreatment ?? automaticTreatment(focus);
  const compositionStyle = input.compositionStyle ?? automaticComposition(focus);
  const focalSubject = concise(input.subject
    ?? input.focalSubject
    ?? input.movieManifest?.artworkBrief?.primarySymbols?.[0]
    ?? manifestOptions['Signature Element'][0]
    ?? recipe.focalSubjects[0], 500);
  const setting = concise(input.setting
    ?? input.movieManifest?.creativeContext?.locations?.[0]
    ?? manifestOptions.Environment[0]
    ?? recipe.settings[0], 500);
  const mood = concise(input.mood
    ?? input.movieManifest?.creativeContext?.themes?.slice(0, 4).join(', ')
    ?? recipe.mood, 700);
  const palette = concise(input.colorHint
    ?? input.movieManifest?.artworkBrief?.paletteHints?.join('; ')
    ?? recipe.palette, 900);
  const titleTreatment = concise(input.titleTreatment ?? recipe.titleTreatment, 300);
  const compositionLines = compositionStyle === 'Genre Led'
    ? [`Genre-led composition: ${recipe.composition}.`]
    : [
      `Selected composition — ${compositionStyle}: ${posterCompositions[compositionStyle].direction}`,
      `Supporting genre rhythm, only where compatible: ${recipe.composition}.`,
    ];
  const referenceLines = input.hasVisualReferences
    ? [
      '',
      'VISUAL REFERENCES',
      '- Jellyfin backdrop stills from this exact title are attached as source material.',
      '- Use the stills to understand story-specific colors, silhouettes, props, architecture, environments, and production design.',
      `- Transform those details through the selected Matinee print treatment and create a new composition for the ${assetType.toLowerCase()} format; do not simply crop, trace, filter, or reproduce a source frame.`,
      '- Do not reproduce recognizable actor faces, official poster layouts, exact franchise logos, or branded costume details.',
    ]
    : [];

  return [
    'OUTPUT',
    ...outputInstructions(assetType).map((line) => `- ${line}`),
    '',
    ...(input.includeMatineeStyle === false ? [] : matineeStyleSection()),
    '',
    'STORY',
    `Title: "${title}".`,
    ...(input.year ? [`Year: ${input.year}.`] : []),
    `Genre recipe: ${recipe.genre}.`,
    ...(input.storyContext ? [`Story context from the user's Jellyfin library: ${concise(input.storyContext, 520)}`] : []),
    ...(input.storyContext ? [input.hasVisualReferences
      ? 'Story interpretation: Use the attached stills for title accuracy, then reinterpret the story through an original illustrated composition.'
      : 'Story interpretation: Treat this context symbolically; do not invent a celebrity likeness or reproduce an existing scene composition.'] : []),
    ...movieManifestSection(input.movieManifest),
    focusDirection(focus, focalSubject, title),
    `Supporting environment: ${setting}.`,
    `Emotional direction: ${mood}.`,
    ...referenceLines,
    '',
    'VISUAL TREATMENT',
    `Selected treatment — ${visualTreatment}: ${posterVisualTreatments[visualTreatment].direction}`,
    `Lighting: ${recipe.lighting}.`,
    `Story palette: ${palette}. Interpret it within the Matinee house-color hierarchy above.`,
    '',
    'COMPOSITION',
    ...compositionLines,
    '',
    'TYPOGRAPHY',
    ...matineeTypographySection(textTreatment),
    ...(textTreatment === 'Title + Tagline' && (input.movieManifest?.official?.tagline || input.tagline)
      ? [`Use this exact tagline: “${exactText(input.movieManifest?.official?.tagline || input.tagline || '', 120)}”`]
      : []),
    ...(textTreatment !== 'No Text' ? [`Use ${titleTreatment}.`] : []),
    'Do not include credits, a billing block, actor names, director attribution, studio marks, or imitation franchise typography.',
    '',
    'RESTRICTIONS',
    ...globalRestrictions.map((line) => `- ${line}`),
    ...(!input.hasVisualReferences ? ['- Do not invent or reproduce celebrity likenesses or recognizable actor faces without a supplied title reference image.'] : []),
    `- Genre-specific avoid list: ${recipe.avoid.join(', ')}.`,
    '',
    ...technicalEnding(assetType),
  ].join('\n');
}

export const posterPromptGuide = {
  styleManifest: matineePosterStyle,
  globalMatineeStyle,
  restrictions: globalRestrictions,
  visualTreatments: posterVisualTreatments,
  compositions: posterCompositions,
  assetTypes: artworkAssetTypes,
  focuses: posterFocusOptions,
  textTreatments: posterTextTreatments,
  recipes: posterRecipes,
};
