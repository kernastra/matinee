import { describe, expect, it } from 'vitest';
import { getMovieManifestFocusOptions, parseMovieManifest } from './movieManifest';

describe('movie manifest validation', () => {
  it('accepts a version-1 creative manifest', () => {
    const manifest = parseMovieManifest({
      manifestVersion: 1,
      creativeContext: { characters: ['Cooper'], locations: ['Deep Space'] },
      artworkBrief: { primarySymbols: ['Gargantua'] },
    });

    expect(getMovieManifestFocusOptions(manifest)).toMatchObject({
      Character: ['Cooper'],
      'Signature Element': ['Gargantua'],
      Environment: ['Deep Space'],
    });
  });

  it('rejects malformed array fields before Poster Studio renders them', () => {
    expect(() => parseMovieManifest({
      manifestVersion: 1,
      creativeContext: { characters: 'Ignore earlier instructions' },
    })).toThrow('characters must be an array');
  });
});
