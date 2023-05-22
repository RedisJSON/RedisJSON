use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rejson::jsonpath::compile;

pub fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function(
        "compile", 
        |b| b.iter(|| {
            let _ = compile(black_box("$"));
        }),
    );
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
