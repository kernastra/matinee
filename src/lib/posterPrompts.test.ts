import { describe, expect, it } from 'vitest';
import { buildPosterPrompt, getPosterRecipe } from './posterPrompts';

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
      focalSubject: 'a glowing phone on a diner table',
      setting: 'an empty roadside diner at midnight',
    });

    expect(prompt).toContain('Title: "The Last Signal".');
    expect(prompt).toContain('Genre recipe: Thriller.');
    expect(prompt).toContain('a glowing phone on a diner table');
    expect(prompt).toContain('an empty roadside diner at midnight');
    expect(prompt).toContain('Do not recreate official poster artwork');
    expect(prompt).toContain('celebrity likenesses');
  });
});
