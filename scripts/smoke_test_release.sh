#!/usr/bin/env bash
# Smoke-test the latest GitHub release binary.
#
# Usage:
#   ./scripts/smoke_test_release.sh           # downloads latest release for this platform
#   ./scripts/smoke_test_release.sh ./rakers  # tests a pre-built binary
#
# Exits 0 if all tests pass, 1 if any fail.

set -euo pipefail

PASS=0
FAIL=0

pass() { echo "  PASS  $1"; PASS=$((PASS + 1)); }
fail() { echo "  FAIL  $1"; FAIL=$((FAIL + 1)); }

check() {
    local label="$1" output="$2" pattern="$3"
    if echo "$output" | grep -qF -- "$pattern"; then
        pass "$label"
    else
        fail "$label (pattern: $pattern)"
        echo "        output: $(echo "$output" | head -3)"
    fi
}

check_absent() {
    local label="$1" output="$2" pattern="$3"
    if echo "$output" | grep -qF -- "$pattern"; then
        fail "$label (unexpected pattern: $pattern)"
        echo "        output: $(echo "$output" | head -3)"
    else
        pass "$label"
    fi
}

# --- resolve binary ---

BIN="${1:-}"

if [ -z "$BIN" ]; then
    OS="$(uname -s)"
    ARCH="$(uname -m)"
    case "${OS}-${ARCH}" in
        Linux-x86_64)   ASSET="rakers-linux-x86_64" ;;
        Darwin-arm64)   ASSET="rakers-macos-aarch64" ;;
        Darwin-x86_64)  ASSET="rakers-macos-x86_64" ;;
        *) echo "Unsupported platform: ${OS}-${ARCH}"; exit 1 ;;
    esac

    TMPBIN="$(mktemp)"
    trap 'rm -f "$TMPBIN"' EXIT

    echo "Downloading latest release ($ASSET)..."
    curl -L -s "https://github.com/tbro/rakers/releases/latest/download/${ASSET}" -o "$TMPBIN"
    chmod +x "$TMPBIN"
    BIN="$TMPBIN"
fi

echo "Binary: $BIN"
echo ""

# --- tests ---

echo "=== basic ==="

out="$("$BIN" --help 2>&1)"
check "--help exits cleanly" "$out" "Usage:"

out="$(printf '<script>document.write("<p>hello</p>")</script>' | "$BIN")"
check "JS render via stdin" "$out" "<p>hello</p>"

out="$(printf '<html><body><h1>Static</h1></body></html>' | "$BIN")"
check "static HTML passthrough" "$out" "<h1>Static</h1>"

echo ""
echo "=== flags ==="

out="$(printf '<script>console.log("test message")</script>' | "$BIN" --verbose 2>&1)"
check "--verbose shows console.log" "$out" "[console] test message"

out="$(printf '<script>console.log("hidden")</script>' | "$BIN" 2>&1)"
check_absent "no --verbose suppresses console.log" "$out" "[console]"

out="$(printf '<html><body><h1>Title</h1><p>Other</p></body></html>' | "$BIN" --selector h1)"
check "--selector matches element" "$out" "<h1>Title</h1>"
check_absent "--selector excludes non-matching" "$out" "<p>Other"

out="$(printf '<html><body><div><p>hello</p></div></body></html>' | "$BIN" --pretty)"
check "--pretty indents body" "$out" "  <body>"
check "--pretty indents div" "$out" "    <div>"

out="$(printf '<script>document.write("<p>hi</p>")</script>' | "$BIN" --json)"
check "--json has raw_bytes" "$out" '"raw_bytes"'
check "--json has rendered_bytes" "$out" '"rendered_bytes"'
check "--json has html field" "$out" '"html"'
check "--json embeds rendered content" "$out" "<p>hi</p>"

out="$(printf '<html><body><script>document.body.innerHTML="<h1>r</h1>"</script></body></html>' | "$BIN" --diff)"
check "--diff shows --- header" "$out" "---"
check "--diff shows +++ header" "$out" "+++"

echo ""
echo "=== error handling ==="

out="$(printf '<html></html>' | "$BIN" --timeout 0 2>&1 || true)"
check "--timeout 0 is rejected" "$out" "greater than zero"

out="$(printf '<html></html>' | "$BIN" --selector "##bad" 2>&1 || true)"
check "invalid selector fails" "$out" "invalid selector"

out="$(printf '<html></html>' | "$BIN" -H "no-colon-here" 2>&1 || true)"
check "invalid header fails" "$out" "invalid header"

echo ""
echo "=== live network ==="

out="$("$BIN" https://todomvc.com/examples/react/dist/ 2>/dev/null)"
check "TodoMVC React renders h1" "$out" "<h1>todos</h1>"
check "TodoMVC React renders new-todo input" "$out" 'class="new-todo"'

echo ""
echo "================================"
echo "  Passed: $PASS  Failed: $FAIL"
echo "================================"

[ "$FAIL" -eq 0 ]
