# Native benchmarks

The `hynergy-benchmarks` crate contains all native Criterion benchmarks and their shared fixtures. The production crates
do not depend on Criterion.

## Quick start

Run these commands from `mna/native`:

```bash
./bench-linux.sh check
./bench-linux.sh list
./bench-linux.sh run
SUITE=full ./bench-linux.sh run
```

The `core` suite is the default. It uses one representative scale for each scaling group and skips the complete CPU
target. Use `full` to run all scales, the stress sizes, and the complete CPU target.

The runner pins the process to one logical CPU. By default, it also tries to select the performance governor and
performance energy preference. Set `TUNE_CPU=0` when you do not want the runner to change CPU policy.

Use a Criterion filter to limit a run:

```bash
FILTER='world/tick/matrix_dirty' ./bench-linux.sh run
FILTER='world/cpu/validated' SUITE=full ./bench-linux.sh run
```

Create and compare a Criterion baseline:

```bash
./bench-linux.sh save before
./bench-linux.sh compare before
```

The runner writes environment and result logs to `target/bench-logs`.

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

Use `./bench-linux.sh list` to get the exact Criterion case names for the active suite.

## Flamegraphs

The profile command requires one target and one exact case. It rejects zero or multiple matches before it starts `perf`.

```bash
./bench-linux.sh profile \
    world_tick \
    world/tick/matrix_dirty/medium/count_32
```

Set `PROFILE_SECONDS` to change the default 20-second profile:

```bash
PROFILE_SECONDS=30 ./bench-linux.sh profile \
    cpu_validated \
    world/cpu/validated/steady_active/small_lanes12_devices1177
```

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

The validated CPU tiers use primitive logic gates to build 8-bit ripple-carry adders. Each result and carry output passes
through a `TickDelay` register. The test accepts a digital LOW at or below 1.0 V and a digital HIGH at or above 4.0 V.
It rejects values in the ambiguous range.

The complete CPU benchmark has 8-bit and 32-bit datapath variants. Both contain a 4-bit program counter, two data
registers, a carry flag, a two-phase FETCH/EXEC controller, a gate-built 16-word ROM, an adder, logic operations, and
branch control. The instruction register is the datapath width plus the 3-bit opcode. The 8-bit fixture has 549 devices,
including 515 nonlinear logic gates and 33 stateful devices. The 32-bit fixture has 1,557 devices, including 1,451
nonlinear logic gates and 105 stateful devices. The correctness tests execute the same program at both widths, including
an overflow-to-zero ADD and a taken conditional branch.
