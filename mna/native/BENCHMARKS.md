# Native benchmarks

The `hynergy-benchmarks` crate contains all native Criterion benchmarks and their shared fixtures. The production crates
do not depend on Criterion.

## Quick start

Run these commands from `mna/native` (the runner also works when invoked from another directory):

```bash
./scripts/bench-linux.sh check
./scripts/bench-linux.sh list
./scripts/bench-linux.sh run
./scripts/bench-linux.sh run --suite full
```

The `core` suite is the default. It uses one representative scale for each scaling group and skips the complete CPU
target. Use `full` to run all scales, the stress sizes, and the complete CPU target.

The runner pins the process to one logical CPU. By default, it also tries to select the performance governor and
performance energy preference. Use `--no-tune` or set `TUNE_CPU=0` when you do not want the runner to change CPU policy.
CLI options override their matching environment variables.

Use `--filter` to limit Criterion cases and repeat `--target` to limit benchmark targets:

```bash
./scripts/bench-linux.sh run --filter 'world/tick/matrix_dirty'
./scripts/bench-linux.sh run --target topology
./scripts/bench-linux.sh run --suite full --target cpu_validated --filter 'world/cpu/validated'
```

Create and compare Criterion baselines independently:

```bash
./scripts/bench-linux.sh run --save before
./scripts/bench-linux.sh run --compare before
./scripts/bench-linux.sh run --compare before --save after
```

The combined form measures each selected benchmark once: Criterion compares the fresh `new` result against `before`,
and the runner then promotes only the results produced by that run to `after`. The old commands remain compatibility
aliases:

```bash
./scripts/bench-linux.sh save before
./scripts/bench-linux.sh compare before
```

Criterion-specific measurement settings can be forwarded after `--`. Baseline arguments are intentionally managed by
the runner so `--compare` and `--save` remain unambiguous:

```bash
./scripts/bench-linux.sh run \
    --target topology \
    --compare before \
    --save after \
    -- \
    --warm-up-time 5 \
    --measurement-time 15 \
    --sample-size 200
```

The runner writes environment and result logs to `target/bench-logs`. Logs include the selected targets, compare/save
baselines, and forwarded Criterion arguments.

Useful environment variables are still supported for automation:

```bash
CPU=13 SUITE=full ./scripts/bench-linux.sh run
FILTER='world/tick/warm_sleep/simple/count_256' ./scripts/bench-linux.sh run
TUNE_CPU=0 SETTLE_SECONDS=0 ./scripts/bench-linux.sh run
```

Equivalent CLI options are `--cpu`, `--suite`, `--filter`, `--no-tune`/`--tune`, and `--settle`.

## Targets

| Target          | Purpose                                                  |
|-----------------|----------------------------------------------------------|
| `world_tick`    | Cold, sleeping, active, dirty, and mixed world ticks     |
| `topology`      | Island merge and split mutations                         |
| `subscriptions` | Sparse, dense sleeping, and dense changing subscriptions |
| `cpu_synthetic` | Synthetic CPU-shaped scaling workload                    |
| `cpu_validated` | Gate-built adders with registered outputs                |
| `cpu_full`      | Complete autonomous 8-/32-bit CPUs; full suite only      |
| `protocol`      | Definition-buffer registration by command shape          |

Use `./scripts/bench-linux.sh list` to get the exact Criterion case names for the active suite. Add one or more
`--target` options when you only want cases from selected benchmark targets.

## Flamegraphs

The profile command requires one target and one exact case. It rejects zero or multiple matches before it starts `perf`.

```bash
./scripts/bench-linux.sh profile \
    world_tick \
    world/tick/matrix_dirty/medium/count_32
```

Use `--profile-seconds` to change the default 20-second profile:

```bash
./scripts/bench-linux.sh profile \
    cpu_validated \
    world/cpu/validated/steady_active/small_lanes12_devices1177 \
    --profile-seconds 60
```

`PROFILE_SECONDS` remains available for automation.

The runner writes SVG files to `target/flamegraphs`.

Start with steady-state cases. Criterion excludes `iter_batched` setup from its timing, but an external process profiler
can still sample that setup. Cold and topology profiles can therefore include fixture construction as well as the
measured operation.

Flamegraphs require Linux `perf` and `cargo-flamegraph`. The host must also permit performance-event access.

## Correctness fixtures

Run the fixture tests before you interpret benchmark results:

```bash
cargo test -p hynergy-benchmarks --test circuit_fixtures
```

The tests verify circuit construction, dirty mutations, topology changes, subscriptions, digital truth tables,
registered arithmetic, workload sizes, and the complete CPU program.

The complete runner regression test does not execute real benchmarks; it uses mock commands to validate argument routing,
baseline promotion, compatibility aliases, and profiling options:

```bash
./scripts/test-bench-linux.sh
```

The validated CPU tiers use primitive logic gates to build 8-bit ripple-carry adders. Each result and carry output
passes through a `TickDelay` register. The test accepts a digital LOW at or below 1.0 V and a digital HIGH at or above
4.0 V. It rejects values in the ambiguous range.

The complete CPU benchmark has 8-bit and 32-bit datapath variants. Both contain a 4-bit program counter, two data
registers, a carry flag, a two-phase FETCH/EXEC controller, a gate-built 16-word ROM, an adder, logic operations, and
branch control. The instruction register is the datapath width plus the 3-bit opcode. The 8-bit fixture has 549 devices,
including 515 nonlinear logic gates and 33 stateful devices. The 32-bit fixture has 1,557 devices, including 1,451
nonlinear logic gates and 105 stateful devices. The correctness tests execute the same program at both widths, including
an overflow-to-zero ADD and a taken conditional branch.

## Solver convergence profiling

Solver profiling is feature-gated and is not enabled by the normal Criterion runner. This keeps profiling counters and
per-iteration trace storage out of ordinary benchmark builds.

Run the CPU convergence diagnostic from `mna/native`:

```bash
cargo run --release -p hynergy-benchmarks \
    --features solver-profiling \
    --example cpu_solver_profile -- 20
```

The optional final argument is the number of simulation ticks to run per CPU width. Each tick reports total nonlinear
iterations, MNA solves, matrix factorizations/restamps, stability-value changes, and the maximum solution delta. It also
prints the per-iteration trace for the nonlinear island that required the most iterations during that tick.

A steadily decreasing stability-change count usually indicates propagation depth. A repeating or oscillating change
pattern points instead toward a convergence problem where damping, continuation, or a different nonlinear algorithm may
help.
