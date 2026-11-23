# Yamp

Yamp is a personal-use, unofficial Winamp-2-style desktop client for macOS. It is not distributed through any app store and is not affiliated with Yandex.

## Installing

### Prerequisites

- **Node.js & npm**
- **Rust toolchain** via `rustup` (`$HOME/.cargo/env`) and Xcode Command Line Tools (`clang`)
- **Python 3** (`/usr/bin/python3`) with a project virtual environment containing `sidecar/requirements.txt`

### Build & Install

```bash
# 1. Create the Python virtual environment and install the sidecar library
/usr/bin/python3 -m venv .venv
.venv/bin/pip install -r sidecar/requirements.txt

# 2. Install frontend & Tauri CLI dependencies
npm install

# 3. Build the macOS .app bundle
source "$HOME/.cargo/env"
npm run tauri build -- --bundles app
# (or `npm run tauri build` to build all configured bundles)
```

The built application bundle lands at:

`src-tauri/target/release/bundle/macos/Yamp.app`

Launch it with:

```bash
open src-tauri/target/release/bundle/macos/Yamp.app
```

Or run in development mode:

```bash
source "$HOME/.cargo/env"
npm run tauri dev
```

*(Note: When launching `Yamp.app` directly from Finder outside the repository directory, ensure `YAMAMP_ROOT` points to this project folder if needed, or launch from the repo root so it finds `.venv` and `sidecar/server.py`.)*

### Personal OAuth Token

Yamp does not collect, ship, or prompt for credentials. To use Search, Playlists, «Мне нравится», «Моя волна», and audio playback, provide your own personal Yandex OAuth token:

1. Open `https://oauth.yandex.ru/authorize?response_type=token&client_id=23cabbbdc6cd418abb4b39c32c41195d` in your browser.
2. Authorize access.
3. Copy `access_token` from the redirect URL fragment (`#access_token=...`).
4. In Yamp, open **Settings** (`CFG` or `Cmd+,`), paste the token into **OAuth Token**, and click **Save to Keychain**.

For additional details on token handling in unofficial clients, see the [`yandex-music` documentation](https://yandex-music.readthedocs.io/).

### Token Storage & Disclaimer

- The OAuth token is stored **only in the macOS Keychain** via `/usr/bin/security` under:
  - **Service:** `dev.kirdyaev.yamamp`
  - **Account:** `oauth-token`
- The token is never written to files, settings JSON, git, stdout, or stderr, and is not displayed again after saving (only a status badge with at most the last 4 characters is shown).
- **Disclaimer:** This project uses an unofficial API for personal use only. It is not affiliated with or endorsed by Yandex and is not distributed through any app store.

## Stack & Architecture

- **Desktop shell:** Tauri v2 (`src-tauri/`) with native macOS `WKWebView` and bundle ID `dev.kirdyaev.yamamp`.
- **Player UI:** [`webamp`](https://www.npmjs.com/package/webamp) using its default bundled Winamp 2 skin (Main, Equalizer, and Playlist windows). Play, pause, next, previous, volume, and equalizer controls are provided directly by Webamp.
- **Library frontend:** Vite + TypeScript (`src/`, `index.html`, `settings.html`, `library.html`) with tabs for **Search**, **Playlists**, **«Мне нравится»**, and **«Моя волна»** (`user:onyourwave`).
- **Yandex Music API sidecar:** Local Python HTTP sidecar (`sidecar/server.py`) running inside a project `.venv` using [`yandex-music`](https://github.com/MarshalX/yandex-music-api) (`yandex-music`). It binds only to `127.0.0.1` on an ephemeral port, receives a per-launch shared secret and token via `stdin` (never via CLI args, files, or logs), and exposes authenticated JSON endpoints plus a localhost CORS/Range audio streaming proxy for Webamp.
- **Legacy internals:** Internal identifiers such as bundle/Keychain ID `dev.kirdyaev.yamamp`, custom stream URI scheme `yamamp://`, and env var `YAMAMP_ROOT` are historical from the original name and are intentionally kept unchanged so existing Keychain tokens and local setups continue to work seamlessly.
