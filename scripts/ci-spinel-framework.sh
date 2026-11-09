#!/usr/bin/env bash
# Runs one spinel-framework shard: TESTS is its suites.
set -uo pipefail
# The binaries run outside cargo, so .cargo/config.toml's [env] stack size is not applied.
export RUST_MIN_STACK="${RUST_MIN_STACK:-33554432}"
test -n "${TESTS:-}" || exit 1
if [ -n "${RUNNER_TEMP:-}" ]; then
  tmp="$RUNNER_TEMP/spinel-framework"
else
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
fi
mkdir -p "$tmp/logs"

# Not left to the job's 30m: a suite stopped here is named, and the shard's other results and timings survive.
budget=${CI_SPINEL_FRAMEWORK_BUDGET:-1620}
deadline=$(( $(date +%s) + budget ))

# Not -O2: these suites judge behavior, and cc at -O2 was over a third of their CPU. A `spin build` finds the
# spinel beside it and keeps -O2, as do spinel-smoke, spinel-compare and the Campfire lanes. Empty keeps the default.
opt=${CI_SPINEL_OPT-0}
if [ -n "$opt" ] && real=$(command -v spinel); then
  mkdir -p "$tmp/shim"
  printf '#!/bin/sh\nexec "%s" -O%s "$@"\n' "$real" "$opt" > "$tmp/shim/spinel"
  chmod +x "$tmp/shim/spinel"
  export PATH="$tmp/shim:$PATH"
fi

args=()
for suite in $TESTS; do args+=(--test "$suite"); done
cargo test --no-run --message-format=json "${args[@]}" > "$tmp/build.json" || exit 1
exe() {
  python3 - "$tmp/build.json" "$1" <<'PY'
import json, sys
for line in open(sys.argv[1]):
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == sys.argv[2] and "test" in m["target"]["kind"]:
        print(m["executable"])
PY
}
# param_binds* also hold JRuby tests, run in compare-jruby.
filter() {
  case "$1" in param_binds|param_binds_values|param_binds_cleanup) echo _spinel ;; esac
}
# Not all at the end: a job that still hits its limit keeps what already finished. The lock keeps each suite's lines together.
report_suite() {
  local suite=$1 status=$2
  (
    if command -v flock > /dev/null; then flock 9; fi
    echo "::group::$suite"
    cat "$tmp/logs/$suite.log"
    echo "::endgroup::"
    grep -qE "^test result: FAILED|error: test failed|panicked at" "$tmp/logs/$suite.log" && echo "::error::$suite failed"
    [ "$status" -eq 124 ] && echo "::error::$suite stopped at this shard's ${budget}s budget"
  ) 9>> "$tmp/report.lock"
}
run_suite() {
  local suite=$1
  local bin left status=124
  bin=$(exe "$suite")
  left=$(( deadline - $(date +%s) ))
  : > "$tmp/logs/$suite.log"
  if [ "$left" -gt 0 ]; then
    python3 scripts/ci-plan.py timed --kind framework --name "$suite" -- \
      timeout -k 15 "$left" "$bin" --ignored --nocapture $(filter "$suite") > "$tmp/logs/$suite.log" 2>&1
    status=$?
  fi
  report_suite "$suite" "$status"
  return "$status"
}
export -f exe filter report_suite run_suite
export tmp budget deadline

start=$(date +%s)

# Not one suite at a time: each is mostly one Spinel compile; the PostgreSQL one shares a database, so it runs alone.
# Not in the plan's order: the longest suites start first, so none is left running alone at the end.
ordered=$(python3 - $TESTS <<'PY'
import json, os, sys
try:
    seconds = json.loads(os.environ.get("CI_SPINEL_TIMINGS") or "{}").get("framework", {})
except ValueError:
    seconds = {}
print(" ".join(sorted(sys.argv[1:], key=lambda s: -seconds.get(s, 0))))
PY
)
status=0
parallel=()
serial=()
for suite in $ordered; do
  case "$suite" in spinel_pg_db) serial+=("$suite") ;; *) parallel+=("$suite") ;; esac
done
printf '%s\n' "${parallel[@]}" | xargs -P "$(nproc)" -I{} bash -c 'run_suite {}' || status=1
for suite in "${serial[@]}"; do run_suite "$suite" || status=1; done
echo "finished in $(( $(date +%s) - start ))s"
exit "$status"
