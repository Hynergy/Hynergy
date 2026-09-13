use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use hynergy_engine::Engine;
use hynergy_model::device::definition::{DefinitionId, PrimitiveElementKind};
use hynergy_protocol::register_definition_buffer;
use std::{hint::black_box, time::Duration};

const DEFINITION_MAGIC: [u8; 4] = *b"HYDF";
const DEFINITION_VERSION: u16 = 1;

const ADD_TERMINAL: u16 = 1;
const ADD_NODE: u16 = 2;
const ADD_PARAMETER: u16 = 3;
const ADD_ELEMENT: u16 = 4;

const VALUE_LITERAL: u8 = 0;

const SIZES: &[usize] = &[8, 32, 128, 512];

struct DefinitionBufferBuilder {
    bytes: Vec<u8>,
    command_count: u32,
}

impl DefinitionBufferBuilder {
    fn new() -> Self {
        let mut bytes = Vec::with_capacity(256);

        bytes.extend_from_slice(&DEFINITION_MAGIC);
        bytes.extend_from_slice(&DEFINITION_VERSION.to_le_bytes());
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
                .expect("benchmark definition payload must fit u32")
                .to_le_bytes(),
        );

        self.bytes.extend_from_slice(payload);

        self.command_count = self
            .command_count
            .checked_add(1)
            .expect("benchmark command count exhausted u32");
    }

    fn add_terminal(&mut self) {
        self.command(ADD_TERMINAL, &[]);
    }

    fn add_node(&mut self) {
        self.command(ADD_NODE, &[]);
    }

    fn add_parameter(&mut self) {
        self.command(ADD_PARAMETER, &0_u16.to_le_bytes());
    }

    fn add_admittance_element(&mut self) {
        let definition: DefinitionId = PrimitiveElementKind::Admittance.into();

        let mut payload = Vec::with_capacity(29);

        payload.extend_from_slice(&definition.get().to_le_bytes());

        payload.extend_from_slice(&2_u32.to_le_bytes());

        payload.extend_from_slice(&0_u32.to_le_bytes());
        payload.extend_from_slice(&1_u32.to_le_bytes());

        payload.extend_from_slice(&1_u32.to_le_bytes());

        payload.push(VALUE_LITERAL);
        payload.extend_from_slice(&1.0_f64.to_le_bytes());

        self.command(ADD_ELEMENT, &payload);
    }

    fn finish(mut self) -> (Vec<u8>, u64) {
        self.bytes[12..16].copy_from_slice(&self.command_count.to_le_bytes());

        (self.bytes, u64::from(self.command_count))
    }
}

fn interface_definition(size: usize) -> (Vec<u8>, u64) {
    let mut buffer = DefinitionBufferBuilder::new();

    for _ in 0..size {
        buffer.add_terminal();
        buffer.add_node();
    }

    buffer.finish()
}

fn parameter_definition(size: usize) -> (Vec<u8>, u64) {
    let mut buffer = DefinitionBufferBuilder::new();

    for _ in 0..size {
        buffer.add_parameter();
    }

    buffer.finish()
}

fn element_definition(size: usize) -> (Vec<u8>, u64) {
    let mut buffer = DefinitionBufferBuilder::new();

    buffer.add_terminal();
    buffer.add_terminal();

    for _ in 0..size {
        buffer.add_admittance_element();
    }

    buffer.finish()
}

fn bench_definition_shape(c: &mut Criterion, name: &str, builder: fn(usize) -> (Vec<u8>, u64)) {
    let mut group = c.benchmark_group(format!("definitions/{name}"));

    for &size in SIZES {
        let (buffer, command_count) = builder(size);

        group.throughput(Throughput::ElementsAndBytes {
            elements: command_count,
            bytes: buffer.len() as u64,
        });

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter_batched_ref(
                Engine::new,
                |engine| {
                    let id = register_definition_buffer(engine, &buffer).unwrap();

                    black_box(id);
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_definitions(c: &mut Criterion) {
    bench_definition_shape(c, "terminal_node_pairs", interface_definition);

    bench_definition_shape(c, "parameters", parameter_definition);

    bench_definition_shape(c, "elements", element_definition);
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = bench_definitions
}

criterion_main!(benches);
