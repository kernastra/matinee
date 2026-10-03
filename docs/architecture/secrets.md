# Credential vault

`matinee-secrets` is the only crate that talks to the OS keyring. Callers
choose a namespace and an account. The crate does not know Jellyfin, Radarr,
Sonarr, or image providers.

## Backends

| Platform | Backend | `keyring` features |
|---|---|---|
| Linux | Secret Service | `sync-secret-service`, `crypto-rust` |
| macOS | Keychain | `apple-native` |
| Windows | Credential Manager | `windows-native` |

`KeyringStore` is the production type. `keyring::Entry` does not leave the
crate. Linux compiles against `libdbus`; the unit tests and CI do not start
a Secret Service daemon, and they do not write Keychain or Credential
Manager entries. Tests use `MemoryStore`.

Other operating systems get `CredentialError::Backend` with an unsupported
message. That path is not used by the shipping app.

## Shipping namespaces

These strings match Matinee v0.5.6. Renaming them would strand saved keys.

| Namespace | Account | Payload |
|---|---|---|
| `dev.sean.matinee.media-integrations` | `radarr` or `sonarr` | JSON `{ "serverUrl", "apiKey" }` |
| `dev.sean.matinee.image-generation` | `fal` or `higgsfield` | The raw API key |

The Jellyfin session namespace is chosen by `matinee-jellyfin`, not by this
crate: `dev.sean.matinee.jellyfin-session` / `default`. Shipping v0.5.6 does
not write that entry. The browser session in `sessionStorage` is a different
store and is not migrated.

## Secret copies

`Secret` zeroizes the `String` it owns on drop. `Debug` and `Display` render
`[redacted]`. That is not process-wide zeroization. Copies this crate does
not wipe:

- the `String` returned by the `keyring` crate before it is wrapped
- the JSON string `serde_json` builds when a caller stores a payload
- header bytes an HTTP client copies for one request
- the `MemoryStore` entry until `remove`

Backend error text is truncated to 180 characters, control characters are
stripped, and assignments such as `password=` are replaced before the text
is stored on `CredentialError`. `Display` does not include that detail.
