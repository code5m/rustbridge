#!/usr/bin/env bash
# Drive a genuine X11 RustBridge edit, kill unexpectedly, then restart and
# check that only a journal, not an implicit source overwrite, survives.
set -euo pipefail
BINARY="$(realpath "$1")"
ROOT="$(mktemp -d /tmp/rustbridge-crash-recovery-XXXXXX)"
APP=""
cleanup(){
  if [[ -n "$APP" ]]; then kill "$APP" 2>/dev/null || true; wait "$APP" 2>/dev/null || true; fi
  rm -rf "$ROOT"
}
trap cleanup EXIT
unset WAYLAND_DISPLAY
export XDG_SESSION_TYPE=x11 WINIT_UNIX_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1
export RUSTBRIDGE_STATE_DIR="$ROOT/state"
printf 'fn main() {}\n' > "$ROOT/code.rs"
"$BINARY" "$ROOT/code.rs" > "$ROOT/first.log" 2>&1 &
APP=$!
WIN=""
for i in $(seq 1 120); do
  WIN="$(xdotool search --onlyvisible --name RustBridge 2>/dev/null | head -n 1 || true)"
  [[ -n "$WIN" ]] && break
  sleep 0.25
done
[[ -n "$WIN" ]] || { cat "$ROOT/first.log"; echo "RECOVERY_GUI_MISSING"; exit 1; }
sleep 2
xdotool windowfocus "$WIN" >/dev/null 2>&1 || true
xdotool key --clearmodifiers ctrl+2
sleep 0.7
xdotool mousemove --sync 344 161 click 1
xdotool key End
xdotool type --clearmodifiers --delay 19 ' // UNSAVED_RECOVERY_MARKER'
sleep 3
JOURNAL="$(find "$ROOT/state" -maxdepth 1 -name 'recover-*.json' -print -quit)"
if [[ -z "$JOURNAL" ]]; then
  echo "RECOVERY_JOURNAL_NOT_WRITTEN"; cat "$ROOT/first.log"; exit 2
fi
grep -q UNSAVED_RECOVERY_MARKER "$JOURNAL" || {
  echo "RECOVERY_JOURNAL_CONTENT_NOT_FOUND"; exit 2;
}
if grep -q UNSAVED_RECOVERY_MARKER "$ROOT/code.rs"; then
  echo "UNSAVED_CHANGES_WRONGFULLY_PERSISTED"; exit 2
fi
kill -9 "$APP" 2>/dev/null || true
wait "$APP" 2>/dev/null || true
APP=""
"$BINARY" "$ROOT/code.rs" > "$ROOT/second.log" 2>&1 &
APP=$!
WIN=""
for i in $(seq 1 120); do
  WIN="$(xdotool search --onlyvisible --name RustBridge 2>/dev/null | head -n 1 || true)"
  [[ -n "$WIN" ]] && break
  sleep 0.25
done
[[ -n "$WIN" ]] || { cat "$ROOT/second.log"; echo "RECOVERY_RESTART_WINDOW_MISSING"; exit 2; }
sleep 3
kill -0 "$APP"
grep -q UNSAVED_RECOVERY_MARKER "$JOURNAL"
if grep -q UNSAVED_RECOVERY_MARKER "$ROOT/code.rs"; then
  echo "RESTART_SILENTLY_OVERWROTE_SOURCE"; exit 2
fi
if [[ "$#" -gt 1 ]] && command -v scrot >/dev/null; then
  scrot "$2" || true
fi
echo "CRASH_RECOVERY_JOURNAL_RESTART_PASS"
