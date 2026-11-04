#!/usr/bin/env bash
# Reproducible GUI soak against a temporary Rust file, never a user source file.
# Usage: xvfb-run -a bash tests/soak.sh ./target/release/rustbridge 300
set -euo pipefail
BINARY="$(realpath "$1")"
DURATION_SECONDS="$2"
if [[ "$DURATION_SECONDS" -lt 30 ]]; then echo "Minimum soak is 30s"; exit 2; fi
ROOT="$(mktemp -d /tmp/rustbridge-soak-XXXXXX)"
APP=""
cleanup() {
  if [[ -n "$APP" ]]; then kill "$APP" 2>/dev/null || true; wait "$APP" 2>/dev/null || true; fi
  rm -rf "$ROOT"
}
trap cleanup EXIT
export RUSTBRIDGE_STATE_DIR="$ROOT/state"
unset WAYLAND_DISPLAY
export XDG_SESSION_TYPE=x11 WINIT_UNIX_BACKEND=x11 GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1
python3 - "$ROOT/soak.rs" <<'PY'
from pathlib import Path
import sys
Path(sys.argv[1]).write_bytes(
 b''.join(f'pub fn value_{i}() -> i32 {{ {i} }}'.encode()+b'\r\n'
          for i in range(42000))
)
PY
"$BINARY" "$ROOT/soak.rs" > "$ROOT/app.log" 2>&1 &
APP=$!
WIN=""
for i in $(seq 1 140); do
  WIN="$(xdotool search --onlyvisible --name RustBridge 2>/dev/null | head -n 1 || true)"
  [[ -n "$WIN" ]] && break
  if ! kill -0 "$APP" 2>/dev/null; then
    echo "SOAK_APP_CRASHED"; tail -n 20 "$ROOT/app.log"; exit 1
  fi
  sleep 0.25
done
[[ -n "$WIN" ]] || { echo "SOAK_NO_WINDOW"; exit 1; }
for i in $(seq 1 45); do
  xdotool windowfocus "$WIN" >/dev/null 2>&1 || true
  [[ "$(xdotool getwindowfocus 2>/dev/null || true)" == "$WIN" ]] && break
  sleep 0.3
done
[[ "$(xdotool getwindowfocus 2>/dev/null || true)" == "$WIN" ]] || { echo "SOAK_NO_FOCUS"; exit 1; }
sleep 7
BASE_RSS="$(awk '/VmRSS:/ {print $2}' "/proc/$APP/status")"
BASE_FD="$(find "/proc/$APP/fd" -mindepth 1 -maxdepth 1 | wc -l)"
MAX_RSS="$BASE_RSS"
MAX_FD="$BASE_FD"
SECONDS_ELAPSED=0
SAMPLES=0
while (( SECONDS_ELAPSED < DURATION_SECONDS )); do
  kill -0 "$APP" || { echo "SOAK_APP_DIED"; exit 1; }
  case $((SAMPLES % 4)) in
    0) xdotool key --clearmodifiers ctrl+2;;
    1) xdotool key --clearmodifiers ctrl+3;;
    2) xdotool key --clearmodifiers ctrl+1;;
    3) xdotool key --clearmodifiers ctrl+2;;
  esac
  sleep 5
  SECONDS_ELAPSED=$((SECONDS_ELAPSED + 5))
  SAMPLES=$((SAMPLES + 1))
  RSS="$(awk '/VmRSS:/ {print $2}' "/proc/$APP/status")"
  FD="$(find "/proc/$APP/fd" -mindepth 1 -maxdepth 1 | wc -l)"
  (( RSS > MAX_RSS )) && MAX_RSS="$RSS" || true
  (( FD > MAX_FD )) && MAX_FD="$FD" || true
  if (( RSS > 524288 || FD > 128 || RSS > BASE_RSS + 131072 || FD > BASE_FD + 12 )); then
    echo "SOAK_BUDGET_EXCEEDED elapsed=$SECONDS_ELAPSED RSS=$RSS BASE_RSS=$BASE_RSS FDs=$FD BASE_FDs=$BASE_FD"
    exit 3
  fi
  if (( SAMPLES % 12 == 0 )); then
    echo "SOAK_SAMPLE elapsed=$SECONDS_ELAPSED RSS_KiB=$RSS FD=$FD"
  fi
done
echo "SOAK_PASS elapsed=$SECONDS_ELAPSED samples=$SAMPLES rss_base_kib=$BASE_RSS rss_peak_kib=$MAX_RSS fd_base=$BASE_FD fd_peak=$MAX_FD"
