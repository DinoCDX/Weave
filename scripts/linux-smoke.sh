#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/.."

OUT="${1:-target/triangle-smoke.png}"
mkdir -p "$(dirname "$OUT")"

for tool in xvfb-run import; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "missing $tool: install xvfb and imagemagick (apt install xvfb imagemagick)" >&2
        exit 1
    fi
done

cargo build -p triangle --locked

xvfb-run -a -s "-screen 0 800x600x24" sh -c '
    ./target/debug/triangle >/dev/null 2>&1 &
    pid=$!
    sleep 6
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "triangle exited early" >&2
        exit 1
    fi
    import -window root "$1"
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
' sh "$OUT"

echo "triangle ran for 6 seconds on a virtual display; screenshot at $OUT"
