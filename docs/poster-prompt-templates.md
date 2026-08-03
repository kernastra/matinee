# Matinee Poster Prompt Templates

Matinee poster prompts use a fixed global style guide plus one genre recipe.

## Global Style

- Vertical 2:3 poster card.
- Warm, nostalgic, cozy, premium cinematic illustration.
- Soft amber practical light, midnight navy shadows, ticket cream typography, restrained burgundy, and faded teal accents.
- One clear focal subject, readable silhouette, elegant title placement, minimal supporting text, and generous negative space.
- Avoid official poster recreation, celebrity likenesses, studio logos, franchise branding, busy floating-head collages, photorealistic stills, neon cyberpunk palettes, and tiny illegible text.

## Prompt Formula

```text
Global Matinee style
+ movie title
+ selected genre recipe
+ focal subject
+ setting
+ mood
+ lighting
+ palette
+ composition
+ title treatment
+ avoid list
```

## Recipe Coverage

The code templates currently cover:

Action, Adventure, Animation, Comedy, Crime, Documentary, Drama, Family, Fantasy, Horror, Music, Mystery, Romance, Science Fiction, Thriller, War, and Western.

Common aliases such as `Sci-Fi`, `Kids`, `Children`, `Musical`, and `Suspense` map to the closest Matinee recipe.

## Example

```text
Create a vertical 2:3 movie poster card for the Matinee streaming interface.
Use a warm, nostalgic, cozy, premium cinematic illustration style with subtle film grain.
Favor soft amber practical light, midnight navy shadows, ticket cream typography, restrained burgundy, and faded teal accents.
Use one clear focal subject, readable silhouette, elegant title placement, minimal supporting text, and generous negative space.
Do not recreate official poster artwork, celebrity likenesses, studio logos, franchise branding, busy floating-head collages, photorealistic stills, neon cyberpunk palettes, or tiny illegible text.
Title: "The Last Signal".
Genre recipe: Thriller.
Show a glowing phone on a diner table in an empty roadside diner at midnight with a taut, paranoid, shadowed, urgent mood.
Lighting: thin amber light against deep shadows, high contrast but not harsh.
Palette: charcoal, midnight navy, warning amber, muted red.
Composition: compressed negative space and one suspenseful focal subject.
Typography: tight serif title with controlled tension; include only the movie title and minimal supporting text if needed.
Genre-specific avoid list: generic gun-and-face layouts, oversharpened realism, explosive action styling.
```

Implementation lives in `src/lib/posterPrompts.ts`.
