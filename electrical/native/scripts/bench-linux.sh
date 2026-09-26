#!/usr/bin/env bash
set -Eeuo pipefail

# Linux benchmark runner for the Hynergy native workspace.
#
# Common commands:
#   ./bench-linux.sh check
#   ./bench-linux.sh list
#   ./bench-linux.sh run
#   ./bench-linux.sh run --compare before --save after
#   ./bench-linux.sh run --target topology --filter 'topology/'
#   ./bench-linux.sh save before          # compatibility alias
#   ./bench-linux.sh compare before       # compatibility alias
#   ./bench-linux.sh profile world_tick \
#       world/tick/matrix_dirty/medium/count_32
#
# Options override their matching environment variables. Criterion-specific
# tuning arguments can be forwarded after `--` in run mode.

COMMAND="${1:-run}"
if (($# > 0)); then
    shift
fi

CPU="${CPU:-}"
FILTER="${FILTER:-}"
SUITE="${SUITE:-core}"
TUNE_CPU="${TUNE_CPU:-1}"
SETTLE_SECONDS="${SETTLE_SECONDS:-3}"
PROFILE_SECONDS="${PROFILE_SECONDS:-20}"
COMPARE_BASELINE=""
SAVE_BASELINE=""
EXPLICIT_TARGETS=()
POSITIONAL_ARGS=()
FORWARDED_CRITERION_ARGS=()

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_DIR="${ROOT_DIR}/target/bench-logs"
FLAMEGRAPH_DIR="${ROOT_DIR}/target/flamegraphs"
cd "$ROOT_DIR"

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
    cat <<EOF_USAGE
usage:
  $0 check [options]
  $0 list [options]
  $0 run [options] [-- <Criterion args...>]
  $0 save <baseline> [options] [-- <Criterion args...>]
  $0 compare <baseline> [options] [-- <Criterion args...>]
  $0 profile <benchmark-target> <exact Criterion case> [options]

run options:
  --compare <baseline>       compare this run to a saved Criterion baseline
  --save <baseline>          save this run as a named baseline
  --target <target>          run only this benchmark target; repeatable
  --suite <core|full>        benchmark suite (overrides SUITE)
  --filter <regex>           Criterion benchmark filter (overrides FILTER)
  --cpu <id>                 logical CPU to pin to (overrides CPU)
  --no-tune                  do not change CPU governor/EPP
  --tune                     enable CPU tuning
  --settle <seconds>         delay after tuning before measurement
  --profile-seconds <secs>   profile duration (profile command only)
  -h, --help                 show this help

examples:
  $0 run --compare main --save optimized
  $0 run --target topology --filter 'topology/' -- --sample-size 200
  $0 run --suite full --cpu 13 --no-tune
EOF_USAGE
}

fail() {
    echo "error: $*" >&2
    exit 2
}

require_value() {
    local option="$1"
    local value="${2:-}"
    [[ -n "$value" ]] || fail "$option requires a value"
}

validate_baseline_name() {
    local name="$1"
    local option="$2"

    [[ "$name" =~ ^[[:alnum:]_.-]+$ ]] \
        || fail "$option baseline '$name' must be a single name using letters, digits, '.', '_' or '-'"
    case "$name" in
        .|..|new|change|report)
            fail "$option baseline '$name' is reserved by Criterion"
            ;;
    esac
}

# Preserve the old `save NAME` and `compare NAME` entry points while making
# save/compare independent run options.
case "$COMMAND" in
    save)
        require_value "save" "${1:-}"
        SAVE_BASELINE="$1"
        shift
        COMMAND="run"
        ;;
    compare)
        require_value "compare" "${1:-}"
        COMPARE_BASELINE="$1"
        shift
        COMMAND="run"
        ;;
    check|list|run|profile)
        ;;
    -h|--help|help)
        usage
        exit 0
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac

while (($# > 0)); do
    case "$1" in
        --compare)
            require_value "--compare" "${2:-}"
            COMPARE_BASELINE="$2"
            shift 2
            ;;
        --compare=*)
            COMPARE_BASELINE="${1#*=}"
            require_value "--compare" "$COMPARE_BASELINE"
            shift
            ;;
        --save)
            require_value "--save" "${2:-}"
            SAVE_BASELINE="$2"
            shift 2
            ;;
        --save=*)
            SAVE_BASELINE="${1#*=}"
            require_value "--save" "$SAVE_BASELINE"
            shift
            ;;
        --target)
            require_value "--target" "${2:-}"
            EXPLICIT_TARGETS+=("$2")
            shift 2
            ;;
        --target=*)
            target_value="${1#*=}"
            require_value "--target" "$target_value"
            EXPLICIT_TARGETS+=("$target_value")
            shift
            ;;
        --suite)
            require_value "--suite" "${2:-}"
            SUITE="$2"
            shift 2
            ;;
        --suite=*)
            SUITE="${1#*=}"
            require_value "--suite" "$SUITE"
            shift
            ;;
        --filter)
            require_value "--filter" "${2:-}"
            FILTER="$2"
            shift 2
            ;;
        --filter=*)
            FILTER="${1#*=}"
            shift
            ;;
        --cpu)
            require_value "--cpu" "${2:-}"
            CPU="$2"
            shift 2
            ;;
        --cpu=*)
            CPU="${1#*=}"
            require_value "--cpu" "$CPU"
            shift
            ;;
        --no-tune)
            TUNE_CPU=0
            shift
            ;;
        --tune)
            TUNE_CPU=1
            shift
            ;;
        --settle)
            require_value "--settle" "${2:-}"
            SETTLE_SECONDS="$2"
            shift 2
            ;;
        --settle=*)
            SETTLE_SECONDS="${1#*=}"
            require_value "--settle" "$SETTLE_SECONDS"
            shift
            ;;
        --profile-seconds)
            require_value "--profile-seconds" "${2:-}"
            PROFILE_SECONDS="$2"
            shift 2
            ;;
        --profile-seconds=*)
            PROFILE_SECONDS="${1#*=}"
            require_value "--profile-seconds" "$PROFILE_SECONDS"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --)
            shift
            FORWARDED_CRITERION_ARGS+=("$@")
            break
            ;;
        -* )
            fail "unknown option '$1'"
            ;;
        *)
            POSITIONAL_ARGS+=("$1")
            shift
            ;;
    esac
done

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

[[ -z "$COMPARE_BASELINE" ]] \
    || validate_baseline_name "$COMPARE_BASELINE" "compare"
[[ -z "$SAVE_BASELINE" ]] \
    || validate_baseline_name "$SAVE_BASELINE" "save"

if [[ "$COMMAND" == "profile" ]]; then
    ((${#POSITIONAL_ARGS[@]} == 2)) \
        || fail "profile requires a benchmark target and one exact Criterion case"
    ((${#FORWARDED_CRITERION_ARGS[@]} == 0)) \
        || fail "Criterion passthrough arguments are only supported by run"
    [[ -z "$COMPARE_BASELINE" && -z "$SAVE_BASELINE" ]] \
        || fail "--compare and --save are only supported by run"
else
    ((${#POSITIONAL_ARGS[@]} == 0)) \
        || fail "unexpected positional argument '${POSITIONAL_ARGS[0]}'"
fi

if [[ "$COMMAND" != "run" ]]; then
    [[ -z "$COMPARE_BASELINE" && -z "$SAVE_BASELINE" ]] \
        || fail "--compare and --save are only supported by run"
    ((${#FORWARDED_CRITERION_ARGS[@]} == 0)) \
        || fail "Criterion passthrough arguments are only supported by run"
fi

for arg in "${FORWARDED_CRITERION_ARGS[@]}"; do
    case "$arg" in
        --baseline|--baseline=*|--baseline-lenient|--baseline-lenient=*|\
        --save-baseline|--save-baseline=*|--discard-baseline|\
        --load-baseline|--load-baseline=*|-b|-b*|-s|-s*)
            fail "baseline arguments after '--' are managed by --compare/--save"
            ;;
    esac
done

command -v cargo >/dev/null 2>&1 || fail "cargo not found"

suite_targets() {
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

for target in "${EXPLICIT_TARGETS[@]}"; do
    target_exists "$target" || fail "unknown benchmark target '$target'"
    if [[ "$target" == "cpu_full" && "$SUITE" != "full" ]]; then
        fail "benchmark target 'cpu_full' requires --suite full"
    fi
done

benchmark_targets() {
    if ((${#EXPLICIT_TARGETS[@]} > 0)); then
        printf '%s\n' "${EXPLICIT_TARGETS[@]}"
    else
        suite_targets
    fi
}

prebuild_benchmark_targets() {
    local args=(bench -p hynergy-benchmarks --no-run)
    local target

    while read -r target; do
        args+=(--bench "$target")
    done < <(benchmark_targets)

    cargo "${args[@]}"
}

list_target() {
    local target="$1"

    echo
    echo "${target}"
    HYNERGY_BENCH_SUITE="$SUITE" \
        cargo bench -p hynergy-benchmarks --bench "$target" -- --list
}

if [[ "$COMMAND" == "check" ]]; then
    if ((${#EXPLICIT_TARGETS[@]} > 0)); then
        prebuild_benchmark_targets
    else
        cargo bench -p hynergy-benchmarks --all-targets --no-run
    fi
    exit 0
fi

if [[ "$COMMAND" == "list" ]]; then
    while read -r target; do
        list_target "$target"
    done < <(benchmark_targets)
    exit 0
fi

PROFILE_TARGET=""
PROFILE_CASE=""
if [[ "$COMMAND" == "profile" ]]; then
    PROFILE_TARGET="${POSITIONAL_ARGS[0]}"
    PROFILE_CASE="${POSITIONAL_ARGS[1]}"
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
PROMOTION_MARKER=""

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
    [[ -z "$PROMOTION_MARKER" ]] || rm -f "$PROMOTION_MARKER"
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

format_array() {
    if (($# == 0)); then
        printf '<none>'
        return
    fi
    printf '%q ' "$@"
}

mkdir -p "$LOG_DIR"
TIMESTAMP="$(date '+%Y%m%d-%H%M%S')"
LOG_FILE="${LOG_DIR}/${TIMESTAMP}-${COMMAND}-${SUITE}.log"
TARGET_SUMMARY="$(benchmark_targets | paste -sd, -)"

echo "Hynergy benchmark runner"
echo "  CPU:       ${CPU} (core ${CORE_ID}, package ${PACKAGE_ID})"
echo "  siblings:  ${THREAD_SIBLINGS}"
echo "  command:   ${COMMAND}"
echo "  suite:     ${SUITE}"
echo "  targets:   ${TARGET_SUMMARY}"
echo "  filter:    ${FILTER:-<none>}"
echo "  compare:   ${COMPARE_BASELINE:-<none>}"
echo "  save:      ${SAVE_BASELINE:-<default>}"
echo "  CPU tune:  ${TUNE_CPU}"
echo "  log:       ${LOG_FILE}"

echo "Pre-building benchmark targets..."
if [[ "$COMMAND" == "profile" ]]; then
    cargo bench -p hynergy-benchmarks --bench "$PROFILE_TARGET" --no-run
else
    prebuild_benchmark_targets
fi
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
    echo "git_commit: $(git -C "$ROOT_DIR" rev-parse HEAD)"
    echo "git_branch: $(git -C "$ROOT_DIR" branch --show-current)"
    if git -C "$ROOT_DIR" diff --quiet && git -C "$ROOT_DIR" diff --cached --quiet; then
        echo "git_dirty: false"
    else
        echo "git_dirty: true"
    fi
    echo "kernel: $(uname -srmo)"
    echo "cpu: ${CPU}"
    echo "core: ${CORE_ID}"
    echo "package: ${PACKAGE_ID}"
    echo "siblings: ${THREAD_SIBLINGS}"
    echo "command: ${COMMAND}"
    echo "suite: ${SUITE}"
    echo "targets: ${TARGET_SUMMARY}"
    echo "filter: ${FILTER:-<none>}"
    echo "compare_baseline: ${COMPARE_BASELINE:-<none>}"
    echo "save_baseline: ${SAVE_BASELINE:-<default>}"
    printf 'criterion_args: '
    format_array "${FORWARDED_CRITERION_ARGS[@]}"
    printf '\n'
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

if [[ "$COMMAND" == "profile" ]]; then
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

criterion_output_dir() {
    if [[ -n "${CRITERION_HOME:-}" ]]; then
        printf '%s\n' "$CRITERION_HOME"
        return
    fi

    local metadata
    local target_dir
    metadata="$(cargo metadata --format-version 1 --no-deps)"
    target_dir="$(sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' <<<"$metadata")"
    [[ -n "$target_dir" ]] || fail "could not determine Cargo target directory for baseline promotion"
    printf '%s/criterion\n' "$target_dir"
}

promote_new_baseline() {
    local baseline="$1"
    local criterion_dir
    local benchmark_file
    local new_dir
    local base_dir
    local file
    local promoted=0

    [[ -n "$PROMOTION_MARKER" ]] || fail "internal error: missing baseline promotion marker"
    criterion_dir="$(criterion_output_dir)"
    [[ -d "$criterion_dir" ]] \
        || fail "Criterion output directory '$criterion_dir' does not exist"

    while IFS= read -r -d '' benchmark_file; do
        new_dir="${benchmark_file%/benchmark.json}"
        base_dir="${new_dir%/new}/${baseline}"
        mkdir -p "$base_dir"

        for file in estimates.json sample.json tukey.json benchmark.json; do
            [[ -f "$new_dir/$file" ]] \
                || fail "Criterion result '$new_dir/$file' is missing"
            cp -f -- "$new_dir/$file" "$base_dir/$file"
        done
        if [[ -f "$new_dir/raw.csv" ]]; then
            cp -f -- "$new_dir/raw.csv" "$base_dir/raw.csv"
        fi
        ((promoted += 1))
    done < <(
        find "$criterion_dir" \
            -type f \
            -path '*/new/benchmark.json' \
            -newer "$PROMOTION_MARKER" \
            -print0
    )

    ((promoted > 0)) \
        || fail "no fresh Criterion results were found to save as baseline '$baseline'"
    echo "Saved ${promoted} fresh Criterion result(s) as baseline '${baseline}'"
}

CRITERION_ARGS=()
[[ -n "$FILTER" ]] && CRITERION_ARGS+=("$FILTER")
if [[ -n "$COMPARE_BASELINE" ]]; then
    CRITERION_ARGS+=(--baseline "$COMPARE_BASELINE")
elif [[ -n "$SAVE_BASELINE" ]]; then
    CRITERION_ARGS+=(--save-baseline "$SAVE_BASELINE")
fi
CRITERION_ARGS+=("${FORWARDED_CRITERION_ARGS[@]}")

if [[ -n "$COMPARE_BASELINE" && -n "$SAVE_BASELINE" ]]; then
    PROMOTION_MARKER="$(mktemp "${TMPDIR:-/tmp}/hynergy-bench-baseline.XXXXXX")"
fi

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

if [[ -n "$COMPARE_BASELINE" && -n "$SAVE_BASELINE" ]]; then
    promote_new_baseline "$SAVE_BASELINE"
fi

echo
echo "Benchmark run completed"
echo "Results log: ${LOG_FILE}"
