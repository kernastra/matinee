# UI Framework (`atelier-ui`)

`atelier-ui` is the reusable design-system layer. It owns tokens, themes, and
components, and it is the compatibility boundary around GPUI. It contains no
Matinee concepts: an application supplies a `Theme` that maps its brand onto
the framework's semantic roles.

## Principles

- **Semantic tokens over values.** Components ask for `text.secondary`,
  `Space::S3`, `Radius::Medium`, `MotionDuration::Fast`, never `#b7bbc2`,
  `12px`, or `120ms`.
- **Components own behavior.** A `Button` owns hover, pressed, focus, disabled,
  loading, keyboard activation, pointer rules, and theme integration, not
  just its look.
- **Accessibility and reduced motion come first.** Every theme passes a
  contrast contract (enforced by tests). Motion goes through one resolver that
  honors reduced motion.
- **Apps use framework APIs, not GPUI, where practical.** Apps import
  `atelier_ui::prelude::*`, a curated surface, and never list `gpui` in their
  manifests (CI-enforced).
- **No speculative abstraction.** Phase 0 ships only what the Gallery and the
  shell use.

## Tokens (`atelier_ui::tokens`)

Tokens are plain Rust data with no GPUI types, so they are unit-tested and
documented in isolation. `bridge.rs` is the only place that converts them to
GPUI values.

| Group | Tokens |
|---|---|
| Color (`ColorRole`) | `text.{primary,secondary,muted,disabled,on_accent,on_destructive}`, `surface.{canvas,panel,elevated,overlay}`, `border.{subtle,default,strong}`, `control.{accent,neutral,destructive}{,_hover,_pressed}`, `control.{subtle_hover,subtle_pressed,disabled}`, `focus.ring` |
| Typography (`TextRole`) | `display`, `title`, `heading`, `subheading`, `body`, `label`, `metadata`, `caption`. Each maps to a `FontRole` (display / interface / monospace), a size, a line height on a 2px grid, and a weight. |
| Spacing (`Space`) | `space.0`, `space.0_5` (2px), `space.1` (4px) … `space.16` (64px). Framework-wide (not per theme) so rhythm is identical across themes. |
| Radius (`Radius`) | `none`, `small`, `medium`, `large`, `full`. Per theme. |
| Elevation (`Elevation`) | `flat`, `raised`, `overlay`, `modal`: two-layer shadows tinted per theme. |
| Motion | `motion.{fast,standard,slow}` (120/200/320ms), `spring.{snappy,smooth,gentle}` (SwiftUI-style response + damping), `MotionPreference::{Full,Reduced}`. |

## Themes

`Theme { name, appearance, colors, typography, radius, elevation, motion }` is
a GPUI global. Built-ins are `Theme::neutral_dark()` (default) and
`Theme::neutral_light()`. Apps add their own; Matinee's is
`matinee_ui::matinee_theme()`.

`Theme::validate()` checks this contract, and every shipped theme has a
test asserting it returns no issues:

| Requirement | Minimum |
|---|---|
| `text.primary` on canvas / panel / elevated | 7:1 |
| `text.secondary`, `text.muted` on canvas / panel / elevated | 4.5:1 |
| `text.primary` on neutral control (all states) | 4.5:1 |
| `text.on_accent` on accent (all states); `text.on_destructive` on destructive (all states) | 4.5:1 |
| `focus.ring` on canvas and panel (WCAG 1.4.11) | 3:1 |

The Gallery's Color page shows the live result for the active theme.

### Matinee mapping (`crates/matinee-ui`)

| Semantic role | Matinee value |
|---|---|
| `surface.canvas` | Midnight Navy `#080e15` |
| `surface.panel` / `surface.elevated` | `#111820` / Projection Room `#1b232b` |
| `text.primary` / `text.muted` | Ticket Cream `#f6eedd` / `#a89e8d` |
| `control.accent` (+hover) / `focus.ring` | Marquee Amber `#e6a452` (`#f0b76d`) |
| `text.on_accent` | `#1c140c` (the stylesheet's selection ink) |
| `control.destructive` | Curtain Burgundy `#963f47` |
| Families | Fraunces (display), Manrope (interface), IBM Plex Mono (metadata / caption) |
| Scale | 64/40/26/17/16/13/11 (`docs/design-spec.md`) |

Theater Brown and Faded Teal are in `matinee_ui::palette` but have no
semantic role yet; they get one only when a component needs it. The fonts are
not bundled yet, so GPUI falls back to the platform UI font (see "Known gaps").

## Components

| Component | Notes |
|---|---|
| `Text` | Role + tone (`Primary/Secondary/Muted/Disabled`); `truncate()`. |
| `Icon`, `IconName`, `IconSize` | 14 hand-drawn, generic 24px stroke glyphs embedded via `UiAssets`. `spinning(true)` rotates the icon only when continuous motion is allowed. |
| `Button` | Variants `Primary/Secondary/Subtle/Destructive`, sizes `Small/Medium/Large`, optional leading icon, `disabled`, `loading`. |
| `IconButton` | Same interaction model; square; **label is a constructor argument** (tooltip now, accessible name later). |
| `Surface` | Themed container (`Canvas/Panel/Elevated`), accepts children and further styling. |
| `h_stack`, `v_stack` | Flex helpers that take a `Space` gap. |
| `FocusRing` | The framework focus indicator, for app-level focusable elements. |
| `StyledExt` | `elevation(theme, level)`, `corner_radius(theme, radius)` on any element. |
| `motion::{timed, spring}` | Turn tokens into GPUI animations. They return `None` under reduced motion, meaning "apply the end state now". |
| `TextField` / `SearchField` | Single-line editing via GPUI's input handler (caret, selection, IME). Controlled `value`. Search adds an icon, a clear button, and Escape. |
| `Switch` / `Checkbox` / `SegmentedControl` / `Slider` | Pointer and keyboard controls. The switch thumb uses `Spring::Snappy`. The slider is a generic value control with one finite ascending range (`min <= max`); reversed and non-finite ranges collapse to `0`. |
| `focus_visible` | Keyboard focus draws `FocusRing`. Pointer interaction does not. |

### Button behavior contract

| State | Behavior |
|---|---|
| Hover | Hover fill from the variant's `*_hover` token; pointer cursor. |
| Pressed | Pressed fill while the primary button is held. The component tracks it itself, not GPUI's `active` style (see quirks). |
| Focused | `FocusRing` outside the border box. Tab / Shift-Tab traverse; **Enter and Space activate** on key-up. |
| Pointer activation | Does **not** move keyboard focus (macOS convention). |
| Disabled | Disabled colors, no pointer feedback, not a tab stop, never activates. |
| Loading | Keeps focus and width (label stays in layout, hidden under a spinner), no pointer feedback, ignores activation. |

`ButtonColors::resolve(theme, variant, status)` is a pure function, unit-tested
for every theme. The interaction rules are covered by headless GPUI tests in
`crates/atelier-ui/tests/button_behavior.rs`.

## GPUI compatibility boundary

Where GPUI is touched:

- `atelier-ui/src/bridge.rs`: token → GPUI conversions (color, fill, weight, shadows, pixels).
- Component `render` functions in `atelier-ui/src/components/`.
- `atelier-app/src/app.rs`: `Application`, windows, keymap, menus, actions.
- `atelier_ui::prelude`: the **curated re-export** apps rely on (`App`,
  `Context`, `Window`, `Render`, `div`, `px`, element traits, …). This list *is*
  the GPUI surface area apps depend on. Growing it is a deliberate API decision.
  When application code needs a raw GPUI API, first decide whether Atelier
  should expose it. Raw use is allowed when a wrapper would be speculative.
- `atelier_ui::gpui`: full re-export as an escape hatch. Any use outside the
  framework crates is migration debt.

Phase 0 doesn't fully hide GPUI's element model (`div()`, `Context<T>`,
`Render`). Wrapping that now would be a speculative layout DSL. The boundary
instead guarantees that the GPUI *version* and its quirks are handled in two
crates.

## GPUI dependency policy

- **Source:** crates.io `gpui = "=0.2.2"` (published 2025-10-22 from
  `zed-industries/zed`, Apache-2.0). It is declared once in
  `[workspace.dependencies]` with an exact `=` requirement. `Cargo.lock` is
  committed and CI builds with `--locked`. There is no git dependency and no
  floating revision.
- **Why crates.io over a git SHA:** an immutable, checksummed artifact that
  needs no git fetch in CI, with a version that is easy to reason about. 0.2.2
  is still the newest crates.io release, so a git pin would only be needed for
  a specific upstream fix. If that happens, use `git = "…zed", rev = "<full
  40-char SHA>"`, never a branch.
- **Update strategy:** bump deliberately in a dedicated PR. Read the upstream
  changes to `crates/gpui`. Re-run the Gallery on all three platforms. Re-check
  the quirk list below, since each quirk has a workaround in the framework that
  may become unnecessary or break. Run the headless behavior tests.
- **Compatibility notes:** edition 2024 / Rust ≥ 1.90. Linux renders through
  Blade on Vulkan, macOS through Metal, and Windows through DirectX 11. GPUI's
  `test-support` feature is used only as a dev-dependency of `atelier-ui`.
  GPUI's crates.io releases are infrequent and its API is unstable between
  minors, which is why it is isolated.

### GPUI 0.2.2 quirks handled by the framework

| Quirk | Workaround |
|---|---|
| A `BoxShadow` with `blur_radius = 0` renders nothing (the shader integrates over ±3σ), so spread-only focus rings are invisible. | `FocusRing` is a bordered overlay element; components track their own `FocusHandle` to know when to show it. |
| `active()` (pressed) style registers its release listener one frame after the press, so a press and release within one frame leave the control stuck pressed. | `Button`/`IconButton` track pressed state with listeners registered every frame. |
| App-level (`cx.on_action`) handlers run while the dispatching window is borrowed, so `window.update` inside them fails silently. | `atelier-app` defers window operations (`cx.defer`). |
| Tab / Shift-Tab are not bound by default. | `atelier-app` binds them to focus traversal. |
| Focusable elements take focus on mouse-down. | Buttons call `prevent_default` on mouse-down (covered by a test). |
| `TestAppContext::simulate_keystrokes` sends key-down only; keyboard clicks fire on key-up. | Tests send explicit key-down and key-up events. |
| `simulate_input` splits on `""`, which yields empty keystrokes and panics. | Tests call `Window::dispatch_keystroke` per character. |
| `Window::scale_factor` is read-only. `set_rem_size` would not scale `px()` layout. | The Gallery displays the live factor and does not simulate one. |
| No reduced-motion API. | `atelier-app` probes the OS at launch. See platform-strategy.md. |

## Gallery (`apps/atelier-gallery`)

- A story is a `fn(&mut Window, &mut App) -> AnyElement` plus metadata in
  `story.rs::STORIES`. Adding a demo means writing one function and adding one
  entry. Story state uses `window.use_keyed_state`.
- Preview settings are globals, so every story reacts to them automatically:
  theme (Neutral Dark / Neutral Light / Matinee), reduced motion, and a
  focus-testing note. The toolbar also shows the live scale factor and whether
  the OS motion probe returned a value. The Settings command (⌘, / Ctrl+,)
  toggles the preview toolbar.
- Pages today: Color, Typography, Spacing, Radius & Elevation, Motion, Icons,
  Button, Icon Button, Text Field, Search Field, Switch, Checkbox, Segmented
  Control, Slider, Text, Surface.
- A lightweight inspector under the story reports the interactive control's
  focus, value, input modality, and reduced motion.
- Still open: simulated platform (needs `Platform` injection) and high-contrast
  themes. Scale-factor simulation is not planned until GPUI can change it.

## Testing strategy

| What | How | Where |
|---|---|---|
| Token math (contrast, scales, springs) | Unit tests | `atelier-ui/src/tokens/*` |
| Theme contract | `validate()` tests for every theme | `atelier-ui/src/theme.rs`, `matinee-ui` |
| Variant/state color resolution | Pure-function tests | `components/button.rs` |
| Interaction behavior | Headless GPUI `TestAppContext` (keyboard activation, focusability, pointer does not focus, loading/disabled) | `atelier-ui/tests/button_behavior.rs` |
| Shortcuts / platform conventions | Pure tests per platform | `atelier-app/src/{command,platform}.rs` |
| Asset routing | Unit tests | `icon.rs`, `app.rs` |
| Story registry | Unit tests | `atelier-gallery/src/story.rs` |
| Layering | `scripts/check-architecture.sh` | CI |
| Per-platform compilation | CI build matrix | `.github/workflows/rust-workspace.yml` |

Not automated, on purpose: pixel and visual regression testing (GPUI has no
stable headless renderer for screenshots, and lavapipe output varies), and
real-window integration tests. Visual states are reviewed in the Gallery.

## Focus-visible contract

`InputModality` is a framework global. `move_focus_forward` / `move_focus_backward`
(what Tab uses) record keyboard modality. Pointer handlers call
`note_pointer_interaction`. `focus_visible(focused, cx)` is true only when the
control is focused and the modality is keyboard. Text fields are the exception
that still take focus on click, because they need a caret; they draw a 1px
`focus.ring` border and a caret, not `FocusRing`.

## Text input limits (GPUI 0.2.2)

- Printable text and IME go through `EntityInputHandler`. Editing commands are
  key bindings in the `TextField` context. Word movement is whitespace-delimited,
  not UAX #29. Newlines are stripped (`shape_line` panics on them).
- The caret does not blink. Marked text is underlined. Headless tests cover
  typing via `dispatch_keystroke`, not a real IME session. Blinking, native
  word boundaries, and manual IME checks on macOS, Windows, and Linux are
  roadmap follow-ups, not this phase.
- `text.danger` is the error-text role (4.5:1 on canvas, panel, and elevated).
  `control.destructive` remains the invalid border and is too dark for small text.

## Known gaps

- Matinee fonts are bundled as static OFL faces (Fraunces SemiBold at optical
  size 72, Manrope Regular and SemiBold, IBM Plex Mono Regular). GPUI 0.2.2
  does not apply variable axes, so the variable originals are not shipped.
- No accessibility tree. GPUI 0.2.2 has no AccessKit integration, so labels are
  captured in the API (for example `IconButton`) for later wiring.
- Hover and press transitions on buttons are instantaneous, matching AppKit
  push buttons. The switch thumb is the first control that animates, via
  `motion::spring`.
- Letter spacing is not expressible in GPUI 0.2.2 text styles.
- Linux reduced motion is GNOME and KDE only. The Windows probe still spawns
  PowerShell; that is temporary technical debt until it calls the Win32 API
  directly. See platform-strategy.md.
