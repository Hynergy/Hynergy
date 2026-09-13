use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use hynergy_engine::Engine;
use hynergy_model::device::definition::{DefinitionId, PrimitiveElementKind};
use hynergy_protocol::apply_world_command_buffer;
use std::{hint::black_box, time::Duration};

const WORLD_MAGIC: [u8; 4] = *b"HYWC";
const WORLD_VERSION: u16 = 1;

const ADD_WIRE: u16 = 1;
const CONNECT_WIRES: u16 = 3;
const ADD_DEVICE: u16 = 5;
const ATTACH_TERMINAL: u16 = 7;

const BUILD_SIZES: &[usize] = &[16, 64, 256, 1_024];

struct WorldBufferBuilder {
    bytes: Vec<u8>,
    command_count: u32,
}

impl WorldBufferBuilder {
    fn new() -> Self {
        let mut bytes = Vec::with_capacity(256);

        bytes.extend_from_slice(&WORLD_MAGIC);
        bytes.extend_from_slice(&WORLD_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());

        Self {
            bytes,
            command_count: 0,
        }
    }

    fn command(&mut self, tag: u16, payload: &[u8]) {
        self.bytes.extend_from_slice(&tag.to_le_bytes());

        self.bytes.extend_from_slice(
            &u32::try_from(payload.len())
                .expect("benchmark command payload must fit u32")
                .to_le_bytes(),
        );

        self.bytes.extend_from_slice(payload);

        self.command_count = self
            .command_count
            .checked_add(1)
            .expect("benchmark command count exhausted u32");
    }

    fn add_wire(&mut self, wire: u32) {
        self.command(ADD_WIRE, &wire.to_le_bytes());
    }

    fn connect_wires(&mut self, a: u32, b: u32) {
        let mut payload = [0_u8; 8];

        payload[0..4].copy_from_slice(&a.to_le_bytes());
        payload[4..8].copy_from_slice(&b.to_le_bytes());

        self.command(CONNECT_WIRES, &payload);
    }

    fn add_device(&mut self, device: u32, definition: u32) {
        let mut payload = [0_u8; 8];

        payload[0..4].copy_from_slice(&device.to_le_bytes());
        payload[4..8].copy_from_slice(&definition.to_le_bytes());

        self.command(ADD_DEVICE, &payload);
    }

    fn attach_terminal(&mut self, wire: u32, device: u32, terminal: u32) {
        let mut payload = [0_u8; 12];

        payload[0..4].copy_from_slice(&wire.to_le_bytes());
        payload[4..8].copy_from_slice(&device.to_le_bytes());
        payload[8..12].copy_from_slice(&terminal.to_le_bytes());

        self.command(ATTACH_TERMINAL, &payload);
    }

    fn finish(mut self) -> (Vec<u8>, u64) {
        self.bytes[12..16].copy_from_slice(&self.command_count.to_le_bytes());

        (self.bytes, u64::from(self.command_count))
    }
}

fn raw(index: usize) -> u32 {
    u32::try_from(index).expect("benchmark ID must fit u32")
}

fn admittance() -> u32 {
    let definition: DefinitionId = PrimitiveElementKind::Admittance.into();

    definition.get()
}

fn add_wires_buffer(count: usize) -> (Vec<u8>, u64) {
    let mut buffer = WorldBufferBuilder::new();

    for id in 1..=count {
        buffer.add_wire(raw(id));
    }

    buffer.finish()
}

fn wire_chain_buffer(count: usize) -> (Vec<u8>, u64) {
    let mut buffer = WorldBufferBuilder::new();

    for id in 1..=count {
        buffer.add_wire(raw(id));
    }

    for id in 1..count {
        buffer.connect_wires(raw(id), raw(id + 1));
    }

    buffer.finish()
}

fn mixed_world_buffer(size: usize) -> (Vec<u8>, u64) {
    let mut buffer = WorldBufferBuilder::new();

    for id in 1..=size {
        buffer.add_wire(raw(id));
    }

    for id in 1..size {
        buffer.connect_wires(raw(id), raw(id + 1));
    }

    let definition = admittance();

    for id in 1..=size {
        buffer.add_device(raw(id), definition);

        buffer.attach_terminal(raw(id), raw(id), 0);
    }

    buffer.finish()
}

fn fresh_engine_world() -> (Engine, u32) {
    let mut engine = Engine::new();
    let world = engine.new_world().unwrap();

    (engine, world)
}

fn bench_single_add_wire(c: &mut Criterion) {
    let (buffer, _) = add_wires_buffer(1);

    c.bench_function("protocol/single_add_wire", |b| {
        b.iter_batched_ref(
            fresh_engine_world,
            |state| {
                let (engine, world) = state;

                apply_world_command_buffer(engine, *world, &buffer).unwrap();

                black_box(engine.world(*world).unwrap().network());
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_add_wire_batching(c: &mut Criterion) {
    let mut group = c.benchmark_group("protocol/batching_add_wires");

    for &size in BUILD_SIZES {
        let (single_buffer, command_count) = add_wires_buffer(size);

        group.throughput(Throughput::ElementsAndBytes {
            elements: command_count,
            bytes: single_buffer.len() as u64,
        });

        group.bench_with_input(BenchmarkId::new("one_buffer", size), &size, |b, _| {
            b.iter_batched_ref(
                fresh_engine_world,
                |state| {
                    let (engine, world) = state;

                    apply_world_command_buffer(engine, *world, &single_buffer).unwrap();

                    black_box(engine.world(*world).unwrap().network());
                },
                BatchSize::SmallInput,
            );
        });

        let individual_buffers: Vec<Vec<u8>> = (1..=size)
            .map(|id| {
                let mut buffer = WorldBufferBuilder::new();

                buffer.add_wire(raw(id));
                buffer.finish().0
            })
            .collect();

        let total_bytes: u64 = individual_buffers
            .iter()
            .map(|buffer| buffer.len() as u64)
            .sum();

        group.throughput(Throughput::ElementsAndBytes {
            elements: size as u64,
            bytes: total_bytes,
        });

        group.bench_with_input(
            BenchmarkId::new("one_command_buffers", size),
            &size,
            |b, _| {
                b.iter_batched_ref(
                    fresh_engine_world,
                    |state| {
                        let (engine, world) = state;

                        for buffer in &individual_buffers {
                            apply_world_command_buffer(engine, *world, buffer).unwrap();
                        }

                        black_box(engine.world(*world).unwrap().network());
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_world_buffer(c: &mut Criterion, name: &str, builder: fn(usize) -> (Vec<u8>, u64)) {
    let mut group = c.benchmark_group(format!("protocol/{name}"));

    for &size in BUILD_SIZES {
        let (buffer, command_count) = builder(size);

        group.throughput(Throughput::ElementsAndBytes {
            elements: command_count,
            bytes: buffer.len() as u64,
        });

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter_batched_ref(
                fresh_engine_world,
                |state| {
                    let (engine, world) = state;

                    apply_world_command_buffer(engine, *world, &buffer).unwrap();

                    black_box(engine.world(*world).unwrap().network());
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_world_buffers(c: &mut Criterion) {
    bench_world_buffer(c, "wire_chain_build", wire_chain_buffer);

    bench_world_buffer(c, "mixed_world_build", mixed_world_buffer);
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(60)
        .warm_up_time(Duration::from_secs(3))
        .measurement_time(Duration::from_secs(8));
    targets =
        bench_single_add_wire,
        bench_add_wire_batching,
        bench_world_buffers
}

criterion_main!(benches);
