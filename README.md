# Yamamp

Yamamp is a personal-use, unofficial Winamp-2-style desktop client for macOS. It is not published or affiliated with Yandex.

## Stack & Architecture

- **Desktop shell:** Tauri v2 (`src-tauri/`) with native macOS `WKWebView` and bundle ID `dev.kirdyaev.yamamp`.
- **Player UI:** [`webamp`](https://www.npmjs.com/package/webamp) using its default bundled Winamp 2 skin (Main, Equalizer, and Playlist windows). Play, pause, next, previous, volume, and equalizer controls are provided directly by Webamp.
- **Library frontend:** Vite + TypeScript (`src/`, `index.html`, `settings.html`) with tabs for **Search**, **Playlists**, **«Мне нравится»**, and **«Моя волна»** (`user:onyourwave`).
- **Yandex Music API sidecar:** Local Python HTTP sidecar (`sidecar/server.py`) running inside a project `.venv` using [`yandex-music`](https://github.com/MarshalX/yandex-music-api) (`yandex-music`). It binds only to `127.0.0.1` on an ephemeral port, receives a per-launch shared secret and token via `stdin` (never via CLI args, files, or logs), and exposes authenticated JSON endpoints plus a localhost CORS/Range audio streaming proxy for Webamp.

## Installing Dependencies

Prerequisites on macOS (Apple Silicon):
- Xcode Command Line Tools (`clang`)
- Rust toolchain via `rustup` (`$HOME/.cargo/env`)
- Node.js & npm
- Python 3 (`/usr/bin/python3`)

Install dependencies in the project folder:

```bash
# 1. Create the Python virtual environment and install the sidecar library
/usr/bin/python3 -m venv .venv
.venv/bin/pip install -r sidecar/requirements.txt

# 2. Install frontend & Tauri CLI dependencies
npm install
```

## Building & Launching

Build the macOS `.app` bundle and launch it:

```bash
source "$HOME/.cargo/env"
npm run build
npm run tauri build -- --bundles app
open src-tauri/target/release/bundle/macos/Yamamp.app
```

Or run in development mode:

```bash
source "$HOME/.cargo/env"
npm run tauri dev
```

*(Note: When launching `Yamamp.app` directly from Finder outside the repository directory, ensure `YAMAMP_ROOT` points to this project folder if needed, or launch from the repo root so it finds `.venv` and `sidecar/server.py`.)*

## Obtaining a Personal OAuth Token

Yamamp does not collect, ship, or prompt for credentials. To use Search, Playlists, «Мне нравится», «Моя волна», and audio playback, provide your own personal Yandex OAuth token:

1. Open the standard OAuth authorization URL used by unofficial `yandex-music` clients in your browser:
   `https://oauth.yandex.ru/authorize?response_type=token&client_id=23cabbbdc6cd418abb4b39c32c41195d`
2. Authorize access in your browser.
3. From the redirected URL fragment (`#access_token=...`), copy the `access_token` value.
4. In Yamamp, click **Open Settings**, paste the token into the **OAuth Token** field, and click **Save to Keychain**.

For additional details on token handling in unofficial clients, see the [`yandex-music` documentation](https://yandex-music.readthedocs.io/).

## Token Storage

- The OAuth token is stored **only in the macOS Keychain** via `/usr/bin/security` under:
  - **Service:** `dev.kirdyaev.yamamp`
  - **Account:** `oauth-token`
- The token is never written to files, settings JSON, git, stdout, or stderr, and is not displayed again after saving (only a status badge with at most the last 4 characters is shown).
