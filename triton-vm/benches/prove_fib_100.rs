use criterion::criterion_group;
use criterion::criterion_main;
use criterion::BenchmarkId;
use criterion::Criterion;
use twenty_first::shared_math::b_field_element::BFieldElement;

use triton_vm::prof_start;
use triton_vm::prof_stop;
use triton_vm::profiler::Report;
use triton_vm::profiler::TritonProfiler;
use triton_vm::program::Program;
use triton_vm::proof::Claim;
use triton_vm::shared_tests::FIBONACCI_SEQUENCE;
use triton_vm::stark::Stark;
use triton_vm::stark::StarkHasher;
use triton_vm::vm::simulate;
use triton_vm::StarkParameters;

/// cargo criterion --bench prove_fib_100
fn prove_fib_100(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("prove_fib_100");
    group.sample_size(10); // runs

    let fib_100 = BenchmarkId::new("ProveFib100", 0);

    let mut maybe_profiler = Some(TritonProfiler::new("Prove Fibonacci 100"));
    let mut report: Report = Report::placeholder();

    // stark object
    prof_start!(maybe_profiler, "parse program");
    let program = match Program::from_code(FIBONACCI_SEQUENCE) {
        Err(e) => panic!("Cannot compile source code into program: {e}"),
        Ok(p) => p,
    };
    prof_stop!(maybe_profiler, "parse program");
    let public_input = [100].map(BFieldElement::new).to_vec();
    prof_start!(maybe_profiler, "generate AET");
    let (aet, output) = simulate(&program, public_input.clone(), vec![]).unwrap();
    prof_stop!(maybe_profiler, "generate AET");

    let parameters = StarkParameters::default();
    let claim = Claim {
        input: public_input,
        program_digest: program.hash::<StarkHasher>(),
        output,
    };
    let proof = Stark::prove(&parameters, &claim, &aet, &mut maybe_profiler);

    let padded_height = proof.padded_height();
    let max_degree = Stark::derive_max_degree(padded_height, parameters.num_trace_randomizers);
    let fri = Stark::derive_fri(&parameters, max_degree);

    if let Some(profiler) = maybe_profiler.as_mut() {
        profiler.finish();
        report = profiler.report(
            Some(aet.processor_trace.nrows()),
            Some(padded_height),
            Some(fri.domain.length),
        );
    }
    //start the benchmarking
    group.bench_function(fib_100, |bencher| {
        bencher.iter(|| {
            let _proof = Stark::prove(&parameters, &claim, &aet, &mut None);
        });
    });

    group.finish();

    println!("Writing report ...");
    println!("{report}");
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = prove_fib_100
}

criterion_main!(benches);
