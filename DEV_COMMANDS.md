# Hex Studio — Developer Commands

## Prerequisites

Both commands below require the Rust toolchain and Node.js to be sourced first.
Add these two lines at the top of any terminal session (or add them to your shell profile):

```bash
source "$HOME/.cargo/env"
export NVM_DIR="$HOME/.nvm" && source "$NVM_DIR/nvm.sh"
```

---

## Development build (hot-reload)

Compiles the Rust backend and starts the Vite dev server with live reload:

```bash
source "$HOME/.cargo/env" && \
export NVM_DIR="$HOME/.nvm" && source "$NVM_DIR/nvm.sh" && \
cd ~/hex-studio && \
npm run tauri dev
```

The app window opens automatically. Frontend changes reload instantly;
Rust changes trigger a recompile and restart.

---

## Release build (DMG)

Produces an optimised binary and packages it as a `.app` and `.dmg`:

```bash
source "$HOME/.cargo/env" && \
export NVM_DIR="$HOME/.nvm" && source "$NVM_DIR/nvm.sh" && \
cd ~/hex-studio && \
npm run tauri build
```

Output files after a successful build:

| Artifact | Path |
|----------|------|
| `.app` bundle | `src-tauri/target/release/bundle/macos/Hex Studio.app` |
| `.dmg` installer | `src-tauri/target/release/bundle/dmg/Hex Studio_0.1.0_aarch64.dmg` |

> **Note:** The app is unsigned. On first launch right-click → **Open**
> to bypass Gatekeeper, or run:
> ```bash
> xattr -cr ~/hex-studio/src-tauri/target/release/bundle/macos/"Hex Studio.app"
> ```
