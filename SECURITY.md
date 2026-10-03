# Security Policy

## Supported versions

Matinee is under active development. Security fixes are applied to the latest source and, when applicable, the latest published release.

## Reporting a vulnerability

Please do **not** open a public issue for a suspected vulnerability, exposed credential, or other security-sensitive report.

Use GitHub's private vulnerability reporting for this repository when available. If private reporting is unavailable, contact the maintainer privately through the contact information on the GitHub profile.

Include enough detail to reproduce and assess the issue, but do not include real API keys, access tokens, passwords, private media metadata, or other secrets.

## Secrets

Matinee does not require repository-level API keys for normal development. Keep Jellyfin credentials, Radarr/Sonarr API keys, image-provider keys, signing credentials, and release secrets out of commits, issues, pull requests, screenshots, and logs.

If a secret is committed, revoke or rotate it immediately. Removing it in a later commit does not remove it from Git history.
