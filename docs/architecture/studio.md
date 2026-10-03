# Poster Studio services

`matinee-studio` is the Poster Studio backend. It is not a screen. It depends
on `matinee-secrets` and does not depend on Tauri, GPUI, Atelier, or
`matinee-jellyfin`. The Tauri adapter builds `StudioPaths` from the app
handle (home, app data, and the Pictures directory when it exists) and calls
`Studio`.

The application polls `ReqwestStudio` and `TokioProcess` on a Tokio runtime
this crate does not create. Shipping Tauri already has that runtime. A
future GPUI app uses the Phase 3 application runtime.

## Credentials and providers

fal and Higgsfield keys are raw strings in
`dev.sean.matinee.image-generation`, accounts `fal` and `higgsfield`. A key
shorter than 12 characters is rejected. Codex does not use that vault. A
saved Codex path is the text file `codex-cli-path` in app data. It is not a
secret. Every run canonicalizes it again and rejects a missing or changed
path.

Codex and fal generate images. Higgsfield can be discovered and can store a
key. Generation returns the shipping sentence that no API-key endpoint is
published. No new provider or model is added.

## Prompt boundary

Creative direction stays in `src/lib/posterPrompts.ts`. The service receives
a `GenerationRequest`: provider, prompt, up to four reference URLs, the
Jellyfin server URL, and an artwork kind. A prompt shorter than 20 bytes
after trim, or longer than 30,000 characters, is rejected. Prompts are not
truncated.

## Codex

The process is `codex` plus an argument vector: `exec`, ephemeral, skip git,
ignore user config and rules, strict config, no color, low reasoning, never
approve, web search disabled, the `matinee-poster` permission profile,
network disabled, no login shell, and an empty inherited environment. The
filesystem permission string is the shipping value. There is no `sh -c`,
`cmd /C`, or PowerShell.

The executable name uses `cfg!(windows)` for the `.exe` suffix. That check
lives here because it is the program name, not a UI platform branch.

One Codex job is active per `Studio` value. A second call returns the
shipping busy sentence. Dropping the future kills the child
(`kill_on_drop`). The 12-minute timeout is the other stop. There is no
separate cancel method. Login status uses a 12-second timeout. Job
directories older than seven days are removed. Logs kept for a failure are
bounded.

## fal and untrusted URLs

Models are `fal-ai/flux/dev` and, when references are present,
`fal-ai/flux-2/edit`. The POST does not follow redirects. Authorization is
`Key {secret}` and is redacted in `Debug`.

Reference downloads do not follow redirects, must share the Jellyfin
server's origin, and stop at 12 MiB. Generated images are fetched from the
provider URL for at most three validated hops. Each hop must be HTTP or
HTTPS with no userinfo. The body stops at 32 MiB. Magic bytes must match
the declared type. `application/octet-stream` or a missing type is accepted
when the bytes sniff as JPEG, PNG, or WebP. This is stricter than the
shipping app, which trusted `Content-Type` and followed image redirects
with the default client.

## Files

Media writes are limited to `~/media-data/media`, `~/media`, and `~/Videos`
after canonicalization. Container paths `/media`, `/movies`, `/tv`, and
`/shows` map onto those roots. `..`, a symlink that leaves a root, a
non-file, and a path outside the roots are rejected.

Generated and custom posters live under app data. Item ids are ASCII
letters, digits, `-`, and `_`, at most 128 characters. Titles are reduced
to a safe file stem. `export_poster_to_media_folder` writes `poster.jpg`
(JPEG quality 92) beside a recognized video file. If that file exists and
`overwrite` is false, the error string is exactly `POSTER_EXISTS`.

`movie.mf.json` must be version 1 and at most 2 MB. Sanitization removes
control characters, collapses whitespace, caps lists, and drops empty
strings. `manifest_json` removes nulls for the invoke payload.
