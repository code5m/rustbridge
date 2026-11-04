#!/usr/bin/env bash
# Invoke inside xvfb-run with a 1440x900x24 display. No real user files touched.
set -euo pipefail
BINARY="${1:?path to rustbridge binary required}"
BINARY="$(realpath "$BINARY")"
ROOT="$(mktemp -d)"
APP=""
cleanup() {
  if [[ -n "$APP" ]]; then kill "$APP" 2>/dev/null || true; wait "$APP" 2>/dev/null || true; fi
  rm -rf "$ROOT"
}
trap cleanup EXIT
unset WAYLAND_DISPLAY
export WINIT_UNIX_BACKEND=x11 XDG_SESSION_TYPE=x11 GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1

# A window may be discoverable before it is viewable on a headless runner.
# Avoid a fatal X_SetInputFocus BadMatch; focus only a mapped window and retry.
focus_viewable_window() {
  local window="$1" active=""
  for attempt in $(seq 1 40); do
    if xdotool windowfocus "$window" >/dev/null 2>&1; then
      active="$(xdotool getwindowfocus 2>/dev/null || true)"
      if [[ "$active" == "$window" ]]; then return 0; fi
    fi
    sleep 0.25
  done
  echo "GUI_FOCUS_FAILED window=$window focus=$active"
  return 1
}
printf 'struct User { name: String }\n' > "$ROOT/small.rs"
"$BINARY" "$ROOT/small.rs" > "$ROOT/log" 2>&1 &
APP=$!
WIN=""
for i in $(seq 1 120); do
  WIN="$(xdotool search --onlyvisible --name RustBridge 2>/dev/null | head -n 1 || true)"
  [[ -n "$WIN" ]] && break
  if ! kill -0 "$APP" 2>/dev/null; then
    echo "GUI_PROCESS_TERMINATED"; cat "$ROOT/log" "$ROOT/large.log" 2>/dev/null || true
    exit 1
  fi
  sleep 0.25
done
if [[ -z "$WIN" ]]; then
  echo "GUI_WINDOW_TIMEOUT"; cat "$ROOT/log"; exit 1
fi
kill -0 "$APP"
# Window creation can precede the first completed egui frame or async source load.
sleep 2
focus_viewable_window "$WIN"
xdotool key ctrl+2
sleep 1
xdotool mousemove --sync 343 161 click 1
xdotool key End
xdotool type --clearmodifiers --delay 15 ' // SMOKE_SAVE'
sleep 1
xdotool key --clearmodifiers ctrl+s
for i in $(seq 1 35); do
  if grep -q SMOKE_SAVE "$ROOT/small.rs"; then break; fi
  sleep 0.2
done
if ! grep -q SMOKE_SAVE "$ROOT/small.rs"; then
  echo "SMALL_SAVE_GUI_FAIL"; cat "$ROOT/log"; exit 1
fi
echo "SMALL_SAVE_GUI_PASS"
kill "$APP" 2>/dev/null || true
wait "$APP" 2>/dev/null || true
APP=""
# >512 KiB and CRLF to exercise 240-line paging and byte-preserving save.
python3 - "$ROOT/large.rs" <<'PY'
from pathlib import Path
import sys
Path(sys.argv[1]).write_bytes(b''.join(f'fn f_{i}() {{}}'.encode()+b'\r\n' for i in range(35000)))
PY
"$BINARY" "$ROOT/large.rs" > "$ROOT/large.log" 2>&1 &
APP=$!
WIN=""
for i in $(seq 1 120); do
  WIN="$(xdotool search --onlyvisible --name RustBridge 2>/dev/null | head -n 1 || true)"
  [[ -n "$WIN" ]] && break
  if ! kill -0 "$APP" 2>/dev/null; then
    echo "GUI_PROCESS_TERMINATED"; cat "$ROOT/log" "$ROOT/large.log" 2>/dev/null || true
    exit 1
  fi
  sleep 0.25
done
if [[ -z "$WIN" ]]; then
  echo "PAGED_GUI_WINDOW_TIMEOUT"; cat "$ROOT/large.log"; exit 1
fi
kill -0 "$APP"
# Window creation can precede the first completed egui frame or async source load.
sleep 2
focus_viewable_window "$WIN"
xdotool key ctrl+2
sleep 1
xdotool key ctrl+Next
xdotool keyup Control_L Control_R 2>/dev/null || true
sleep 1
xdotool mousemove --sync 342 163 click 1
xdotool key End
xdotool type --clearmodifiers --delay 12 ' // SMOKE_PAGED'
sleep 1
xdotool key --clearmodifiers ctrl+s
for i in $(seq 1 50); do
  if grep -q SMOKE_PAGED "$ROOT/large.rs"; then break; fi
  sleep 0.2
done
# On some virtual-display keymaps the first Ctrl+S is swallowed while a
# paginated text edit requests another frame. Retry then exercise the
# visible Save button so the source persistence invariant is still proven.
if ! grep -q SMOKE_PAGED "$ROOT/large.rs"; then
  xdotool keyup Control_L Control_R 2>/dev/null || true
  xdotool key --clearmodifiers ctrl+s
  for i in $(seq 1 25); do
    if grep -q SMOKE_PAGED "$ROOT/large.rs"; then break; fi
    sleep 0.2
  done
fi
if ! grep -q SMOKE_PAGED "$ROOT/large.rs"; then
  echo "LARGE_SHORTCUT_UNDELIVERED_VERIFY_SAVE_BUTTON"
  xdotool mousemove --sync 405 70 click 1
  for i in $(seq 1 25); do
    if grep -q SMOKE_PAGED "$ROOT/large.rs"; then break; fi
    sleep 0.2
  done
fi
python3 - "$ROOT/large.rs" <<'PY'
from pathlib import Path
import sys
b=Path(sys.argv[1]).read_bytes()
assert b'SMOKE_PAGED' in b, "PAGED_GUI_SAVE_MISSING"
assert b.count(b'\r\n') == b.count(b'\n') == 35000, "CRLF line endings corrupted"
assert b' // SMOKE_PAGED\r\n' in b, "Paged insertion at wrong byte boundary"
print("PAGED_GUI_CRLF_SAVE_PASS")
PY
echo "GUI_SMOKE_PASS"
