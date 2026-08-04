# Matinee Poster Prompt Architecture

Matinee builds image-generation prompts from structured creative data so the normal Poster Studio workflow never requires prompt writing. The engine combines a title's metadata and optional Movie DNA with Matinee's versioned house style, then adapts the result to the chosen artwork format.

## Prompt Sources

The generated brief draws from three layers:

1. **Matinee house style** — The permanent visual identity in `src/data/matinee-poster-style.json`, including palette, typography, texture, composition, and quality constraints.
2. **Title identity** — Jellyfin metadata plus an optional `movie.mf.json` stored beside the movie file. The manifest can define characters, signature elements, scenes, environments, colors, lighting, mood, and composition.
3. **Creative direction** — Four concise selections in Poster Studio: asset type, focus, title-specific subject, and text treatment.

When a title has no local manifest, Matinee derives useful fallback subjects and settings from its Jellyfin metadata. The selected title remains the source of truth; Matinee does not scan unrelated media folders.

## Assembly Order

```text
Matinee master style
+ asset-specific layout rules
+ Jellyfin movie metadata
+ optional Movie DNA
+ selected focus and subject
+ typography treatment
+ output and quality constraints
```

The assembled prompt asks for a premium collector-edition illustration with a clear visual metaphor, restrained detail, generous negative space, cinematic lighting, and a handcrafted screen-print character. It avoids direct poster recreation, studio branding, floating-head collages, glossy photorealism, and other generic promotional layouts.

## Creative Controls

### Asset Type

- **Poster** — Portrait 2:3 composition with optional title and tagline treatment.
- **Backdrop** — Wide 16:9 artwork with cinematic negative space and no typography.
- **Banner** — Wide promotional composition designed for shallow display areas.
- **Thumbnail** — Compact landscape composition with a strong readable focal point.

### Focus

- **Auto** — Chooses a balanced symbolic direction from the strongest Movie DNA.
- **Character** — Centers one selected character without requiring a literal actor likeness.
- **Signature Element** — Builds the composition around an iconic object, symbol, or motif.
- **Scene** — Reinterprets a memorable sequence as collector artwork.
- **Environment** — Uses a defining place or atmosphere as the main subject.

### Text Treatment

The user can include the title, use official or custom tagline text, or generate artwork without typography. Because image models can misspell text, the no-text option is best when deterministic typography will be added later.

## Advanced Workflow

The **Advanced** panel reveals an editable copy of the assembled prompt. It can also import a complete prompt from `.txt`, `.md`, or prompt-bearing `.json` files. The user may keep Matinee's house style wrapped around the custom direction or disable it for a fully independent prompt.

Provider policy and safety checks still apply. Matinee sends one generation request per click and does not automatically retry rejected output.

## Movie Manifest

A `movie.mf.json` file is optional and editable. Its exact fields may vary, but the engine looks for structured lists and visual guidance such as:

```json
{
  "movie": "Interstellar",
  "characters": ["Cooper", "Murph", "Brand", "TARS"],
  "signatureElements": ["Gargantua", "Endurance", "Saturn", "Wormhole"],
  "scenes": ["Water Planet", "Docking Scene", "Tesseract"],
  "environments": ["Deep Space", "Cooper Farm", "Ice Planet"],
  "movieDna": {
    "heroColor": "white",
    "secondaryColor": "navy",
    "lighting": "vertical beam",
    "texture": "paper grain",
    "mood": "hope",
    "composition": "centered"
  }
}
```

Movie DNA identifies what makes a title visually distinct; the Matinee style manifest determines how that identity is rendered across the collection.

Implementation lives in `src/lib/posterPrompts.ts`, with local manifest handling in `src/lib/movieManifest.ts`.
