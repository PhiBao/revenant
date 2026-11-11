#!/usr/bin/env bash
# Start the real server, restore the seeded example, and verify the download the way the
# certificate tells a visitor to: matching SHA-256, and no archive host in any file.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

port="${E2E_PORT:-18099}"
work="$(mktemp -d)"
server_pid=""
cleanup() {
  [[ -n "$server_pid" ]] && kill "$server_pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

cargo build --quiet -p revenant-api
PORT="$port" REVENANT_WEB_DIR="$root/apps/web/out" ./target/debug/revenant-api >"$work/server.log" 2>&1 &
server_pid=$!

for _ in $(seq 1 40); do
  curl -fs "localhost:$port/api/health" >/dev/null 2>&1 && break
  sleep 0.25
done
curl -fs "localhost:$port/api/health" >/dev/null || { echo "server did not start"; cat "$work/server.log"; exit 1; }

job="$(curl -fs -X POST "localhost:$port/api/resurrections" \
  -H 'content-type: application/json' \
  -d '{"example":"y2k-briefings","date":"2000-03-02","policy":"include-anachronisms"}')"
id="$(printf '%s' "$job" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
want="$(printf '%s' "$job" | python3 -c 'import json,sys; print(json.load(sys.stdin)["zip_sha256"])')"
mode="$(printf '%s' "$job" | python3 -c 'import json,sys; print(json.load(sys.stdin)["mode"])')"

curl -fs -o "$work/r.zip" "localhost:$port/api/resurrections/$id/archive.zip"
got="$(sha256sum "$work/r.zip" | cut -d' ' -f1)"
[[ "$got" == "$want" ]] || { echo "FAIL: ZIP hash $got != reported $want"; exit 1; }
echo "ok: ZIP hash matches the API ($mode)"

mkdir "$work/out"
unzip -q "$work/r.zip" -d "$work/out"
if grep -r "web.archive.org" "$work/out" >/dev/null; then
  echo "FAIL: archive host found in the restored files"
  grep -r "web.archive.org" "$work/out" | head -3
  exit 1
fi
echo "ok: no web.archive.org in the restored files"

headers="$(curl -fsI "localhost:$port/p/$id/")"
for h in 'content-security-policy: sandbox' 'x-content-type-options: nosniff' 'referrer-policy: no-referrer'; do
  printf '%s' "$headers" | tr -d '\r' | grep -qi "^$h" || { echo "FAIL: preview is missing '$h'"; exit 1; }
done
echo "ok: preview carries the sandbox headers"

curl -fs "localhost:$port/" | grep -q "Revenant" || { echo "FAIL: web app is not served"; exit 1; }
echo "ok: web app is served"
