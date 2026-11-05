#!/usr/bin/env bash
# Real X11 interaction: enter Compare, collapse & expand AST regions.
# Invoke under xvfb-run -a -s '-screen 0 1440x900x24'.
set -euo pipefail
BINARY="$(realpath "${1:?path to RustBridge binary}")"
ROOT="$(mktemp -d)"
APP=""
cleanup() {
  if [[ -n "$APP" ]]; then kill "$APP" 2>/dev/null || true; wait "$APP" 2>/dev/null || true; fi
  rm -rf "$ROOT"
}
trap cleanup EXIT
export RUSTBRIDGE_STATE_DIR="$ROOT/state"
unset WAYLAND_DISPLAY
export WINIT_UNIX_BACKEND=x11 XDG_SESSION_TYPE=x11 GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1
cp "$(dirname "$0")/../src/line_index.rs" "$ROOT/line_index.rs"
cp "$ROOT/line_index.rs" "$ROOT/source_original.rs"
"$BINARY" "$ROOT/line_index.rs" >"$ROOT/gui.log" 2>&1 &
APP=$!
WIN=""
for attempt in $(seq 1 100); do
  WIN="$(xdotool search --onlyvisible --name RustBridge 2>/dev/null | head -n 1 || true)"
  [[ -n "$WIN" ]] && break
  if ! kill -0 "$APP" 2>/dev/null; then
    echo "COMPARE_FOLD_GUI_EXITED"; cat "$ROOT/gui.log"; exit 1
  fi
  sleep 0.15
done
if [[ -z "$WIN" ]]; then echo "COMPARE_FOLD_GUI_WINDOW_TIMEOUT"; cat "$ROOT/gui.log"; exit 1; fi
xdotool windowmove "$WIN" 0 0
xdotool windowsize "$WIN" 1320 840
sleep 3
for attempt in $(seq 1 30); do
  if xdotool windowfocus "$WIN" >/dev/null 2>&1; then break; fi
  sleep 0.2
done
# Compare mode is a visible toolbar control, independent of keymap layout.
xdotool mousemove --sync 612 13 click 1
sleep 2
xwd -silent -id "$WIN" -out "$ROOT/before.xwd"
# Collapse All in the Java half-pane, leaving the Rust half-pane untouched.
xdotool mousemove --sync 371 155 click 1
sleep 2
xwd -silent -id "$WIN" -out "$ROOT/collapsed.xwd"
python3 - "$ROOT/before.xwd" "$ROOT/collapsed.xwd" <<'PY'
from pathlib import Path
import sys
a,b=(Path(path).read_bytes() for path in sys.argv[1:])
assert len(a)==len(b) and len(a)>1000000, 'COMPARE_FOLD_GUI_CAPTURE_INVALID'
changed=sum(left!=right for left,right in zip(a[2048:],b[2048:]))
print('COMPARE_COLLAPSE_CHANGED_BYTES',changed)
assert changed>20000, 'COMPARE_FOLD_COLLAPSE_MISSING'
PY
# Expand All must restore the visible code lines without changing the source.
xdotool mousemove --sync 423 155 click 1
sleep 2
xwd -silent -id "$WIN" -out "$ROOT/expanded.xwd"
python3 - "$ROOT/collapsed.xwd" "$ROOT/expanded.xwd" <<'PY'
from pathlib import Path
import sys
a,b=(Path(path).read_bytes() for path in sys.argv[1:])
assert len(a)==len(b), 'COMPARE_EXPAND_GUI_CAPTURE_INVALID'
changed=sum(left!=right for left,right in zip(a[2048:],b[2048:]))
print('COMPARE_EXPAND_CHANGED_BYTES',changed)
assert changed>20000, 'COMPARE_FOLD_EXPAND_MISSING'
PY
cmp "$ROOT/line_index.rs" "$ROOT/source_original.rs"
kill -0 "$APP"
echo "COMPARE_FOLD_GUI_PASS original_rust_unchanged"
