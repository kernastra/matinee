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
  genres?: string[];
  focalSubject?: string;
  setting?: string;
  mood?: string;
  colorHint?: string;
  titleTreatment?: string;
};

const globalMatineeStyle = [
  'Create a vertical 2:3 movie poster card for the Matinee streaming interface.',
  'Use a warm, nostalgic, cozy, premium cinematic illustration style with subtle film grain.',
  'Favor soft amber practical light, midnight navy shadows, ticket cream typography, restrained burgundy, and faded teal accents.',
  'Use one clear focal subject, readable silhouette, elegant title placement, minimal supporting text, and generous negative space.',
  'Do not recreate official poster artwork, celebrity likenesses, studio logos, franchise branding, busy floating-head collages, photorealistic stills, neon cyberpunk palettes, or tiny illegible text.',
];

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

export function buildPosterPrompt(input: PosterPromptInput) {
  const recipe = getPosterRecipe(input.genres);
  const focalSubject = input.focalSubject ?? recipe.focalSubjects[0];
  const setting = input.setting ?? recipe.settings[0];
  const mood = input.mood ?? recipe.mood;
  const palette = input.colorHint ?? recipe.palette;
  const titleTreatment = input.titleTreatment ?? recipe.titleTreatment;

  return [
    ...globalMatineeStyle,
    `Title: "${input.title}".`,
    `Genre recipe: ${recipe.genre}.`,
    `Show ${focalSubject} in ${setting} with a ${mood} mood.`,
    `Lighting: ${recipe.lighting}.`,
    `Palette: ${palette}.`,
    `Composition: ${recipe.composition}.`,
    `Typography: ${titleTreatment}; include only the movie title and minimal supporting text if needed.`,
    `Genre-specific avoid list: ${recipe.avoid.join(', ')}.`,
  ].join('\n');
}

export const posterPromptGuide = {
  globalMatineeStyle,
  recipes: posterRecipes,
};
