#!/usr/bin/env bash
# Runs a browser check page in headless Chrome and prints its verdict.
#
#     web/run-browser-check.sh golden.html    # cross-target golden corpus
#     web/run-browser-check.sh selftest.html  # the editor, driven like a user
#
# Build the wasm modules and copy them to web/ first (see README). Set CHROME
# to the browser executable when it is not on the usual path.
set -u
page="${1:-golden.html}"
root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
port="${PORT:-8731}"
chrome="${CHROME:-}"
if [ -z "$chrome" ]; then
  for c in google-chrome chromium chromium-browser "/c/Program Files/Google/Chrome/Application/chrome.exe" "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"; do
    if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then chrome="$c"; break; fi
  done
fi
py="$(command -v python || command -v python3)"
"$py" "$root/web/serve.py" --port "$port" --result "$work/result.txt" 2>/dev/null &
server=$!
sleep 1
"$chrome" --headless=new --disable-gpu --no-first-run --user-data-dir="$work/profile" "http://127.0.0.1:$port/web/$page" >/dev/null 2>&1 &
browser=$!
for _ in $(seq 1 120); do
  [ -f "$work/result.txt" ] && break
  sleep 1
done
kill "$browser" 2>/dev/null
pkill -f "$work/profile" 2>/dev/null || true
kill "$server" 2>/dev/null
if [ -f "$work/result.txt" ]; then
  cat "$work/result.txt"
  echo
  grep -q -- "-PASS" "$work/result.txt"
else
  echo "no verdict after 120 seconds"
  exit 1
fi
