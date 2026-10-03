# Contributing to Matinee

Thanks for helping improve Matinee.

> [!IMPORTANT]
> Matinee v2 is currently being rebuilt as a native Rust + GPUI desktop client. The existing Tauri/React app remains the reference implementation until the native replacement reaches parity.

## Before opening a pull request

- Keep changes focused and explain the user-visible or architectural reason for them.
- Preserve the shipping Tauri app unless a migration phase explicitly replaces behavior.
- Do not commit API keys, access tokens, passwords, private server addresses, media-library metadata, or other secrets.
- Add or update tests for behavior changes.
- Run the relevant formatting, type-checking, tests, and builds before requesting review.

## Architecture

The native migration is intentionally layered. Generic GPUI components belong in Atelier; Matinee-specific UI and domain behavior belong in Matinee crates. Shared service crates must remain independent of Tauri and GPUI unless the architecture documentation explicitly says otherwise.

Start with `docs/architecture/overview.md`, `docs/migration/roadmap.md`, and `docs/design-spec.md` before making broad architectural or visual changes.

## Development checks

For the web/Tauri application:

```bash
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test
pnpm build
```

For the native Rust workspace:

```bash
scripts/check-architecture.sh
cargo +1.90.0 fmt --all -- --check
cargo +1.90.0 clippy --workspace --all-targets --locked -- -D warnings
cargo +1.90.0 test --workspace --locked
```

Please report security issues according to `SECURITY.md` rather than opening a public issue.
