// The reactive runtime's hot paths, through the public API: reads and writes, a write that reaches
// many effects directly or through a memo, memo chains read after a change and without one, a memo
// whose value doesn't change, a diamond, a memo with many sources, a batch, and a scope's whole
// life.
//
// To compare a change, save a baseline on main and compare the branch against it on the same
// machine:
//
//   cargo bench -p artic-core --bench reactive -- --save-baseline main
//   cargo bench -p artic-core --bench reactive -- --baseline main
//
// Every write is followed by a flush, as the dispatcher does after each event, so a bench measures
// the whole cost of a change. Each bench checks that the measured work ran and that no node leaked.

use artic_core::reactive::{LiveCounts, Memo, Scope, Signal, batch, effect, flush_effects, live_counts};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::cell::Cell;
use std::hint::black_box;
use std::rc::Rc;

/// Checks that every reactive node made since `before` is gone.
fn assert_no_leak(before: LiveCounts) {
  assert_eq!(live_counts(), before, "the bench leaked reactive nodes");
}

/// Builds a graph with `build` in a root scope, measures it with `measure`, then disposes the
/// scope and checks that every node it made is gone.
fn in_scope<TGraph>(build: impl FnOnce() -> TGraph, measure: impl FnOnce(&TGraph)) {
  let before = live_counts();
  let scope = Scope::new_root();
  let graph = scope.run(build);
  measure(&graph);
  scope.dispose();
  assert_no_leak(before);
}

/// A counter that effects and memos bump each time they run.
fn counter() -> Rc<Cell<u64>> {
  Rc::new(Cell::new(0))
}

/// Adds one to `counter`.
fn bump(counter: &Cell<u64>) {
  counter.set(counter.get() + 1);
}

/// Writes `value` to `signal` and flushes, as the dispatcher does after an event.
fn write(signal: &Signal<u64>, value: u64) {
  signal.set(value);
  flush_effects();
}

// Reading a signal no one reads, and writing it: the cost every other bench builds on.
fn signal(c: &mut Criterion) {
  let mut group = c.benchmark_group("signal");
  in_scope(
    || Signal::new(0_u64),
    |signal| {
      group.bench_function("get", |b| b.iter(|| signal.get()));
      let mut value = 0;
      group.bench_function("set", |b| {
        b.iter(|| {
          value += 1;
          write(signal, value);
        });
      });
      assert_eq!(signal.get(), value);
    },
  );
  group.finish();
}

// A write to a signal that `count` effects read: the flush runs every effect, which reads the
// signal again and so tracks it again.
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
            bump(&runs);
          });
        }
        signal
      },
      |signal| {
        let mut writes = 0;
        group.bench_function(BenchmarkId::from_parameter(count), |b| {
          b.iter(|| {
            writes += 1;
            write(signal, writes);
          });
        });
        assert_eq!(runs.get(), count * (1 + writes), "an effect missed a write");
      },
    );
  }
  group.finish();
}

// A write to a signal a memo reads, which `count` effects read: the flush reaches the effects
// through the memo, which computes once for all of them.
fn memo_fan_out(c: &mut Criterion) {
  let mut group = c.benchmark_group("memo_fan_out");
  for count in [1_u64, 10, 100] {
    let (computes, runs) = (counter(), counter());
    in_scope(
      || {
        let signal = Signal::new(0_u64);
        let memo_computes = Rc::clone(&computes);
        let memo = Memo::new(move || {
          bump(&memo_computes);
          signal.get() + 1
        });
        for _ in 0..count {
          let runs = Rc::clone(&runs);
          effect(move || {
            black_box(memo.get());
            bump(&runs);
          });
        }
        signal
      },
      |signal| {
        let mut writes = 0;
        group.bench_function(BenchmarkId::from_parameter(count), |b| {
          b.iter(|| {
            writes += 1;
            write(signal, writes);
          });
        });
        assert_eq!((computes.get(), runs.get()), (1 + writes, count * (1 + writes)));
      },
    );
  }
  group.finish();
}

/// A chain of `length` memos, each one more than the one before, the first one more than
/// `source`. `computes` counts the first memo's computations.
fn chain(source: Memo<u64>, length: u64, computes: &Rc<Cell<u64>>) -> Memo<u64> {
  let first_computes = Rc::clone(computes);
  let first = Memo::new(move || {
    bump(&first_computes);
    source.get() + 1
  });
  (1..length).fold(first, |previous, _| Memo::new(move || previous.get() + 1))
}

// A write at the start of a chain of `length` memos, then a read at its end, which recomputes the
// whole chain; and a read of the chain when nothing changed, which only checks it.
fn memo_chain(c: &mut Criterion) {
  let mut group = c.benchmark_group("memo_chain");
  for length in [10_u64, 100] {
    let computes = counter();
    in_scope(
      || {
        let source = Signal::new(0_u64);
        (source, chain(Memo::new(move || source.get()), length, &computes))
      },
      |(source, last)| {
        let mut value = 0;
        group.bench_function(BenchmarkId::new("changed", length), |b| {
          b.iter(|| {
            value += 1;
            write(source, value);
            last.get()
          });
        });
        assert_eq!(last.get(), value + length);
        let computed = computes.get();
        group.bench_function(BenchmarkId::new("unchanged", length), |b| b.iter(|| last.get()));
        assert_eq!(computes.get(), computed, "an unchanged chain recomputed");
      },
    );
  }
  group.finish();
}

// A write that a memo absorbs: its value stays the same, so the chain of `length` memos after it
// is checked but never recomputed.
fn cut_off(c: &mut Criterion) {
  let mut group = c.benchmark_group("cut_off");
  for length in [10_u64, 100] {
    let computes = counter();
    in_scope(
      || {
        let source = Signal::new(0_u64);
        let parity = Memo::new(move || source.get() % 2);
        (source, chain(parity, length, &computes))
      },
      |(source, last)| {
        let mut value = 0;
        group.bench_function(BenchmarkId::from_parameter(length), |b| {
          b.iter(|| {
            value += 2;
            write(source, value);
            last.get()
          });
        });
        assert_eq!((last.get(), computes.get()), (length, 1), "the chain recomputed");
      },
    );
  }
  group.finish();
}

// A write to a signal two memos read, both read by a third: the third, computed when first read,
// computes once per write.
fn diamond(c: &mut Criterion) {
  let computes = counter();
  in_scope(
    || {
      let source = Signal::new(0_u64);
      let left = Memo::new(move || source.get() + 1);
      let right = Memo::new(move || source.get() * 2);
      let join_computes = Rc::clone(&computes);
      let join = Memo::new(move || {
        bump(&join_computes);
        left.get() + right.get()
      });
      (source, join)
    },
    |(source, join)| {
      let mut value = 0;
      c.bench_function("diamond", |b| {
        b.iter(|| {
          value += 1;
          write(source, value);
          join.get()
        });
      });
      assert_eq!(computes.get(), value, "the join computed other than once per write");
      assert_eq!(join.get(), 3 * value + 1);
    },
  );
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
        group.bench_function(BenchmarkId::from_parameter(count), |b| {
          b.iter(|| {
            value += 1;
            write(&signals[0], value);
            sum.get()
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
  let runs = counter();
  in_scope(
    || {
      let signals: Vec<Signal<u64>> = (0..100).map(|_| Signal::new(0)).collect();
      let read = signals.clone();
      let effect_runs = Rc::clone(&runs);
      effect(move || {
        black_box(read.iter().map(Signal::get).sum::<u64>());
        bump(&effect_runs);
      });
      signals
    },
    |signals| {
      let mut batches = 0;
      c.bench_function("batch/100", |b| {
        b.iter(|| {
          batches += 1;
          batch(|| signals.iter().for_each(|signal| signal.set(batches)));
        });
      });
      assert_eq!(runs.get(), 1 + batches, "the effect ran other than once per batch");
    },
  );
}

// A child scope's whole life: 10 signals, a memo over them, and an effect that reads the memo and
// makes a signal of its own on each run, made, written once (so the effect runs twice), and
// disposed.
fn scope(c: &mut Criterion) {
  let before = live_counts();
  let parent = Scope::new_root();
  let runs = counter();
  let mut scopes = 0;
  c.bench_function("scope/create_dispose", |b| {
    b.iter(|| {
      scopes += 1;
      let scope = parent.child();
      let first = scope.run(|| {
        let signals: Vec<Signal<u64>> = (0..10).map(Signal::new).collect();
        let first = signals[0];
        let sum = Memo::new(move || signals.iter().map(Signal::get).sum::<u64>());
        let effect_runs = Rc::clone(&runs);
        effect(move || {
          black_box(Signal::new(sum.get()));
          bump(&effect_runs);
        });
        first
      });
      write(&first, 1);
      scope.dispose();
    });
  });
  parent.dispose();
  assert_eq!(runs.get(), 2 * scopes, "the effect didn't run again after the write");
  assert_no_leak(before);
}

criterion_group!(
  reactive,
  signal,
  fan_out,
  memo_fan_out,
  memo_chain,
  cut_off,
  diamond,
  tracked_reads,
  batched,
  scope
);
criterion_main!(reactive);
