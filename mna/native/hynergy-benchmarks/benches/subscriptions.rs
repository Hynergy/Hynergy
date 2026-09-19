use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use hynergy_benchmarks::fixtures::workloads::build_subscription_case;
use hynergy_benchmarks::fixtures::{BenchSuite, SubscriptionProfile};
use std::{hint::black_box, time::Duration};

fn bench_subscriptions(c: &mut Criterion) {
    let suite = BenchSuite::from_env();
    let count = if suite.is_full() { 4096 } else { 256 };
    let mut group = c.benchmark_group("world/tick/subscriptions");
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(6));

    for profile in SubscriptionProfile::ALL {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_function(
            BenchmarkId::new(profile.name(), format!("count_{count}")),
            |b| {
                let mut scenario = build_subscription_case(profile, count);
                scenario.tick().unwrap();
                if profile.changes_each_tick() {
                    b.iter_custom(|iterations| scenario.measure_rhs_dirty_ticks(iterations));
                } else {
                    b.iter(|| {
                        scenario.tick().unwrap();
                        black_box(scenario.subscription_update_count());
                    });
                }
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_subscriptions);
criterion_main!(benches);
