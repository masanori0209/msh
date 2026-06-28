use criterion::{black_box, criterion_group, criterion_main, Criterion};
use msh::{expand, highlight, parse, path_cache};
use std::collections::HashMap;

fn bench_parse(c: &mut Criterion) {
    c.bench_function("parse_pipeline", |b| {
        b.iter(|| parse::parse_line(black_box("echo hello | wc -c")).unwrap())
    });
}

fn bench_expand(c: &mut Criterion) {
    c.bench_function("expand_vars", |b| {
        b.iter(|| expand::expand_vars(black_box("$MSH_BENCH/path")))
    });
}

fn bench_highlight(c: &mut Criterion) {
    c.bench_function("highlight_line", |b| {
        b.iter(|| highlight::highlight_line(black_box("echo \"hello world\" | wc -l")))
    });
}

fn bench_completion(c: &mut Criterion) {
    let aliases = HashMap::new();
    c.bench_function("complete_commands_ec", |b| {
        b.iter(|| path_cache::complete_commands(black_box("ec"), &aliases, true))
    });
}

fn bench_parse_redirect(c: &mut Criterion) {
    c.bench_function("parse_redirect_pipeline", |b| {
        b.iter(|| parse::parse_line(black_box("echo hello > /tmp/out.txt | wc -c")).unwrap())
    });
}

criterion_group!(
    benches,
    bench_parse,
    bench_expand,
    bench_highlight,
    bench_completion,
    bench_parse_redirect
);
criterion_main!(benches);
