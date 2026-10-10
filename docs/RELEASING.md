# Releasing Grandie Chess

Releases are built by GitHub Actions (`.github/workflows/release.yml`) when you push a version tag. They are universal (Apple Silicon and Intel) and are published as a **draft** GitHub release, which you review and then publish.

## One-time setup

### 1. Updater signing key (required)

The in-app updater only installs updates signed with the project's key. The key pair was generated with:

```sh
pnpm tauri signer generate --ci -w ~/.tauri/grandie-chess.key
```

- `~/.tauri/grandie-chess.key` is the **private** key. Never commit it; keep a backup somewhere safe. If it is lost, existing installs can't be updated automatically.
- The public key is in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.

Add the private key to the repository's Actions secrets (Settings → Secrets and variables → Actions):

| Secret | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | the contents of `~/.tauri/grandie-chess.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | leave empty (the key was generated without a password) |

With the GitHub CLI: `gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/grandie-chess.key`.

### 2. Apple Developer ID (optional, recommended for public releases)

Without it, builds are **ad-hoc signed**: they run, but on first launch macOS asks the user to approve the app in System Settings → Privacy & Security. With a Developer ID and notarisation, the app opens without any warning.

Once you have an Apple Developer Program membership:

1. Create a **Developer ID Application** certificate and export it from Keychain Access as a `.p12` with a password.
2. Create an **App Store Connect API key** (Users and Access → Integrations → Keys) with the Developer role, and download the `.p8` file.
3. Add these Actions secrets:

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | `base64 -i certificate.p12` output |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_API_ISSUER` | the issuer ID shown above the keys list |
| `APPLE_API_KEY` | the key ID |
| `APPLE_API_KEY_P8` | the contents of the `.p8` file |

The release workflow detects these and switches from ad-hoc signing to Developer ID signing plus notarisation. Tauri signs the bundled `lc0` helper too, with the hardened runtime.

To sign locally instead, export the same variables (with `APPLE_API_KEY_PATH` pointing to the `.p8` file) before `pnpm tauri build`.

## Making a release

1. Bump the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json` (they must match).
2. Commit, then tag and push:

   ```sh
   git tag v0.2.0
   git push origin main v0.2.0
   ```

3. Wait for the **Release** workflow. It builds universal lc0, the app and the DMG, signs the update bundle, and creates a draft release with:
   - `Grandie.Chess_<version>_universal.dmg` for new installs
   - `Grandie-Chess.dmg`, the same DMG under a fixed name for the download link
   - `Grandie.Chess_universal.app.tar.gz` and its `.sig` for the updater
   - `latest.json`, the update manifest
4. Edit the draft's notes and publish it. Installed copies find the update through
   `https://github.com/Inad-Tech-World/grandie_chess/releases/latest/download/latest.json`
   and offer to install it.

## Download link

Share this link; it always serves the DMG of the newest **published** release (drafts and pre-releases are skipped):

```
https://github.com/Inad-Tech-World/grandie_chess/releases/latest/download/Grandie-Chess.dmg
```

It returns "Not Found" until the first release is published.

## Building a release locally

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
engine/build-lc0.sh --universal
CI=true TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/grandie-chess.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
  pnpm tauri build --target universal-apple-darwin
```

The DMG is written to `src-tauri/target/universal-apple-darwin/release/bundle/dmg/`. `CI=true` skips arranging the DMG window through Finder, which fails in sessions that can't control Finder (SSH, automation tools).

## Distribution notes

- Grandie Chess is GPL-3.0-or-later; every release must make the matching source available. The GitHub tag does that.
- Don't publish to the Mac App Store: its terms conflict with the GPL (lc0, chessground and shakmaty are GPL).
