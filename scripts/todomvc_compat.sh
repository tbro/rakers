#!/usr/bin/env bash
# Smoke-test rakers against all TodoMVC examples.
#
# Usage:
#   ./scripts/todomvc_compat.sh              # builds debug binary first
#   ./scripts/todomvc_compat.sh ./rakers     # uses a pre-built binary
#
# Exit code is always 0 — failures from individual sites are informational.

BINARY=${1:-}

if [ -z "$BINARY" ]; then
  echo "Building rakers..."
  cargo build -q
  BINARY="./target/debug/rakers"
fi

BASE="https://todomvc.com/"
EXAMPLES=(
  "examples/react/dist/"
  "examples/react-redux/dist/"
  "examples/vue/dist/"
  "examples/preact/dist/"
  "examples/svelte/dist/"
  "examples/angular/dist/browser/"
  "examples/lit/dist/"
  "examples/emberjs/todomvc/dist/"
  "examples/backbone/dist/"
  "examples/javascript-es5/dist/"
  "examples/javascript-es6/dist/"
  "examples/jquery/dist/"
  "examples/web-components/dist/"
  "examples/knockoutjs/"
  "examples/mithril/"
  "examples/backbone_marionette/"
  "examples/elm/"
  "examples/riotjs/"
  "examples/aurelia/"
  "examples/dojo/"
)

printf "%-40s %6s %6s %4s %6s  %s\n" "EXAMPLE" "RAW" "OUT" "ERRS" "SKIPS" "h1-todos"
printf "%-40s %6s %6s %4s %6s  %s\n" "-------" "---" "---" "----" "-----" "--------"

for path in "${EXAMPLES[@]}"; do
  url="${BASE}${path}"
  raw=$(curl -s --max-time 10 "$url" || true)
  raw_size=${#raw}

  rendered=$(timeout 20 "$BINARY" "$url" 2>/tmp/rakers_todomvc_stderr.txt)
  rc=$?

  if [ $rc -eq 124 ]; then
    printf "%-40s %6d %6s %4s %6s  %s\n" "$path" "$raw_size" "TIMEOUT" "-" "-" "-"
    continue
  fi

  out_size=${#rendered}

  err_count=0
  skip_count=0
  if [ -f /tmp/rakers_todomvc_stderr.txt ]; then
    err_count=$(grep -c '^\[js error\]' /tmp/rakers_todomvc_stderr.txt 2>/dev/null || true)
    skip_count=$(grep -c '^\[skip\]'     /tmp/rakers_todomvc_stderr.txt 2>/dev/null || true)
    err_count=${err_count:-0}
    skip_count=${skip_count:-0}
  fi

  raw_has=$(echo "$raw"      | grep -ciE '<h1[^>]*>todos</h1>' 2>/dev/null || true); raw_has=${raw_has:-0}
  out_has=$(echo "$rendered" | grep -ciE '<h1[^>]*>todos</h1>' 2>/dev/null || true); out_has=${out_has:-0}

  if   [ "$raw_has" -eq 0 ] && [ "$out_has" -gt 0 ]; then verdict="YES (JS added)"
  elif [ "$out_has" -gt 0 ];                          then verdict="yes (prerendered)"
  else                                                     verdict="no"
  fi

  printf "%-40s %6d %6d %4d %6d  %s\n" \
    "$path" "$raw_size" "$out_size" "$err_count" "$skip_count" "$verdict"
done

exit 0
