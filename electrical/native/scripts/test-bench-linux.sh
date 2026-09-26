#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUNNER="${SCRIPT_DIR}/bench-linux.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
MOCK_BIN="$TMP/bin"
mkdir -p "$MOCK_BIN" "$TMP/criterion"
CALLS="$TMP/cargo-calls"
: >"$CALLS"
export TEST_CARGO_CALLS="$CALLS"
export CRITERION_HOME="$TMP/criterion"

cat >"$MOCK_BIN/cargo" <<'MOCK'
#!/usr/bin/env bash
set -Eeuo pipefail
printf '%q ' "$@" >>"$TEST_CARGO_CALLS"
printf '\n' >>"$TEST_CARGO_CALLS"
if [[ "${1:-}" == "--version" ]]; then
    echo 'cargo 1.0.0 (mock)'
    exit 0
fi
if [[ "${1:-}" == "metadata" ]]; then
    printf '{"target_directory":"%s"}\n' "${CRITERION_HOME%/criterion}"
    exit 0
fi
if [[ " $* " == *" --no-run "* ]]; then
    exit 0
fi
if [[ " $* " == *" -- --list "* ]]; then
    echo 'world/tick/matrix_dirty/medium/count_32: benchmark'
    exit 0
fi
mkdir -p "$CRITERION_HOME/mock/case/new"
for file in estimates.json sample.json tukey.json benchmark.json; do
    printf 'fresh-%s\n' "$file" >"$CRITERION_HOME/mock/case/new/$file"
done
MOCK

cat >"$MOCK_BIN/taskset" <<'MOCK'
#!/usr/bin/env bash
set -Eeuo pipefail
[[ "${1:-}" == '-c' ]] || exit 90
shift 2
exec "$@"
MOCK

cat >"$MOCK_BIN/git" <<'MOCK'
#!/usr/bin/env bash
case "$*" in
    *'rev-parse HEAD'*) echo deadbeef ;;
    *'branch --show-current'*) echo bench-test ;;
    *'diff --quiet'*) exit 0 ;;
    *) exit 0 ;;
esac
MOCK

cat >"$MOCK_BIN/rustc" <<'MOCK'
#!/usr/bin/env bash
echo 'rustc 1.0.0 (mock)'
MOCK
chmod +x "$MOCK_BIN"/*

CPU="$(awk '/^Cpus_allowed_list:/ { split($2, parts, /[-,]/); print parts[1]; exit }' /proc/self/status)"
[[ -n "$CPU" ]]

PATH="$MOCK_BIN:$PATH" \
CPU="$CPU" TUNE_CPU=0 SETTLE_SECONDS=0 SUITE=core \
    "$RUNNER" run \
        --compare before \
        --save after \
        --target topology \
        --suite full \
        --filter split \
        -- \
        --sample-size 10 \
        >"$TMP/output" 2>"$TMP/error"

grep -q -- '--bench topology' "$CALLS"
! grep -q -- '--bench world_tick' "$CALLS"
grep -q -- '--baseline before' "$CALLS"
grep -q -- '--sample-size 10' "$CALLS"
grep -q -- 'split' "$CALLS"
! grep -q -- '--save-baseline after' "$CALLS"
for file in estimates.json sample.json tukey.json benchmark.json; do
    cmp "$CRITERION_HOME/mock/case/new/$file" "$CRITERION_HOME/mock/case/after/$file"
done

echo 'combined compare+save: ok'

reset_calls() {
    : >"$CALLS"
    rm -rf "$CRITERION_HOME/mock"
}

reset_calls
PATH="$MOCK_BIN:$PATH" CPU="$CPU" TUNE_CPU=0 SETTLE_SECONDS=0 \
    "$RUNNER" save legacy-save --target topology >"$TMP/save-output" 2>"$TMP/save-error"
grep -q -- '--save-baseline legacy-save' "$CALLS"
! grep -q -- '--baseline legacy-save' "$CALLS"
echo 'legacy save alias: ok'

reset_calls
PATH="$MOCK_BIN:$PATH" CPU="$CPU" TUNE_CPU=0 SETTLE_SECONDS=0 \
    "$RUNNER" compare legacy-compare --target topology >"$TMP/compare-output" 2>"$TMP/compare-error"
grep -q -- '--baseline legacy-compare' "$CALLS"
! grep -q -- '--save-baseline legacy-compare' "$CALLS"
echo 'legacy compare alias: ok'

reset_calls
PATH="$MOCK_BIN:$PATH" CPU="$CPU" TUNE_CPU=0 SETTLE_SECONDS=0 FILTER='from-env' SUITE=core \
    "$RUNNER" run --suite full --filter from-cli --target protocol \
        >"$TMP/override-output" 2>"$TMP/override-error"
grep -q -- '--bench protocol' "$CALLS"
! grep -q -- '--bench world_tick' "$CALLS"
grep -q -- 'from-cli' "$CALLS"
! grep -q -- 'from-env' "$CALLS"
echo 'CLI overrides and target selection: ok'

reset_calls
if PATH="$MOCK_BIN:$PATH" CPU="$CPU" TUNE_CPU=0 SETTLE_SECONDS=0 \
    "$RUNNER" run --target topology -- --baseline forbidden \
        >"$TMP/managed-output" 2>"$TMP/managed-error"; then
    echo 'expected managed baseline passthrough to fail' >&2
    exit 1
fi
grep -q -- 'managed by --compare/--save' "$TMP/managed-error"
[[ ! -s "$CALLS" ]]
echo 'managed baseline passthrough rejection: ok'

reset_calls
PATH="$MOCK_BIN:$PATH" CPU="$CPU" TUNE_CPU=0 SETTLE_SECONDS=0 \
    "$RUNNER" check --target topology >"$TMP/check-output" 2>"$TMP/check-error"
grep -q -- '--bench topology' "$CALLS"
grep -q -- '--no-run' "$CALLS"
! grep -q -- '--all-targets' "$CALLS"
echo 'targeted check: ok'

reset_calls
PATH="$MOCK_BIN:$PATH" CPU="$CPU" TUNE_CPU=0 SETTLE_SECONDS=0 \
    "$RUNNER" profile \
        world_tick \
        world/tick/matrix_dirty/medium/count_32 \
        --profile-seconds 7 \
        >"$TMP/profile-output" 2>"$TMP/profile-error"
grep -q -- 'flamegraph' "$CALLS"
grep -q -- '--profile-time 7' "$CALLS"
echo 'profile seconds option: ok'
