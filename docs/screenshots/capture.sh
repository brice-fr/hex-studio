#!/usr/bin/env bash
# Regenerate the README screenshots.
#
# Renders the app's own components against data decoded from the ASAM demo pair
# by the Rust backend, so the images show real values rather than mock-ups. The
# harness route is created under src/routes and removed again, so nothing extra
# ships in the app.
#
# Requires the ASAM demo pair (A2L + hex), which is ASAM-licensed and not in
# this repository. Point A2L_DEMO_DIR at a directory holding
# ASAP2_Demo_V171.a2l and ASAP2_Demo_V171.hex.
#
#   ./docs/screenshots/capture.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEMO="${A2L_DEMO_DIR:-$HOME/Downloads/ECU_Description}"
OUT="$ROOT/docs/screenshots"
ROUTE="$ROOT/src/routes/__shots"
PORT="${PORT:-5199}"
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
# 1x keeps the images small enough to live in the repo. Raise to 2 for crisp
# retina captures if the size is ever worth paying for.
SCALE="${SCALE:-1}"
# Light reads better against GitHub's own light default, which is what most
# people see the README in. THEME=dark for the other one.
THEME="${THEME:-light}"

[ -f "$DEMO/ASAP2_Demo_V171.a2l" ] || { echo "demo A2L not found in $DEMO" >&2; exit 1; }
[ -x "$CHROME" ] || { echo "Chrome not found at $CHROME" >&2; exit 1; }

cleanup() { rm -rf "$ROUTE"; [ -n "${VITE_PID:-}" ] && kill "$VITE_PID" 2>/dev/null || true; }
trap cleanup EXIT

mkdir -p "$ROUTE"

echo "==> decoding the demo pair"
( cd "$ROOT/src-tauri" && A2L_DEMO_DIR="$DEMO" DUMP_TO="$ROUTE" \
    cargo test -q -p a2l-data --test demo_file dump_ui -- --ignored >/dev/null )

echo "==> reading the hex image"
python3 - "$DEMO/ASAP2_Demo_V171.hex" "$ROUTE/records.json" <<'PY'
import json, sys
recs, base = [], 0
for line in open(sys.argv[1]):
    line = line.strip()
    if not line.startswith(':'):
        continue
    b = bytes.fromhex(line[1:])
    n, addr, rt = b[0], (b[1] << 8) | b[2], b[3]
    data = list(b[4:4 + n])
    if rt == 0:
        recs.append({"record_type": "Data", "address": base + addr, "data": data})
    elif rt == 4:
        base = ((data[0] << 8) | data[1]) << 16
json.dump(recs, open(sys.argv[2], 'w'))
PY

cp "$OUT/harness.svelte" "$ROUTE/+page.svelte"
cp "$ROOT/src-tauri/icons/128x128@2x.png" "$ROUTE/icon.png"

echo "==> serving on :$PORT"
( cd "$ROOT" && npx vite dev --port "$PORT" --strictPort >/tmp/hex-shots-vite.log 2>&1 & echo $! >/tmp/hex-shots.pid )
VITE_PID="$(cat /tmp/hex-shots.pid)"
for _ in $(seq 1 40); do
  curl -sf -o /dev/null "http://localhost:$PORT/__shots" && break
  sleep 0.5
done

# The social preview has its own aspect: GitHub wants 1280x640 and crops
# anything else. It is captured at 1x regardless, since it must stay under 1 MB.
for shot in hex data map og; do
  case "$shot" in
    og) size="1280,640"; scale=1 ;;
    *)  size="1280,800"; scale="$SCALE" ;;
  esac
  echo "==> capturing $shot"
  "$CHROME" --headless --disable-gpu --hide-scrollbars --force-device-scale-factor="$scale" \
    --virtual-time-budget=5000 --window-size="$size" \
    --screenshot="$OUT/$shot.png" \
    "http://localhost:$PORT/__shots?shot=$shot&theme=$THEME" >/dev/null 2>&1
done

echo "==> done"
ls -la "$OUT"/*.png
