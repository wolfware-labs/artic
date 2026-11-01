// The reactive runtime's hot paths, through the public API: reads and writes, a write that reaches
// many effects, memo chains, a memo with many sources, a batch, and a scope's whole life.
//
// To compare a change, save a baseline on main and compare the branch against it on the same
// machine:
//
//   cargo bench -p artic-core --bench reactive -- --save-baseline main
//   cargo bench -p artic-core --bench reactive -- --baseline main
//
// Each group builds its graph in a root scope and disposes it after, then checks that the measured
// work ran and that nothing leaked.

use artic_core::reactive::{Memo, Scope, Signal, batch, effect, flush_effects, live_counts};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::cell::Cell;
use std::hint::black_box;
use std::rc::Rc;

/// Builds a graph with `build` in a root scope, measures it with `measure`, then disposes the
/// scope and checks that every node it made is gone.
fn in_scope<TGraph>(build: impl FnOnce() -> TGraph, measure: impl FnOnce(&TGraph)) {
  let before = live_counts();
  let scope = Scope::new_root();
  let graph = scope.run(build);
  measure(&graph);
  scope.dispose();
  assert_eq!(live_counts(), before, "the bench leaked reactive nodes");
}

/// A counter that effects bump each time they run.
fn counter() -> Rc<Cell<u64>> {
  Rc::new(Cell::new(0))
}

// Reading and writing a signal no one reads: the cost every other bench builds on.
fn signal(c: &mut Criterion) {
  let mut group = c.benchmark_group("signal");
  in_scope(
    || Signal::new(0_u64),
    |signal| {
      group.bench_function("get", |b| b.iter(|| black_box(signal.get())));
      let mut value = 0;
      group.bench_function("set", |b| {
        b.iter(|| {
          value += 1;
          signal.set(black_box(value));
        });
      });
      assert_eq!(signal.get(), value);
    },
  );
  group.finish();
}

// A write to a signal that `count` effects read, then a flush: it runs every effect, which reads
// the signal again and so tracks it again.
fn fan_out(c: &mut Criterion) {
  let mut group = c.benchmark_group("fan_out");
  for count in [1_u64, 10, 100] {
    let runs = counter();
    in_scope(
      || {
        let signal = Signal::new(0_u64);
        for _ in 0..count {
          let runs = Rc::clone(&runs);
          effect(move || {
            black_box(signal.get());
            runs.set(runs.get() + 1);
          });
        }
        signal
      },
      |signal| {
        let mut writes = 0;
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
          b.iter(|| {
            writes += 1;
            signal.set(writes);
            flush_effects();
          });
        });
        assert_eq!(runs.get(), count * (1 + writes), "an effect missed a write");
      },
    );
  }
  group.finish();
}

// A write at the start of a chain of `length` memos, each reading the one before, then a read at
// its end, which recomputes the whole chain.
fn memo_chain(c: &mut Criterion) {
  let mut group = c.benchmark_group("memo_chain");
  for length in [10_u64, 100] {
    in_scope(
      || {
        let source = Signal::new(0_u64);
        let first = Memo::new(move || source.get() + 1);
        let last = (1..length).fold(first, |previous, _| Memo::new(move || previous.get() + 1));
        (source, last)
      },
      |(source, last)| {
        let mut value = 0;
        group.bench_with_input(BenchmarkId::from_parameter(length), &length, |b, _| {
          b.iter(|| {
            value += 1;
            source.set(value);
            black_box(last.get())
          });
        });
        assert_eq!(last.get(), value + length);
      },
    );
  }
  group.finish();
}

// A write to one of the `count` signals a memo sums, then a read of the memo, which reads and
// tracks every signal again.
fn tracked_reads(c: &mut Criterion) {
  let mut group = c.benchmark_group("tracked_reads");
  for count in [10_u64, 100] {
    in_scope(
      || {
        let signals: Vec<Signal<u64>> = (0..count).map(|_| Signal::new(1)).collect();
        let summed = signals.clone();
        (signals, Memo::new(move || summed.iter().map(Signal::get).sum::<u64>()))
      },
      |(signals, sum)| {
        let mut value = 1;
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
          b.iter(|| {
            value += 1;
            signals[0].set(value);
            black_box(sum.get())
          });
        });
        assert_eq!(sum.get(), value + count - 1);
      },
    );
  }
  group.finish();
}

// A batch that writes each of 100 signals one effect reads: the effect runs once per batch.
fn batched(c: &mut Criterion) {
  let mut group = c.benchmark_group("batch");
  let runs = counter();
  in_scope(
    || {
      let signals: Vec<Signal<u64>> = (0..100).map(|_| Signal::new(0)).collect();
      let read = signals.clone();
      let effect_runs = Rc::clone(&runs);
      effect(move || {
        black_box(read.iter().map(Signal::get).sum::<u64>());
        effect_runs.set(effect_runs.get() + 1);
      });
      signals
    },
    |signals| {
      let mut batches = 0;
      group.bench_function("100", |b| {
        b.iter(|| {
          batches += 1;
          batch(|| signals.iter().for_each(|signal| signal.set(batches)));
        });
      });
      assert_eq!(runs.get(), 1 + batches, "the effect ran other than once per batch");
    },
  );
  group.finish();
}

// A scope's whole life: 10 signals, a memo over them and an effect reading the memo, made and then
// disposed.
fn scope(c: &mut Criterion) {
  let before = live_counts();
  c.bench_function("scope/create_dispose", |b| {
    b.iter(|| {
      let scope = Scope::new_root();
      scope.run(|| {
        let signals: Vec<Signal<u64>> = (0..10).map(Signal::new).collect();
        let sum = Memo::new(move || signals.iter().map(Signal::get).sum::<u64>());
        effect(move || {
          black_box(sum.get());
        });
      });
      scope.dispose();
    });
  });
  assert_eq!(live_counts(), before, "the bench leaked reactive nodes");
}

criterion_group!(reactive, signal, fan_out, memo_chain, tracked_reads, batched, scope);
criterion_main!(reactive);
