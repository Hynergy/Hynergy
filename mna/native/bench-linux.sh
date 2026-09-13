#!/usr/bin/env bash
set -Eeuo pipefail

# Hynergy Linux benchmark runner
#
# Usage:
#   ./bench-linux.sh run
#   ./bench-linux.sh save before
#   ./bench-linux.sh compare before
#
# Optional:
#   CPU=13 ./bench-linux.sh run
#   FILTER='topology/detach_terminal' ./bench-linux.sh run
#   TUNE_CPU=0 ./bench-linux.sh run
#   SETTLE_SECONDS=5 ./bench-linux.sh run
#
# CPU:
#   If omitted, the script automatically selects an online logical CPU.
#
# TUNE_CPU:
#   1 = try to use performance governor / EPP if available
#   0 = don't modify CPU power settings

MODE="${1:-run}"
BASELINE="${2:-before}"

CPU="${CPU:-}"
FILTER="${FILTER:-}"

TUNE_CPU="${TUNE_CPU:-1}"
SETTLE_SECONDS="${SETTLE_SECONDS:-3}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

LOG_DIR="${ROOT_DIR}/target/bench-logs"
TIMESTAMP="$(date '+%Y%m%d-%H%M%S')"
LOG_FILE="${LOG_DIR}/${TIMESTAMP}-${MODE}.log"

mkdir -p "$LOG_DIR"

case "$MODE" in
    run|save|compare)
        ;;
    *)
        echo "usage: $0 {run|save|compare} [baseline]"
        exit 1
        ;;
esac

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found"
    exit 1
fi

if ! command -v taskset >/dev/null 2>&1; then
    echo "error: taskset not found (usually provided by util-linux)"
    exit 1
fi

# ---------------------------------------------------------------------------
# CPU-list helpers
# Handles Linux CPU-list syntax:
#
#   0
#   0-7
#   0-3,8-11
# ---------------------------------------------------------------------------

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

online_cpus() {
    if [[ -r /sys/devices/system/cpu/online ]]; then
        expand_cpu_list "$(< /sys/devices/system/cpu/online)"
        return
    fi

    # Fallback for unusual/older systems.
    local path

    for path in /sys/devices/system/cpu/cpu[0-9]*; do
        [[ -d "$path" ]] || continue
        basename "$path" | sed 's/^cpu//'
    done
}

cpu_is_online() {
    local wanted="$1"
    local cpu

    while read -r cpu; do
        if [[ "$cpu" == "$wanted" ]]; then
            return 0
        fi
    done < <(online_cpus)

    return 1
}

# ---------------------------------------------------------------------------
# CPU selection
# ---------------------------------------------------------------------------

choose_cpu() {
    # Prefer an explicitly isolated CPU when the kernel configuration
    # provides one.
    if [[ -r /sys/devices/system/cpu/isolated ]]; then
        local isolated
        isolated="$(< /sys/devices/system/cpu/isolated)"

        if [[ -n "$isolated" ]]; then
            expand_cpu_list "$isolated" | head -n 1
            return
        fi
    fi

    # Otherwise avoid CPU 0 when possible because it often handles
    # more kernel housekeeping/interrupt work.
    local first=""
    local cpu

    while read -r cpu; do
        [[ -n "$first" ]] || first="$cpu"

        if (( cpu != 0 )); then
            echo "$cpu"
            return
        fi
    done < <(online_cpus)

    if [[ -n "$first" ]]; then
        echo "$first"
        return
    fi

    echo "error: no online CPUs found" >&2
    return 1
}

if [[ -z "$CPU" ]]; then
    CPU="$(choose_cpu)"
fi

if ! [[ "$CPU" =~ ^[0-9]+$ ]]; then
    echo "error: invalid CPU: $CPU"
    exit 1
fi

if ! cpu_is_online "$CPU"; then
    echo "error: CPU $CPU is not online"
    exit 1
fi

CPU_SYS="/sys/devices/system/cpu/cpu${CPU}"
CPUFREQ_SYS="${CPU_SYS}/cpufreq"

# ---------------------------------------------------------------------------
# Topology discovery
# ---------------------------------------------------------------------------

THREAD_SIBLINGS="$CPU"

if [[ -r "${CPU_SYS}/topology/thread_siblings_list" ]]; then
    THREAD_SIBLINGS="$(<"${CPU_SYS}/topology/thread_siblings_list")"
fi

CORE_ID="unknown"
PACKAGE_ID="unknown"

[[ -r "${CPU_SYS}/topology/core_id" ]] \
    && CORE_ID="$(<"${CPU_SYS}/topology/core_id")"

[[ -r "${CPU_SYS}/topology/physical_package_id" ]] \
    && PACKAGE_ID="$(<"${CPU_SYS}/topology/physical_package_id")"

# ---------------------------------------------------------------------------
# CPU power state
# ---------------------------------------------------------------------------

ORIGINAL_GOVERNOR=""
ORIGINAL_EPP=""

if [[ -r "${CPUFREQ_SYS}/scaling_governor" ]]; then
    ORIGINAL_GOVERNOR="$(<"${CPUFREQ_SYS}/scaling_governor")"
fi

if [[ -r "${CPUFREQ_SYS}/energy_performance_preference" ]]; then
    ORIGINAL_EPP="$(<"${CPUFREQ_SYS}/energy_performance_preference")"
fi

restore_cpu() {
    local status=$?

    trap - EXIT INT TERM
    set +e

    if [[ "$TUNE_CPU" == "1" ]]; then
        echo
        echo "Restoring CPU settings..."

        if [[ -n "$ORIGINAL_GOVERNOR" ]] \
            && [[ -w "${CPUFREQ_SYS}/scaling_governor" || -e "${CPUFREQ_SYS}/scaling_governor" ]]
        then
            printf '%s\n' "$ORIGINAL_GOVERNOR" \
                | sudo tee "${CPUFREQ_SYS}/scaling_governor" \
                >/dev/null
        fi

        if [[ -n "$ORIGINAL_EPP" ]] \
            && [[ -e "${CPUFREQ_SYS}/energy_performance_preference" ]]
        then
            printf '%s\n' "$ORIGINAL_EPP" \
                | sudo tee "${CPUFREQ_SYS}/energy_performance_preference" \
                >/dev/null
        fi

        echo "CPU settings restored."
    fi

    exit "$status"
}

trap restore_cpu EXIT INT TERM

# ---------------------------------------------------------------------------
# Information
# ---------------------------------------------------------------------------

echo "========================================"
echo " Hynergy benchmark runner"
echo "========================================"
echo "CPU:             ${CPU}"
echo "Physical core:   ${CORE_ID}"
echo "Package/socket:  ${PACKAGE_ID}"
echo "SMT siblings:    ${THREAD_SIBLINGS}"
echo "Mode:            ${MODE}"
echo "Baseline:        ${BASELINE}"
echo "Filter:          ${FILTER:-<none>}"
echo "CPU tuning:      ${TUNE_CPU}"
echo "Log:             ${LOG_FILE}"
echo

if [[ "$THREAD_SIBLINGS" == *,* || "$THREAD_SIBLINGS" == *-* ]]; then
    echo "note: CPU ${CPU} shares a physical core with logical CPU(s):"
    echo "      ${THREAD_SIBLINGS}"
    echo "      Heavy work on a sibling can add benchmark noise."
    echo
fi

# ---------------------------------------------------------------------------
# Compile before changing CPU policy or taking measurements.
# ---------------------------------------------------------------------------

echo "Pre-building benchmark binaries..."

cargo bench \
    -p hynergy-engine \
    --bench world \
    --no-run

cargo bench \
    -p hynergy-protocol \
    --bench world_commands \
    --no-run

cargo bench \
    -p hynergy-protocol \
    --bench definitions \
    --no-run

echo "Pre-build complete."
echo

# ---------------------------------------------------------------------------
# CPU tuning
#
# Feature-detected rather than AMD/Intel-specific.
# Works with drivers that expose standard cpufreq sysfs controls.
# ---------------------------------------------------------------------------

if [[ "$TUNE_CPU" == "1" ]]; then
    if [[ -e "${CPUFREQ_SYS}/scaling_governor" ]]; then
        AVAILABLE_GOVERNORS=""

        if [[ -r "${CPUFREQ_SYS}/scaling_available_governors" ]]; then
            AVAILABLE_GOVERNORS="$(
                <"${CPUFREQ_SYS}/scaling_available_governors"
            )"
        fi

        if grep -qw performance <<<"$AVAILABLE_GOVERNORS"; then
            echo "Setting CPU ${CPU} governor to performance..."

            printf '%s\n' performance \
                | sudo tee "${CPUFREQ_SYS}/scaling_governor" \
                >/dev/null
        else
            echo "note: performance governor unavailable; leaving governor unchanged"
        fi
    else
        echo "note: CPU frequency governor interface unavailable"
    fi

    if [[ -e "${CPUFREQ_SYS}/energy_performance_preference" ]]; then
        AVAILABLE_EPP=""

        if [[ -r "${CPUFREQ_SYS}/energy_performance_available_preferences" ]]; then
            AVAILABLE_EPP="$(
                <"${CPUFREQ_SYS}/energy_performance_available_preferences"
            )"
        fi

        if grep -qw performance <<<"$AVAILABLE_EPP"; then
            echo "Setting CPU ${CPU} EPP to performance..."

            printf '%s\n' performance \
                | sudo tee \
                    "${CPUFREQ_SYS}/energy_performance_preference" \
                >/dev/null
        else
            echo "note: EPP performance preference unavailable"
        fi
    fi
fi

echo

CURRENT_GOVERNOR="unknown"
CURRENT_EPP="unavailable"

[[ -r "${CPUFREQ_SYS}/scaling_governor" ]] \
    && CURRENT_GOVERNOR="$(<"${CPUFREQ_SYS}/scaling_governor")"

[[ -r "${CPUFREQ_SYS}/energy_performance_preference" ]] \
    && CURRENT_EPP="$(<"${CPUFREQ_SYS}/energy_performance_preference")"

echo "Benchmark CPU state:"
echo "  governor: ${CURRENT_GOVERNOR}"
echo "  EPP:      ${CURRENT_EPP}"

if command -v cpupower >/dev/null 2>&1; then
    echo
    cpupower -c "$CPU" frequency-info || true
fi

echo
echo "Allowing CPU to settle for ${SETTLE_SECONDS}s..."
sleep "$SETTLE_SECONDS"

# ---------------------------------------------------------------------------
# Criterion arguments
# ---------------------------------------------------------------------------

CRITERION_ARGS=()

if [[ -n "$FILTER" ]]; then
    CRITERION_ARGS+=("$FILTER")
fi

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

# ---------------------------------------------------------------------------
# Record environment metadata.
# ---------------------------------------------------------------------------

{
    echo "========================================"
    echo "Hynergy benchmark metadata"
    echo "========================================"

    echo "date: $(date --iso-8601=seconds)"
    echo "hostname: $(hostname)"
    echo "kernel: $(uname -srmo)"

    echo
    echo "benchmark:"
    echo "  cpu: ${CPU}"
    echo "  core: ${CORE_ID}"
    echo "  package: ${PACKAGE_ID}"
    echo "  siblings: ${THREAD_SIBLINGS}"
    echo "  mode: ${MODE}"
    echo "  baseline: ${BASELINE}"
    echo "  filter: ${FILTER:-<none>}"

    echo
    echo "power:"
    echo "  governor: ${CURRENT_GOVERNOR}"
    echo "  EPP: ${CURRENT_EPP}"

    echo
    echo "rustc:"
    rustc --version --verbose

    echo
    echo "cargo:"
    cargo --version

    if command -v lscpu >/dev/null 2>&1; then
        echo
        echo "lscpu:"
        lscpu
    fi

    if command -v cpupower >/dev/null 2>&1; then
        echo
        echo "cpupower:"
        cpupower -c "$CPU" frequency-info || true
    fi

    echo
    echo "========================================"
} >"$LOG_FILE"

# ---------------------------------------------------------------------------
# Benchmark execution
# ---------------------------------------------------------------------------

run_bench() {
    local package="$1"
    local bench="$2"

    echo
    echo "========================================"
    echo " ${package} / ${bench}"
    echo "========================================"

    {
        echo
        echo "========================================"
        echo "${package} / ${bench}"
        echo "========================================"
    } >>"$LOG_FILE"

    taskset -c "$CPU" \
        cargo bench \
            -p "$package" \
            --bench "$bench" \
            -- \
            "${CRITERION_ARGS[@]}" \
        2>&1 | tee -a "$LOG_FILE"
}

run_bench hynergy-engine world
run_bench hynergy-protocol definitions
run_bench hynergy-protocol world_commands

echo
echo "========================================"
echo " Benchmark run completed"
echo "========================================"
echo "Results log:"
echo "  ${LOG_FILE}"