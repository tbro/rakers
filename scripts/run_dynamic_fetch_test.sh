#!/usr/bin/env bash
set -euo pipefail

# Run a stubbed and a live-server dynamic-script fetch test for rakers.
# Exits non-zero if fetched content is not observed.

ROOT=$(pwd)
PAGES_DIR="$ROOT/scripts/test_pages"
mkdir -p "$PAGES_DIR/server"

# Stubbed test page (no network required)
cat > "$PAGES_DIR/stubbed.html" <<'HTML'
<!doctype html>
<html><head></head><body>
<script>
  // Provide a stubbed native fetch used by the bootstrap
  window._r_fetch_sync = function(url) {
    return "document.write('<p>fetched-via-stub</p>');";
  };
</script>
<script>
  var s = document.createElement('script');
  s.src = 'https://example.com/fetch.js';
  document.body.appendChild(s);
</script>
</body></html>
HTML

# Live-server fetch page and server-side script
cat > "$PAGES_DIR/server/fetch.js" <<'JS'
document.write('<p>fetched-from-server</p>');
JS

cat > "$PAGES_DIR/test.html" <<'HTML'
<!doctype html>
<html><head></head><body>
<script>
  var s = document.createElement('script');
  s.src = 'http://127.0.0.1:8000/fetch.js';
  document.body.appendChild(s);
</script>
</body></html>
HTML

# Build rakers
cargo build --quiet

RAKERS_BIN="$ROOT/target/debug/rakers"
if [ ! -x "$RAKERS_BIN" ]; then
  echo "rakers binary not found at $RAKERS_BIN" >&2
  exit 2
fi

# Run stubbed test
OUT1=$(mktemp)
ERR1=$(mktemp)
"$RAKERS_BIN" "$PAGES_DIR/stubbed.html" > "$OUT1" 2> "$ERR1" || true
if ! grep -q "fetched-via-stub" "$OUT1"; then
  echo "[stubbed] expected fetched content not found" >&2
  echo "--- stdout ---" >&2; sed -n '1,200p' "$OUT1" >&2
  echo "--- stderr ---" >&2; sed -n '1,200p' "$ERR1" >&2
  rm -f "$OUT1" "$ERR1"
  exit 3
fi
rm -f "$OUT1" "$ERR1"

echo "[stubbed] OK"

# Run live-server test (start server)
cd "$PAGES_DIR/server"
nohup python3 -m http.server 8000 > /tmp/rakers-ci-server.log 2>&1 &
PID=$!
cd "$ROOT"
trap 'kill $PID 2>/dev/null || true' EXIT
sleep 0.5

OUT2=$(mktemp)
ERR2=$(mktemp)
"$RAKERS_BIN" "$PAGES_DIR/test.html" > "$OUT2" 2> "$ERR2" || true
# Stop server
kill $PID 2>/dev/null || true
trap - EXIT

if ! grep -q "fetched-from-server" "$OUT2"; then
  echo "[live] expected fetched content not found" >&2
  echo "--- stdout ---" >&2; sed -n '1,200p' "$OUT2" >&2
  echo "--- stderr ---" >&2; sed -n '1,200p' "$ERR2" >&2
  rm -f "$OUT2" "$ERR2"
  exit 4
fi
rm -f "$OUT2" "$ERR2"

echo "[live] OK"

echo "Dynamic fetch smoke tests passed"
exit 0
