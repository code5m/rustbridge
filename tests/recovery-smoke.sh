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
xdotool windowfocus --sync "$WIN"
echo "RECOVERY_WINDOW=$WIN FOCUS=$(xdotool getwindowfocus 2>/dev/null || true)"
grep -q UNSAVED_RECOVERY_MARKER "$JOURNAL"
if grep -q UNSAVED_RECOVERY_MARKER "$ROOT/code.rs"; then
  echo "RESTART_SILENTLY_OVERWROTE_SOURCE"; exit 2
fi
# Prove the user can restore into the editor, continue editing, and explicitly
# save. The original source remains untouched until this click/keyboard action.
xdotool mousemove --sync 500 442 click 1
sleep 2
if command -v scrot >/dev/null; then scrot /tmp/rustbridge-recovery-after-click.png; fi
xdotool mousemove --sync 400 130 click 1
xdotool key End
xdotool type --clearmodifiers --delay 18 ' // RECOVERY_RESTORED_AND_EDITABLE'
sleep 1
xdotool key --clearmodifiers ctrl+s
for i in $(seq 1 50); do
  if grep -q RECOVERY_RESTORED_AND_EDITABLE "$ROOT/code.rs"; then break; fi
  sleep 0.2
done
grep -q UNSAVED_RECOVERY_MARKER "$ROOT/code.rs" || {
  echo "RECOVERY_BUFFER_NOT_RESTORED"; cat "$ROOT/second.log"; if command -v scrot >/dev/null; then scrot /tmp/rustbridge-recovery-after-failed-restore.png; fi; exit 3;
}
grep -q RECOVERY_RESTORED_AND_EDITABLE "$ROOT/code.rs" || {
  echo "RECOVERED_BUFFER_NOT_EDITABLE_OR_SAVE_FAILED"; exit 3;
}
for i in $(seq 1 25); do
  [[ ! -e "$JOURNAL" ]] && break
  sleep 0.2
done
[[ ! -e "$JOURNAL" ]] || { echo "SAVED_RECOVERY_JOURNAL_NOT_CLEARED"; exit 3; }
if [[ "$#" -gt 1 ]] && command -v scrot >/dev/null; then
  scrot "$2" || true
fi
echo "CRASH_RECOVERY_RESTART_RESTORE_EDIT_SAVE_PASS"
