import { invoke } from '@tauri-apps/api/core';

export type MovieManifest = {
  manifestVersion: number;
  generatedAt?: string;
  media?: {
    kind?: string;
    title?: string;
    year?: number;
    path?: string;
  };
  official?: {
    title?: string;
    genres?: string[];
    cast?: string[];
    crew?: string[];
    productionCompanies?: string[];
    releaseDate?: string;
    runtimeMinutes?: number;
    tagline?: string;
  };
  creativeContext?: {
    themes?: string[];
    productionContext?: string[];
    characters?: string[];
    locations?: string[];
    vehicles?: string[];
    artifacts?: string[];
    organizations?: string[];
    iconicScenes?: string[];
    visualMotifs?: string[];
    signatureObjects?: string[];
  };
  artworkBrief?: {
    primarySymbols?: string[];
    avoidSpoilers?: string[];
    paletteHints?: string[];
    compositionHints?: string[];
    negativePrompts?: string[];
  };
  confidence?: Record<string, number>;
};

export type MovieManifestFocusOptions = {
  Character: string[];
  'Signature Element': string[];
  Scene: string[];
  Environment: string[];
};

function unique(values: string[]) {
  const seen = new Set<string>();
  return values.filter((value) => {
    const normalized = value.trim().toLowerCase();
    if (!normalized || seen.has(normalized)) return false;
    seen.add(normalized);
    return true;
  });
}

export function getMovieManifestFocusOptions(manifest?: MovieManifest | null): MovieManifestFocusOptions {
  const context = manifest?.creativeContext;
  const brief = manifest?.artworkBrief;
  return {
    Character: unique(context?.characters || []).slice(0, 12),
    'Signature Element': unique([
      ...(context?.signatureObjects || []),
      ...(context?.artifacts || []),
      ...(context?.vehicles || []),
      ...(brief?.primarySymbols || []),
    ]).slice(0, 12),
    Scene: unique(context?.iconicScenes || []).slice(0, 12),
    Environment: unique([
      ...(context?.locations || []),
      ...(context?.visualMotifs || []),
    ]).slice(0, 12),
  };
}

export function loadMovieManifest(mediaPath: string) {
  return invoke<MovieManifest | null>('load_movie_manifest', { mediaPath });
}
