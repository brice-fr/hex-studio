#!/usr/bin/env bash
#
# Build, sign and optionally notarise a release of Hex Studio.
#
#   ./scripts/release.sh --dmg                    signed, notarised and stapled .dmg
#   ./scripts/release.sh --dmg --no-notarize      signed only, no submission to Apple
#
# Notarisation uses a keychain profile, so this script never reads the credentials. Create it once:
#
#   xcrun notarytool store-credentials "hex-studio" \
#     --apple-id "<your-apple-id>" --team-id "<your-team-id>"
#
set -euo pipefail

readonly APP_NAME="Hex Studio"
readonly ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Keychain profile this project notarises with. Used by default; `--no-notarize` opts out.
#
# Deliberately preferred over the APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID exports in ~/.zshrc:
# those are shared with other projects and keep the app-specific password in a plain-text file,
# whereas notarytool reads this straight from the keychain.
#
# Create it once with:
#   xcrun notarytool store-credentials "hex-studio" \
#     --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_PASSWORD"
readonly DEFAULT_NOTARY_PROFILE="hex-studio"

want_dmg=false
notarize_profile="$DEFAULT_NOTARY_PROFILE"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dmg) want_dmg=true ;;
    --notarize)
      notarize_profile="${2:-}"
      [[ -n "$notarize_profile" ]] || { echo "--notarize needs a keychain profile name" >&2; exit 2; }
      shift
      ;;
    --no-notarize) notarize_profile="" ;;
    -h|--help) sed -n '2,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

# Notarising a .app means zipping it first; a .dmg is submitted directly and is what you would
# actually hand to someone, so ask for one rather than quietly notarising something undistributable.
if [[ -n "$notarize_profile" && "$want_dmg" == false ]]; then
  echo "note: --notarize without --dmg notarises the .app alone. Add --dmg for a distributable." >&2
fi

cd "$ROOT"
source "$HOME/.cargo/env"
export NVM_DIR="$HOME/.nvm"
[[ -s "$NVM_DIR/nvm.sh" ]] && \. "$NVM_DIR/nvm.sh"

step() { printf '\n\033[1;34m==> %s\033[0m\n' "$1"; }

# ---------------------------------------------------------------------------- signing identity
step "Signing identity"
if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  identity="$APPLE_SIGNING_IDENTITY"
  echo "  $identity (from APPLE_SIGNING_IDENTITY)"
else
  identity="$(security find-identity -v -p codesigning \
    | grep "Developer ID Application" \
    | head -1 \
    | sed -E 's/.*"(.*)".*/\1/' || true)"

  if [[ -n "$identity" ]]; then
    echo "  $identity"
    export APPLE_SIGNING_IDENTITY="$identity"
  else
    echo "  none found — building unsigned (right-click → Open to run it locally)"
  fi
fi

# When a notary profile is requested, unset the env-var credentials so Tauri does not
# try to notarise during the build itself. We do it manually afterwards (two-phase), because
# Tauri staples the .app and then builds the dmg *without* submitting the dmg, which is the
# exact bug this script exists to fix.
if [[ -n "$notarize_profile" ]]; then
  unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID
  echo "  notarisation: after the build, via keychain profile '$notarize_profile'"
elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
  echo "  notarisation: Tauri will submit during the build (APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID)"
else
  echo "  notarisation: none — the build will be signed but not notarised"
fi

# ---------------------------------------------------------------------------- build
# Always `--bundles app`: the dmg is built further down, after the app has been notarised and
# stapled, so the copy inside it carries a ticket. Letting Tauri build the dmg here would produce
# one containing an unstapled app.
step "Building (app)"
npm run tauri build -- --bundles app

out_dir="src-tauri/target/release/bundle"
app_path="$out_dir/macos/$APP_NAME.app"
[[ -d "$app_path" ]] || { echo "expected bundle missing: $app_path" >&2; exit 1; }

# ---------------------------------------------------------------------------- verify signature
if [[ -n "$identity" ]]; then
  step "Verifying signature"
  codesign --verify --deep --strict --verbose=2 "$app_path"
  echo "  hardened runtime: $(codesign -dvv "$app_path" 2>&1 | grep -o 'flags=[^ ]*' || echo '?')"
fi

# Submit an artefact and wait for Apple's verdict.
notarize() {
  xcrun notarytool submit "$1" --keychain-profile "$notarize_profile" --wait
}

# ---------------------------------------------------------------------------- notarise the app
if [[ -n "$notarize_profile" ]]; then
  zip_path="$out_dir/macos/$APP_NAME.zip"
  step "Submitting the app to Apple (this uploads the build)"
  ditto -c -k --keepParent "$app_path" "$zip_path"
  notarize "$zip_path"
  rm -f "$zip_path"

  step "Stapling the app"
  xcrun stapler staple "$app_path"
fi

# ---------------------------------------------------------------------------- dmg
if [[ "$want_dmg" == true ]]; then
  step "Building the dmg from the stapled app"
  dmg_dir="$out_dir/dmg"
  version="$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")"
  arch="$(uname -m)"
  dmg_path="$dmg_dir/${APP_NAME}_${version}_${arch}.dmg"

  staging="$(mktemp -d)"
  trap 'rm -rf "$staging"' EXIT
  ditto "$app_path" "$staging/$APP_NAME.app"

  mkdir -p "$dmg_dir"
  rm -f "$dmg_path"
  "$ROOT/scripts/bundle_dmg.sh" \
    --volname "$APP_NAME" \
    --volicon "src-tauri/icons/icon.icns" \
    --icon "$APP_NAME.app" 180 170 \
    --app-drop-link 480 170 \
    --window-size 660 400 \
    --hide-extension "$APP_NAME.app" \
    "$dmg_path" "$staging"

  if [[ -n "$identity" ]]; then
    codesign --force --sign "$identity" "$dmg_path"
  fi

  if [[ -n "$notarize_profile" ]]; then
    step "Submitting the dmg to Apple"
    notarize "$dmg_path"
    step "Stapling the dmg"
    xcrun stapler staple "$dmg_path"
  fi
fi

# ---------------------------------------------------------------------------- verify
# Each check is recorded and the script fails at the end rather than at the first problem, so one
# run tells you everything that is wrong with the artefact.
#
# These *fail* the build. Printing a warning and exiting 0 is exactly how v0.3.3 shipped a dmg that
# was signed but never submitted: the build said "Done" while the artefact was undistributable. A
# release script that cannot fail is not verifying anything.
step "Verifying the result"
failures=()

# check <description> <command...> — quiet when it passes, shows the output when it does not.
check() {
  local what="$1"; shift
  local out
  if out="$("$@" 2>&1)"; then
    printf '  ok    %s\n' "$what"
  else
    printf '  FAIL  %s\n' "$what" >&2
    printf '%s\n' "$out" | sed 's/^/          /' >&2
    failures+=("$what")
  fi
}

if [[ -n "$identity" ]]; then
  check "app accepted by Gatekeeper" spctl --assess --type exec "$app_path"
fi

if [[ -n "$notarize_profile" ]]; then
  check "app has a stapled ticket" xcrun stapler validate "$app_path"

  if [[ "$want_dmg" == true ]]; then
    check "dmg has a stapled ticket" xcrun stapler validate "$dmg_path"
    # The assessment that failed on v0.3.3. `-t install` is the one a receiving Mac makes of a disk
    # image; the default `exec` type says nothing useful about a dmg.
    check "dmg accepted for install" spctl -a -vvv -t install "$dmg_path"

    # The ticket on the dmg says nothing about the copy inside it, and that copy is what people
    # drag to Applications.
    mnt="$(mktemp -d)"
    if hdiutil attach -nobrowse -quiet -mountpoint "$mnt" "$dmg_path"; then
      check "app inside the dmg has a stapled ticket" xcrun stapler validate "$mnt/$APP_NAME.app"
      hdiutil detach -quiet "$mnt" || true
    else
      echo "  FAIL  dmg could not be mounted" >&2
      failures+=("dmg could not be mounted")
    fi
    rmdir "$mnt" 2>/dev/null || true
  fi

  # Apple's own pre-distribution checker, which judges an artefact the way the receiving Mac will.
  # Requires macOS 14+, so a missing tool is a note rather than a failure — the checks above already
  # cover notarisation.
  if command -v syspolicy_check >/dev/null; then
    check "app passes pre-distribution checks" syspolicy_check distribution "$app_path"
    if [[ "$want_dmg" == true ]]; then
      check "dmg passes pre-distribution checks" syspolicy_check distribution "$dmg_path"
    fi
  else
    echo "  note: syspolicy_check not on this system (needs macOS 14+); skipped" >&2
  fi
fi

if (( ${#failures[@]} > 0 )); then
  printf '\n\033[1;31m==> %d check(s) failed — do not distribute this build\033[0m\n' \
    "${#failures[@]}" >&2
  printf '    - %s\n' "${failures[@]}" >&2
  exit 1
fi

step "Done"
echo "  app: $app_path"
[[ "$want_dmg" == true ]] && echo "  dmg: $dmg_path"
exit 0
