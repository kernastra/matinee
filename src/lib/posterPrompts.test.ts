import { describe, expect, it } from 'vitest';
import { buildCustomArtworkPrompt, buildPosterPrompt, getPosterRecipe, getTitlePosterOptions } from './posterPrompts';

describe('poster prompt templates', () => {
  it('selects the first known genre recipe from Jellyfin genres', () => {
    expect(getPosterRecipe(['IMAX', 'Science Fiction', 'Adventure']).genre).toBe('Science Fiction');
  });

  it('supports common genre aliases', () => {
    expect(getPosterRecipe(['Sci-Fi']).genre).toBe('Science Fiction');
    expect(getPosterRecipe(['Kids']).genre).toBe('Family');
  });

  it('falls back to drama when no specific genre matches', () => {
    expect(getPosterRecipe(['Special Interest']).genre).toBe('Drama');
  });

  it('builds a complete prompt with global style and genre guidance', () => {
    const prompt = buildPosterPrompt({
      title: 'The Last Signal',
      genres: ['Thriller'],
      focus: 'Signature Element',
      subject: 'a glowing phone on a diner table',
    });

    expect(prompt).toContain('Title: "The Last Signal".');
    expect(prompt).toContain('Genre recipe: Thriller.');
    expect(prompt).toContain('a glowing phone on a diner table');
    expect(prompt).toContain('Do not recreate official key art');
    expect(prompt).toContain('celebrity likenesses');
  });

  it('derives title-specific subjects and settings from Jellyfin metadata', () => {
    const options = getTitlePosterOptions({
      Name: 'The Last Signal',
      Overview: 'In 1987, Mara Vale follows a mysterious transmission through an abandoned desert observatory.',
      Taglines: ['Some messages were never meant to arrive.'],
      ProductionLocations: ['New Mexico'],
      People: [
        { Name: 'Alex Example', Type: 'Actor', Role: 'Mara Vale' },
        { Name: 'Sam Example', Type: 'Director' },
      ],
    }, 'Mystery');

    expect(options.specificFocalSubjects[0]).toContain('Mara Vale');
    expect(options.specificFocalSubjects.join(' ')).not.toContain('Alex Example');
    expect(options.specificSettings).toContain('a period-authentic 1987 story-world environment');
    expect(options.specificSettings).toContain('a cinematic story-world landscape inspired by New Mexico');
    expect(options.focalSubjects).toContain('a key');
    expect(options.settings).toContain('libraries');
    expect(options.byFocus.Character).toContain('Mara Vale');
    expect(options.byFocus.Environment).toContain('a cinematic story-world landscape inspired by New Mexico');
  });

  it('adds Jellyfin story context to the generated image prompt', () => {
    const prompt = buildPosterPrompt({
      title: 'The Last Signal',
      genres: ['Mystery'],
      storyContext: 'A radio astronomer follows an impossible transmission into the desert.',
    });
    expect(prompt).toContain("Story context from the user's Jellyfin library");
    expect(prompt).toContain('radio astronomer');
    expect(prompt).toContain('do not invent a celebrity likeness');
  });

  it('builds the new structured Matinee editorial prompt by default', () => {
    const prompt = buildPosterPrompt({ title: 'The Last Signal', genres: ['Science Fiction'] });
    expect(prompt).toContain('OUTPUT\n');
    expect(prompt).toContain('MATINEE HOUSE LANGUAGE · 1.0.0\n');
    expect(prompt).toContain('VISUAL TREATMENT\nSelected treatment — Monumental Editorial');
    expect(prompt).toContain('COMPOSITION\nSelected composition — Monument and Witness');
    expect(prompt).toContain('TYPOGRAPHY\n');
    expect(prompt).toContain('RESTRICTIONS\n');
    expect(prompt).toContain('Do not default to cosmic imagery');
  });

  it('supports alternate treatment and genre-led composition blocks', () => {
    const prompt = buildPosterPrompt({
      title: 'The Last Signal',
      genres: ['Mystery'],
      visualTreatment: 'Archival Ticket Print',
      compositionStyle: 'Genre Led',
    });
    expect(prompt).toContain('Selected treatment — Archival Ticket Print');
    expect(prompt).toContain('Genre-led composition: single clue-like object');
    expect(prompt).not.toContain('Supporting genre rhythm');
  });

  it('uses attached Jellyfin stills for title accuracy without copying their composition', () => {
    const prompt = buildPosterPrompt({
      title: 'The Last Signal',
      genres: ['Science Fiction'],
      storyContext: 'Mara crosses an abandoned observatory to answer an impossible signal.',
      hasVisualReferences: true,
    });
    expect(prompt).toContain('VISUAL REFERENCES');
    expect(prompt).toContain('story-specific colors, silhouettes, props');
    expect(prompt).toContain('Do not reproduce recognizable actor faces');
    expect(prompt).toContain('do not simply crop, trace, filter, or reproduce a source frame');
  });

  it('adapts asset layout, focus, and typography from the four guided controls', () => {
    const prompt = buildPosterPrompt({
      title: 'The Last Signal',
      year: 2026,
      genres: ['Mystery'],
      assetType: 'Backdrop',
      focus: 'Environment',
      subject: 'an abandoned desert observatory',
      textTreatment: 'No Text',
    });

    expect(prompt).toContain('Wide landscape orientation at a 16:9 aspect ratio');
    expect(prompt).toContain('Environment focus');
    expect(prompt).toContain('an abandoned desert observatory');
    expect(prompt).toContain('Do not include any lettering');
    expect(prompt).toContain('Year: 2026.');
  });

  it('uses movie.mf.json creative context for focus options', () => {
    const manifest = {
      manifestVersion: 1,
      creativeContext: {
        characters: ['Mara Vale: A radio astronomer searching for the source.'],
        signatureObjects: ['The brass receiver: A link to the impossible signal.'],
        iconicScenes: ['The observatory blackout: The signal arrives during total darkness.'],
        locations: ['The desert observatory: An isolated listening station.'],
      },
      artworkBrief: {
        primarySymbols: ['A narrow radio wave crossing a dark sky'],
      },
    };
    const options = getTitlePosterOptions({ Name: 'The Last Signal' }, 'Mystery', manifest);

    expect(options.byFocus.Character).toEqual(manifest.creativeContext.characters);
    expect(options.byFocus['Signature Element']).toContain(manifest.creativeContext.signatureObjects[0]);
    expect(options.byFocus.Scene).toEqual(manifest.creativeContext.iconicScenes);
    expect(options.byFocus.Environment).toEqual(manifest.creativeContext.locations);
    expect(options.byFocus.Character).not.toContain('A central character from The Last Signal');
  });

  it('combines movie.mf.json details with the locked Matinee style manifest', () => {
    const prompt = buildPosterPrompt({
      title: 'The Last Signal',
      genres: ['Mystery'],
      focus: 'Signature Element',
      subject: 'The brass receiver: A link to the impossible signal.',
      textTreatment: 'Title + Tagline',
      movieManifest: {
        manifestVersion: 1,
        official: { tagline: 'Some messages were never meant to arrive.' },
        creativeContext: {
          themes: ['Isolation', 'Obsession'],
          locations: ['The desert observatory'],
          signatureObjects: ['The brass receiver'],
          iconicScenes: ['The observatory blackout'],
        },
        artworkBrief: {
          paletteHints: ['Cold navy sky', 'one amber receiver light'],
          compositionHints: ['A single object beneath a large empty sky'],
          negativePrompts: ['Generic satellite dishes'],
        },
      },
    });

    expect(prompt).toContain('MOVIE-SPECIFIC CREATIVE MANIFEST · v1');
    expect(prompt).toContain('The brass receiver');
    expect(prompt).toContain('The observatory blackout');
    expect(prompt).toContain('Cold navy sky');
    expect(prompt).toContain('Generic satellite dishes');
    expect(prompt).toContain('MATINEE HOUSE LANGUAGE · 1.0.0');
    expect(prompt).toContain('Title system: Fraunces 600');
    expect(prompt).toContain('Some messages were never meant to arrive.');
  });

  it('wraps imported prompts with optional Matinee and technical direction', () => {
    const styled = buildCustomArtworkPrompt({
      customPrompt: 'A quiet lighthouse above a storm.',
      assetType: 'Banner',
    });
    const independent = buildCustomArtworkPrompt({
      customPrompt: 'A quiet lighthouse above a storm.',
      assetType: 'Banner',
      includeMatineeStyle: false,
    });

    expect(styled).toContain('MATINEE HOUSE LANGUAGE');
    expect(styled).toContain('Ultra-wide banner orientation at a 12:5 aspect ratio');
    expect(independent).not.toContain('MATINEE HOUSE LANGUAGE');
    expect(independent).toContain('Make exactly one image-generation attempt');
  });
});
