#!/usr/bin/env bash
set -Eeuo pipefail

# Linux benchmark runner for the Hynergy native workspace.
#
# Common commands:
#   ./bench-linux.sh check
#   ./bench-linux.sh list
#   ./bench-linux.sh run
#   ./bench-linux.sh save before
#   ./bench-linux.sh compare before
#   ./bench-linux.sh profile world_tick \
#       world/tick/matrix_dirty/medium/count_32
#
# Optional environment:
#   CPU=13 SUITE=full ./bench-linux.sh run
#   FILTER='world/tick/warm_sleep/simple/count_256' ./bench-linux.sh run
#   TUNE_CPU=0 SETTLE_SECONDS=0 ./bench-linux.sh run
#   PROFILE_SECONDS=30 ./bench-linux.sh profile <target> <exact-case>

MODE="${1:-run}"
MODE_ARGUMENT="${2:-}"
PROFILE_CASE="${3:-}"

CPU="${CPU:-}"
FILTER="${FILTER:-}"
SUITE="${SUITE:-core}"
TUNE_CPU="${TUNE_CPU:-1}"
SETTLE_SECONDS="${SETTLE_SECONDS:-3}"
PROFILE_SECONDS="${PROFILE_SECONDS:-20}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_DIR="${ROOT_DIR}/target/bench-logs"
FLAMEGRAPH_DIR="${ROOT_DIR}/target/flamegraphs"

CORE_TARGETS=(
    world_tick
    topology
    subscriptions
    cpu_synthetic
    cpu_validated
    protocol
)
FULL_TARGETS=(
    world_tick
    topology
    subscriptions
    cpu_synthetic
    cpu_validated
    cpu_full
    protocol
)
ALL_TARGETS=(
    world_tick
    topology
    subscriptions
    cpu_synthetic
    cpu_validated
    cpu_full
    protocol
)

usage() {
    cat <<EOF
usage:
  $0 check
  $0 list
  $0 run
  $0 save <baseline>
  $0 compare <baseline>
  $0 profile <benchmark-target> <exact Criterion case>
EOF
}

fail() {
    echo "error: $*" >&2
    exit 2
}

case "$MODE" in
    check|list|run|save|compare|profile)
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac

case "$SUITE" in
    core|full)
        ;;
    *)
        fail "invalid SUITE '$SUITE' (expected core or full)"
        ;;
esac

[[ "$TUNE_CPU" == "0" || "$TUNE_CPU" == "1" ]] \
    || fail "invalid TUNE_CPU '$TUNE_CPU' (expected 0 or 1)"
[[ "$SETTLE_SECONDS" =~ ^[0-9]+$ ]] \
    || fail "invalid SETTLE_SECONDS '$SETTLE_SECONDS'"
[[ "$PROFILE_SECONDS" =~ ^[1-9][0-9]*$ ]] \
    || fail "invalid PROFILE_SECONDS '$PROFILE_SECONDS'"

command -v cargo >/dev/null 2>&1 || fail "cargo not found"

benchmark_targets() {
    if [[ "$SUITE" == "full" ]]; then
        printf '%s\n' "${FULL_TARGETS[@]}"
    else
        printf '%s\n' "${CORE_TARGETS[@]}"
    fi
}

target_exists() {
    local wanted="$1"
    local target

    for target in "${ALL_TARGETS[@]}"; do
        [[ "$target" == "$wanted" ]] && return 0
    done

    return 1
}

list_target() {
    local target="$1"

    echo
    echo "${target}"
    HYNERGY_BENCH_SUITE="$SUITE" \
        cargo bench -p hynergy-benchmarks --bench "$target" -- --list
}

if [[ "$MODE" == "check" ]]; then
    cargo bench -p hynergy-benchmarks --all-targets --no-run
    exit 0
fi

if [[ "$MODE" == "list" ]]; then
    while read -r target; do
        list_target "$target"
    done < <(benchmark_targets)
    exit 0
fi

PROFILE_TARGET=""
if [[ "$MODE" == "profile" ]]; then
    PROFILE_TARGET="$MODE_ARGUMENT"
    [[ -n "$PROFILE_TARGET" && -n "$PROFILE_CASE" ]] \
        || fail "profile requires a benchmark target and one exact Criterion case"
    target_exists "$PROFILE_TARGET" \
        || fail "unknown benchmark target '$PROFILE_TARGET'"

    BENCH_LIST="$(
        HYNERGY_BENCH_SUITE="$SUITE" \
            cargo bench -p hynergy-benchmarks \
                --bench "$PROFILE_TARGET" -- --list
    )"
    MATCH_COUNT="$(
        awk -v expected="${PROFILE_CASE}: benchmark" \
            '$0 == expected { count += 1 } END { print count + 0 }' \
            <<<"$BENCH_LIST"
    )"
    [[ "$MATCH_COUNT" == "1" ]] \
        || fail "exact Criterion case '$PROFILE_CASE' matched ${MATCH_COUNT} cases in ${PROFILE_TARGET}"
fi

command -v taskset >/dev/null 2>&1 \
    || fail "taskset not found (usually provided by util-linux)"

expand_cpu_list() {
    local input="$1"
    local part
    local start
    local end
    local cpu

    IFS=',' read -ra parts <<<"$input"
    for part in "${parts[@]}"; do
        if [[ "$part" == *-* ]]; then
            start="${part%-*}"
            end="${part#*-}"
            for ((cpu = start; cpu <= end; cpu++)); do
                echo "$cpu"
            done
        elif [[ -n "$part" ]]; then
            echo "$part"
        fi
    done
}

available_cpus() {
    local allowed
    allowed="$(awk '/^Cpus_allowed_list:/ { print $2 }' /proc/self/status 2>/dev/null || true)"
    if [[ -n "$allowed" ]]; then
        expand_cpu_list "$allowed"
        return
    fi

    if [[ -r /sys/devices/system/cpu/online ]]; then
        expand_cpu_list "$(< /sys/devices/system/cpu/online)"
        return
    fi

    local path
    for path in /sys/devices/system/cpu/cpu[0-9]*; do
        [[ -d "$path" ]] || continue
        basename "$path" | sed 's/^cpu//'
    done
}

cpu_is_available() {
    local wanted="$1"
    local cpu

    while read -r cpu; do
        [[ "$cpu" == "$wanted" ]] && return 0
    done < <(available_cpus)

    return 1
}

choose_cpu() {
    local first=""
    local cpu

    while read -r cpu; do
        [[ -n "$first" ]] || first="$cpu"
        if ((cpu != 0)); then
            echo "$cpu"
            return
        fi
    done < <(available_cpus)

    [[ -n "$first" ]] || fail "no available CPUs found"
    echo "$first"
}

if [[ -z "$CPU" ]]; then
    CPU="$(choose_cpu)"
fi
[[ "$CPU" =~ ^[0-9]+$ ]] || fail "invalid CPU '$CPU'"
cpu_is_available "$CPU" || fail "CPU $CPU is not available to this process"

CPU_SYS="/sys/devices/system/cpu/cpu${CPU}"
CPUFREQ_SYS="${CPU_SYS}/cpufreq"
THREAD_SIBLINGS="$CPU"
CORE_ID="unknown"
PACKAGE_ID="unknown"
ORIGINAL_GOVERNOR=""
ORIGINAL_EPP=""

[[ -r "${CPU_SYS}/topology/thread_siblings_list" ]] \
    && THREAD_SIBLINGS="$(<"${CPU_SYS}/topology/thread_siblings_list")"
[[ -r "${CPU_SYS}/topology/core_id" ]] \
    && CORE_ID="$(<"${CPU_SYS}/topology/core_id")"
[[ -r "${CPU_SYS}/topology/physical_package_id" ]] \
    && PACKAGE_ID="$(<"${CPU_SYS}/topology/physical_package_id")"
[[ -r "${CPUFREQ_SYS}/scaling_governor" ]] \
    && ORIGINAL_GOVERNOR="$(<"${CPUFREQ_SYS}/scaling_governor")"
[[ -r "${CPUFREQ_SYS}/energy_performance_preference" ]] \
    && ORIGINAL_EPP="$(<"${CPUFREQ_SYS}/energy_performance_preference")"

restore_cpu() {
    local status=$?

    trap - EXIT INT TERM
    set +e
    if [[ "$TUNE_CPU" == "1" ]]; then
        if [[ -n "$ORIGINAL_GOVERNOR" && -e "${CPUFREQ_SYS}/scaling_governor" ]]; then
            printf '%s\n' "$ORIGINAL_GOVERNOR" \
                | sudo tee "${CPUFREQ_SYS}/scaling_governor" >/dev/null
        fi
        if [[ -n "$ORIGINAL_EPP" && -e "${CPUFREQ_SYS}/energy_performance_preference" ]]; then
            printf '%s\n' "$ORIGINAL_EPP" \
                | sudo tee "${CPUFREQ_SYS}/energy_performance_preference" >/dev/null
        fi
    fi

    exit "$status"
}
trap restore_cpu EXIT INT TERM

tune_cpu() {
    [[ "$TUNE_CPU" == "1" ]] || return 0

    local available_governors=""
    local available_epp=""

    [[ -r "${CPUFREQ_SYS}/scaling_available_governors" ]] \
        && available_governors="$(<"${CPUFREQ_SYS}/scaling_available_governors")"
    if grep -qw performance <<<"$available_governors"; then
        printf '%s\n' performance \
            | sudo tee "${CPUFREQ_SYS}/scaling_governor" >/dev/null
    fi

    [[ -r "${CPUFREQ_SYS}/energy_performance_available_preferences" ]] \
        && available_epp="$(<"${CPUFREQ_SYS}/energy_performance_available_preferences")"
    if grep -qw performance <<<"$available_epp"; then
        printf '%s\n' performance \
            | sudo tee "${CPUFREQ_SYS}/energy_performance_preference" >/dev/null
    fi
}

mkdir -p "$LOG_DIR"
TIMESTAMP="$(date '+%Y%m%d-%H%M%S')"
LOG_FILE="${LOG_DIR}/${TIMESTAMP}-${MODE}-${SUITE}.log"
BASELINE="${MODE_ARGUMENT:-before}"

echo "Hynergy benchmark runner"
echo "  CPU:       ${CPU} (core ${CORE_ID}, package ${PACKAGE_ID})"
echo "  siblings:  ${THREAD_SIBLINGS}"
echo "  mode:      ${MODE}"
echo "  suite:     ${SUITE}"
echo "  filter:    ${FILTER:-<none>}"
echo "  CPU tune:  ${TUNE_CPU}"
echo "  log:       ${LOG_FILE}"

echo "Pre-building benchmark targets..."
cargo bench -p hynergy-benchmarks --all-targets --no-run
tune_cpu

CURRENT_GOVERNOR="unknown"
CURRENT_EPP="unavailable"
[[ -r "${CPUFREQ_SYS}/scaling_governor" ]] \
    && CURRENT_GOVERNOR="$(<"${CPUFREQ_SYS}/scaling_governor")"
[[ -r "${CPUFREQ_SYS}/energy_performance_preference" ]] \
    && CURRENT_EPP="$(<"${CPUFREQ_SYS}/energy_performance_preference")"

{
    echo "Hynergy benchmark metadata"
    echo "date: $(date --iso-8601=seconds)"
    echo "hostname: $(hostname)"
    echo "kernel: $(uname -srmo)"
    echo "cpu: ${CPU}"
    echo "core: ${CORE_ID}"
    echo "package: ${PACKAGE_ID}"
    echo "siblings: ${THREAD_SIBLINGS}"
    echo "mode: ${MODE}"
    echo "suite: ${SUITE}"
    echo "filter: ${FILTER:-<none>}"
    echo "governor: ${CURRENT_GOVERNOR}"
    echo "EPP: ${CURRENT_EPP}"
    rustc --version --verbose
    cargo --version
    if command -v lscpu >/dev/null 2>&1; then
        lscpu
    fi
} >"$LOG_FILE"

echo "Allowing CPU to settle for ${SETTLE_SECONDS}s..."
sleep "$SETTLE_SECONDS"

if [[ "$MODE" == "profile" ]]; then
    cargo flamegraph --version >/dev/null 2>&1 \
        || fail "cargo-flamegraph is not installed"

    mkdir -p "$FLAMEGRAPH_DIR"
    PROFILE_SLUG="$(
        sed 's|/|--|g; s/[ _]/-/g' <<<"${PROFILE_CASE}" \
            | tr -cd '[:alnum:]._-'
    )"
    RUN_NAME="${TIMESTAMP}--${PROFILE_SLUG}"
    PROFILE_OUTPUT="${FLAMEGRAPH_DIR}/${RUN_NAME}.svg"
    PERF_OUTPUT="${FLAMEGRAPH_DIR}/${RUN_NAME}.perf.data"

    echo "Profiling ${PROFILE_TARGET}: ${PROFILE_CASE}"
    # Use a relative recording path because flamegraph splits --cmd on whitespace.
    (
        cd "$FLAMEGRAPH_DIR"
        HYNERGY_BENCH_SUITE="$SUITE" \
            taskset -c "$CPU" \
            cargo flamegraph \
            --manifest-path "${ROOT_DIR}/Cargo.toml" \
            -p hynergy-benchmarks \
            --bench "$PROFILE_TARGET" \
            --output "$PROFILE_OUTPUT" \
            --cmd "record -F 997 --call-graph dwarf,64000 -g -o ${RUN_NAME}.perf.data" \
            --title "Hynergy ${PROFILE_CASE}" \
            --palette rust \
            --deterministic \
            -- \
            "$PROFILE_CASE" \
            --exact \
            --profile-time "$PROFILE_SECONDS" \
            --bench
    )

    echo "Flamegraph: ${PROFILE_OUTPUT}"
    echo "Perf recording: ${PERF_OUTPUT}"
    exit 0
fi

CRITERION_ARGS=()
[[ -n "$FILTER" ]] && CRITERION_ARGS+=("$FILTER")
case "$MODE" in
    run)
        ;;
    save)
        CRITERION_ARGS+=(--save-baseline "$BASELINE")
        ;;
    compare)
        CRITERION_ARGS+=(--baseline "$BASELINE")
        ;;
esac

run_target() {
    local target="$1"

    echo
    echo "Benchmark target: ${target}"
    {
        echo
        echo "Benchmark target: ${target}"
    } >>"$LOG_FILE"

    HYNERGY_BENCH_SUITE="$SUITE" \
        taskset -c "$CPU" \
        cargo bench \
            -p hynergy-benchmarks \
            --bench "$target" \
            -- \
            "${CRITERION_ARGS[@]}" \
            2>&1 | tee -a "$LOG_FILE"
}

while read -r target; do
    run_target "$target"
done < <(benchmark_targets)

echo
echo "Benchmark run completed"
echo "Results log: ${LOG_FILE}"
