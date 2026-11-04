#!/usr/bin/env bash
# Run with xvfb-run -a -s '-screen 0 1440x900x24' bash tests/resource-smoke.sh ./target/release/rustbridge
set -euo pipefail
BINARY="$(realpath "${1:?path to RustBridge binary}")"
DIR="$(mktemp -d)"
APP=""
cleanup() {
  if [[ -n "$APP" ]]; then kill "$APP" 2>/dev/null || true; wait "$APP" 2>/dev/null || true; fi
  rm -rf "$DIR"
}
trap cleanup EXIT
unset WAYLAND_DISPLAY
export WINIT_UNIX_BACKEND=x11 XDG_SESSION_TYPE=x11 GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1
python3 - "$DIR/large.rs" <<'PY'
from pathlib import Path
import sys
Path(sys.argv[1]).write_bytes(b''.join(f'fn f_{i}() {{ let x = {i}; }}'.encode()+b'\r\n' for i in range(40000)))
PY
"$BINARY" "$DIR/large.rs" > "$DIR/stderr" 2>&1 &
APP=$!
WIN=""
for i in $(seq 1 120); do
  WIN="$(xdotool search --name RustBridge 2>/dev/null | head -n 1 || true)"
  [[ -n "$WIN" ]] && break
  sleep 0.25
done
if [[ -z "$WIN" ]]; then cat "$DIR/stderr"; echo RESOURCE_WINDOW_NOT_FOUND; exit 1; fi
sleep 6
kill -0 "$APP"
if [[ ! -r "/proc/$APP/stat" ]]; then echo RESOURCE_PROCESS_MISSING; exit 1; fi
cpu_a="$(awk '{print $14+$15}' "/proc/$APP/stat")"
rss_a="$(awk '/VmRSS:/ {print $2}' "/proc/$APP/status")"
fd_a="$(find "/proc/$APP/fd" -mindepth 1 -maxdepth 1 | wc -l)"
sleep 5
kill -0 "$APP"
cpu_b="$(awk '{print $14+$15}' "/proc/$APP/stat")"
rss_b="$(awk '/VmRSS:/ {print $2}' "/proc/$APP/status")"
fd_b="$(find "/proc/$APP/fd" -mindepth 1 -maxdepth 1 | wc -l)"
hz="$(getconf CLK_TCK)"
awk -v a="$cpu_a" -v b="$cpu_b" -v hz="$hz" -v x="$rss_a" -v y="$rss_b" -v p="$fd_a" -v q="$fd_b" 'BEGIN {
  printf "RESOURCE_SAMPLE idle_cpu_single_core=%.2f%% rss_kib=%d_to_%d fds=%d_to_%d\n", 100*(b-a)/(5*hz),x,y,p,q;
  if (y > 524288 || q > 128 || y-x > 65536) {
    print "RESOURCE_BUDGET_EXCEEDED";
    exit 3;
  }
}'
echo RESOURCE_SMOKE_PASS
