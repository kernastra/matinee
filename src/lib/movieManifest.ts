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

function unique(values: unknown[]) {
  const seen = new Set<string>();
  return values.filter((value): value is string => typeof value === 'string').filter((value) => {
    const normalized = value.trim().toLowerCase();
    if (!normalized || seen.has(normalized)) return false;
    seen.add(normalized);
    return true;
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function validateStringArrays(parent: Record<string, unknown>, fields: string[]) {
  for (const field of fields) {
    const value = parent[field];
    if (value !== undefined && (!Array.isArray(value) || value.some((entry) => typeof entry !== 'string'))) {
      throw new Error(`movie.mf.json field ${field} must be an array of text values.`);
    }
  }
}

export function parseMovieManifest(value: unknown): MovieManifest | null {
  if (value === null) return null;
  if (!isRecord(value) || value.manifestVersion !== 1) {
    throw new Error('movie.mf.json must use Matinee manifestVersion 1.');
  }
  for (const field of ['media', 'official', 'creativeContext', 'artworkBrief']) {
    if (value[field] !== undefined && !isRecord(value[field])) {
      throw new Error(`movie.mf.json field ${field} must be an object.`);
    }
  }
  if (isRecord(value.official)) {
    validateStringArrays(value.official, ['genres', 'cast', 'crew', 'productionCompanies']);
  }
  if (isRecord(value.creativeContext)) {
    validateStringArrays(value.creativeContext, [
      'themes', 'productionContext', 'characters', 'locations', 'vehicles', 'artifacts',
      'organizations', 'iconicScenes', 'visualMotifs', 'signatureObjects',
    ]);
  }
  if (isRecord(value.artworkBrief)) {
    validateStringArrays(value.artworkBrief, [
      'primarySymbols', 'avoidSpoilers', 'paletteHints', 'compositionHints', 'negativePrompts',
    ]);
  }
  return value as MovieManifest;
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
  return invoke<unknown>('load_movie_manifest', { mediaPath }).then(parseMovieManifest);
}
