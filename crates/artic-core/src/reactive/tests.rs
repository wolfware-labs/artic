use crate::reactive::runtime::{Node, Runtime};
use crate::reactive::{
  Change, LiveCounts, Memo, Scope, Signal, assert_no_reactive_leaks, batch, effect, flush_effects, live_counts,
};
use pretty_assertions::assert_eq;
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

/// Creates an observer and runs `action` as that observer, so every signal read inside subscribes it.
fn observe(action: impl FnOnce()) -> Node {
  let observer = Runtime::create_observer();
  Runtime::run_tracked(observer, action);
  observer
}

// Reading a signal outside any observer records no dependency.
#[test]
fn get_outside_observer_records_nothing() {
  let count = Signal::new(0);
  let observer = Runtime::create_observer();

  count.get();
  count.set(1);

  assert!(!Runtime::is_dirty(observer));
}

// Reading a signal inside `run_tracked` records the running observer as a subscriber.
#[test]
fn get_inside_observer_records_dependency() {
  let count = Signal::new(0);
  let observer = observe(|| {
    count.get();
  });

  count.set(1);

  assert!(Runtime::is_dirty(observer));
}

// `set` marks every observer that read the signal as dirty.
#[test]
fn set_marks_dependents_dirty() {
  let count = Signal::new(0);
  let first = observe(|| {
    count.get();
  });
  let second = observe(|| {
    count.get();
  });
  assert!(!Runtime::is_dirty(first));
  assert!(!Runtime::is_dirty(second));

  count.set(1);

  assert!(Runtime::is_dirty(first));
  assert!(Runtime::is_dirty(second));
}

// `set_if_changed` with a value equal to the current one does not mark dependents dirty (`PartialEq`
// types). Plain `set` always propagates: see `set_with_equal_value_still_marks_dependents_dirty`.
#[test]
fn equal_set_does_not_propagate() {
  let count = Signal::new(1);
  let observer = observe(|| {
    count.get();
  });

  count.set_if_changed(1);

  assert!(!Runtime::is_dirty(observer));
}

// `set` marks dependents dirty even when the new value equals the current one.
#[test]
fn set_with_equal_value_still_marks_dependents_dirty() {
  let count = Signal::new(1);
  let observer = observe(|| {
    count.get();
  });

  count.set(1);

  assert!(Runtime::is_dirty(observer));
}

// `set_if_changed` with a different value marks dependents dirty, the same as `set`.
#[test]
fn set_if_changed_with_new_value_marks_dependents_dirty() {
  let count = Signal::new(1);
  let observer = observe(|| {
    count.get();
  });

  count.set_if_changed(2);

  assert!(Runtime::is_dirty(observer));
}

// The comparison inside `set_if_changed` does not subscribe the running observer: it is a write.
#[test]
fn set_if_changed_inside_observer_records_nothing() {
  let count = Signal::new(0);
  let observer = observe(|| count.set_if_changed(1));

  count.set(5);

  assert!(!Runtime::is_dirty(observer));
}

// Tracking and dirty marking

// Reading through `with` records a dependency, the same as `get`.
#[test]
fn with_inside_observer_records_dependency() {
  let name = Signal::new(String::from("Ada"));
  let observer = observe(|| {
    name.with(|name| name.len());
  });

  name.set(String::from("Grace"));

  assert!(Runtime::is_dirty(observer));
}

// `update` marks dependents dirty, the same as `set`.
#[test]
fn update_marks_dependents_dirty() {
  let items = Signal::new(vec![1, 2]);
  let observer = observe(|| {
    items.get();
  });

  items.update(|items| items.push(3));

  assert!(Runtime::is_dirty(observer));
}

// An observer that read a different signal is not marked dirty.
#[test]
fn set_leaves_unrelated_observers_clean() {
  let read = Signal::new(0);
  let written = Signal::new(0);
  let observer = observe(|| {
    read.get();
  });

  written.set(1);

  assert!(!Runtime::is_dirty(observer));
}

// Running an observer again clears its dirty flag, so only writes after the run mark it dirty.
#[test]
fn run_tracked_marks_observer_clean() {
  let count = Signal::new(0);
  let observer = observe(|| {
    count.get();
  });
  count.set(1);

  Runtime::run_tracked(observer, || {
    count.get();
  });

  assert!(!Runtime::is_dirty(observer));
}

// `run_tracked` records every signal the action read as a source, once each and in read order.
#[test]
fn run_tracked_records_each_signal_read_once() {
  let first = Signal::new(0);
  let second = Signal::new(0);
  let observer = Runtime::create_observer();

  Runtime::run_tracked(observer, || {
    first.get();
    second.with(|_| ());
    first.get();
  });

  assert_eq!(Runtime::sources(observer), [Node::from(first.id()), Node::from(second.id())]);
}

// Clearing a node's sources stops writes to them from marking it dirty.
#[test]
fn cleared_sources_stop_marking_dirty() {
  let count = Signal::new(0);
  let observer = Runtime::create_observer();
  Runtime::run_tracked(observer, || {
    count.get();
  });

  Runtime::clear_sources(observer);
  count.set(1);

  assert!(!Runtime::is_dirty(observer));
}

// Panic safety

// If the action passed to `run_tracked` panics, the observer is unsubscribed from what it read before
// the panic, because nothing returns that list to the caller.
#[test]
fn run_tracked_unsubscribes_reads_when_action_panics() {
  let count = Signal::new(0);
  let observer = Runtime::create_observer();

  let result = catch_unwind(AssertUnwindSafe(|| {
    Runtime::run_tracked(observer, || {
      count.get();
      panic!("action failed");
    })
  }));
  count.set(1);

  assert!(result.is_err());
  assert!(!Runtime::is_dirty(observer));
}

// If the action passed to `run_tracked` panics, the observer is popped and the panic still reaches the caller.
#[test]
fn run_tracked_pops_observer_when_action_panics() {
  let count = Signal::new(0);
  let observer = Runtime::create_observer();

  let result = catch_unwind(AssertUnwindSafe(|| {
    Runtime::run_tracked(observer, || panic!("action failed"))
  }));

  let payload = result.expect_err("the panic should reach the caller of run_tracked");
  assert_eq!(payload.downcast_ref::<&str>(), Some(&"action failed"));

  // If the observer were still on the stack, this read would subscribe it.
  count.get();
  count.set(1);
  assert!(!Runtime::is_dirty(observer));
}

// If an `update` closure panics, dependents are still marked dirty before the panic continues.
#[test]
fn panicking_update_still_marks_dependents_dirty() {
  let count = Signal::new(0);
  let observer = observe(|| {
    count.get();
  });

  let result = catch_unwind(AssertUnwindSafe(|| count.update(|_| panic!("update failed"))));

  assert!(result.is_err());
  assert!(Runtime::is_dirty(observer));
}

// Nesting

// When observers nest, a read subscribes only the innermost running observer.
#[test]
fn nested_run_tracked_subscribes_only_innermost_observer() {
  let count = Signal::new(0);
  let outer = Runtime::create_observer();
  let inner = Runtime::create_observer();

  Runtime::run_tracked(outer, || {
    Runtime::run_tracked(inner, || {
      count.get();
    });
  });
  count.set(1);

  assert!(Runtime::is_dirty(inner));
  assert!(!Runtime::is_dirty(outer));
}

// Once a nested observer finishes, reads subscribe the outer observer again.
#[test]
fn outer_observer_is_current_again_after_inner_finishes() {
  let inner_signal = Signal::new(0);
  let outer_signal = Signal::new(0);
  let outer = Runtime::create_observer();
  let inner = Runtime::create_observer();

  Runtime::run_tracked(outer, || {
    Runtime::run_tracked(inner, || {
      inner_signal.get();
    });
    outer_signal.get();
  });
  outer_signal.set(1);

  assert!(Runtime::is_dirty(outer));
  assert!(!Runtime::is_dirty(inner));
}

// If a nested observer panics and the outer one catches it, the outer observer is current again.
#[test]
fn inner_panic_restores_outer_observer() {
  let count = Signal::new(0);
  let outer = Runtime::create_observer();
  let inner = Runtime::create_observer();

  Runtime::run_tracked(outer, || {
    let result = catch_unwind(AssertUnwindSafe(|| Runtime::run_tracked(inner, || panic!("inner failed"))));
    assert!(result.is_err());
    count.get();
  });
  count.set(1);

  assert!(Runtime::is_dirty(outer));
  assert!(!Runtime::is_dirty(inner));
}

// More tracking rules

// An observer that read two signals is marked dirty by a write to either of them.
#[test]
fn observer_of_two_signals_is_dirtied_by_either() {
  for write_first in [true, false] {
    let first = Signal::new(0);
    let second = Signal::new(0);
    let observer = observe(|| {
      first.get();
      second.get();
    });

    if write_first {
      first.set(1)
    } else {
      second.set(1)
    }

    assert!(Runtime::is_dirty(observer), "write_first = {write_first}");
  }
}

// Creating a signal while an observer runs does not subscribe the observer to it.
#[test]
fn creating_signal_inside_observer_records_nothing() {
  let mut created = None;
  let observer = observe(|| created = Some(Signal::new(0)));

  created.expect("the signal should have been created").set(1);

  assert!(!Runtime::is_dirty(observer));
}

// Writing a signal while an observer runs does not subscribe the observer: only reads do.
#[test]
fn update_inside_observer_records_nothing() {
  let count = Signal::new(0);
  let observer = observe(|| count.update(|count| *count += 1));

  count.set(5);

  assert!(!Runtime::is_dirty(observer));
}

// Memo

/// Returns a call counter and a closure that bumps it, to count how often a memo computes.
fn counter() -> (Rc<Cell<u32>>, impl Fn() + 'static) {
  let calls = Rc::new(Cell::new(0));
  let calls_in_closure = Rc::clone(&calls);
  (calls, move || calls_in_closure.set(calls_in_closure.get() + 1))
}

// Creating a memo runs nothing; the first read computes it.
#[test]
fn memo_not_computed_until_read() {
  let (calls, count_call) = counter();
  let source = Signal::new(2);
  let double = Memo::new(move || {
    count_call();
    source.get() * 2
  });
  assert_eq!(calls.get(), 0);

  assert_eq!(double.get(), 4);
  assert_eq!(calls.get(), 1);
}

// After a dependency changes, the memo computes once on the next read and remembers the result.
#[test]
fn memo_recomputes_once_after_dependency_change() {
  let (calls, count_call) = counter();
  let source = Signal::new(2);
  let double = Memo::new(move || {
    count_call();
    source.get() * 2
  });
  double.get();

  source.set(3);
  source.set(4);
  assert_eq!(calls.get(), 1, "a change alone must not compute");

  assert_eq!(double.get(), 8);
  assert_eq!(double.get(), 8);
  assert_eq!(calls.get(), 2);
}

// A dependency read only on a branch that is no longer taken stops causing computes.
#[test]
fn memo_drops_stale_conditional_dependency() {
  let (calls, count_call) = counter();
  let use_detail = Signal::new(true);
  let detail = Signal::new(1);
  let shown = Memo::new(move || {
    count_call();
    if use_detail.get() { detail.get() } else { 0 }
  });
  assert_eq!(shown.get(), 1);
  use_detail.set(false);
  assert_eq!(shown.get(), 0);

  detail.set(2);

  assert_eq!(shown.get(), 0);
  assert_eq!(calls.get(), 2, "changing a dropped dependency must not compute");
}

// A change to a signal reaches the end of a chain of memos.
#[test]
fn memo_chain_propagates() {
  let source = Signal::new(1);
  let double = Memo::new(move || source.get() * 2);
  let label = Memo::new(move || format!("value: {}", double.get()));
  assert_eq!(label.get(), "value: 2");

  source.set(5);

  assert_eq!(label.get(), "value: 10");
}

// A memo that computes a value equal to its last one leaves the memos that read it up to date.
#[test]
fn equal_recompute_does_not_dirty_dependents() {
  let (calls, count_call) = counter();
  let source = Signal::new(2);
  let is_even = Memo::new(move || source.get() % 2 == 0);
  let label = Memo::new(move || {
    count_call();
    if is_even.get() { "even" } else { "odd" }
  });
  assert_eq!(label.get(), "even");

  source.set(4);

  assert_eq!(label.get(), "even");
  assert_eq!(calls.get(), 1, "label must not compute when is_even is unchanged");
}

// A memo read inside a tracked run is recorded once, as the memo.
#[test]
fn run_tracked_records_a_memo_read_as_the_memo() {
  let source = Signal::new(1);
  let double = Memo::new(move || source.get() * 2);
  let observer = Runtime::create_observer();

  Runtime::run_tracked(observer, || {
    double.get();
    double.with(|_| ());
  });

  let sources = Runtime::sources(observer);
  assert_eq!(sources.len(), 1);
  assert!(Runtime::is_memo(sources[0]));
}

// Effect

// With `a -> (b, c) -> effect`, one write to `a` reaches the effect by two paths but runs it once.
#[test]
fn diamond_dependency_runs_effect_once() {
  let (runs, count_run) = counter();
  let a = Signal::new(1);
  let b = Memo::new(move || a.get() + 1);
  let c = Memo::new(move || a.get() * 2);
  effect(move || {
    count_run();
    b.get();
    c.get();
  });

  a.set(2);
  flush_effects();

  assert_eq!(runs.get(), 2, "one run on creation, one for the write");
}

// An effect that reads a signal and a memo derived from it always sees both updated together.
#[test]
fn effect_sees_consistent_values() {
  let a = Signal::new(1);
  let b = Memo::new(move || a.get() + 1);
  let seen = Rc::new(RefCell::new(Vec::new()));
  let seen_in_effect = Rc::clone(&seen);
  effect(move || seen_in_effect.borrow_mut().push((a.get(), b.get())));

  for value in [2, 3, 4] {
    a.set(value);
    flush_effects();
  }

  assert_eq!(*seen.borrow(), [(1, 2), (2, 3), (3, 4), (4, 5)]);
}

// Effects run in the order they were created, and each one sees every memo it reads already
// updated, however long the chain of memos behind it.
#[test]
fn effects_run_in_topological_order() {
  let a = Signal::new(1);
  let b = Memo::new(move || a.get() + 1);
  let c = Memo::new(move || a.get() * 2);
  let deep = Memo::new(move || b.get() + c.get());
  let order = Rc::new(RefCell::new(Vec::new()));
  let order_in_deep = Rc::clone(&order);
  effect(move || order_in_deep.borrow_mut().push(("deep", deep.get())));
  let order_in_shallow = Rc::clone(&order);
  effect(move || order_in_shallow.borrow_mut().push(("shallow", a.get())));
  order.borrow_mut().clear();

  a.set(10);
  flush_effects();

  assert_eq!(*order.borrow(), [("deep", 31), ("shallow", 10)]);
}

// Several writes inside one `batch` rerun an effect once.
#[test]
fn batch_coalesces_sets() {
  let (runs, count_run) = counter();
  let first = Signal::new(1);
  let second = Signal::new(1);
  effect(move || {
    count_run();
    first.get();
    second.get();
  });

  batch(|| {
    first.set(2);
    second.set(2);
    first.set(3);
  });

  assert_eq!(runs.get(), 2, "one run on creation, one for the batch");
}

// A batch inside a batch doesn't flush; the outermost one flushes once when it returns.
#[test]
fn nested_batch_flushes_once() {
  let (runs, count_run) = counter();
  let source = Signal::new(1);
  effect(move || {
    count_run();
    source.get();
  });

  batch(|| {
    batch(|| source.set(2));
    assert_eq!(runs.get(), 1, "the inner batch must not flush");
    source.set(3);
  });

  assert_eq!(runs.get(), 2);
}

// Live counts

// Creating a signal, a memo and an effect raises each count by one. A memo and an effect each also
// add the scope that owns what their closure creates.
#[test]
fn live_counts_track_created_nodes() {
  let before = live_counts();

  let count = Signal::new(0);
  let double = Memo::new(move || count.get() * 2);
  effect(move || {
    double.get();
  });

  assert_eq!(
    live_counts(),
    LiveCounts {
      signals: before.signals + 1,
      memos: before.memos + 1,
      effects: before.effects + 1,
      scopes: before.scopes + 2,
    }
  );
}

// Creating a root scope and a child scope raises the scope count by two.
#[test]
fn live_counts_track_created_scopes() {
  let before = live_counts();

  let root = Scope::new_root();
  root.child();

  assert_eq!(live_counts().scopes, before.scopes + 2);
}

// Scope

// `run` makes the scope active only while its closure runs, and returns the closure's result.
#[test]
fn run_activates_scope_only_inside_closure() {
  let scope = Scope::new_root();

  let active = scope.run(Runtime::active_scope);

  assert_eq!(active, Some(scope.id()));
  assert_eq!(Runtime::active_scope(), None);
}

// A nested `run` activates the inner scope, and the outer one is active again once it returns.
#[test]
fn nested_run_restores_outer_scope() {
  let outer = Scope::new_root();
  let inner = outer.child();

  let (during, after) = outer.run(|| (inner.run(Runtime::active_scope), Runtime::active_scope()));

  assert_eq!(during, Some(inner.id()));
  assert_eq!(after, Some(outer.id()));
}

// If the closure passed to `run` panics, the previous scope is active again and the panic reaches
// the caller.
#[test]
fn run_restores_scope_when_action_panics() {
  let outer = Scope::new_root();
  let inner = outer.child();

  let after = outer.run(|| {
    let result = catch_unwind(AssertUnwindSafe(|| inner.run(|| panic!("boom"))));
    assert!(result.is_err());
    Runtime::active_scope()
  });

  assert_eq!(after, Some(outer.id()));
}

// Dispose

/// Returns a run counter and an effect body that bumps it and reads `source`.
fn counting_reader(source: Signal<i32>) -> (Rc<Cell<u32>>, impl FnMut() + 'static) {
  let runs = Rc::new(Cell::new(0));
  let runs_in_effect = Rc::clone(&runs);
  (runs, move || {
    runs_in_effect.set(runs_in_effect.get() + 1);
    source.get();
  })
}

// Disposing a scope frees every signal, memo and effect created in it, and the scope itself.
#[test]
fn dispose_frees_all_owned_nodes() {
  let before = live_counts();
  let scope = Scope::new_root();

  scope.run(|| {
    let count = Signal::new(1);
    let double = Memo::new(move || count.get() * 2);
    effect(move || {
      double.get();
    });
  });
  scope.dispose();

  assert_no_reactive_leaks(before);
}

// Disposing a parent scope disposes its child scopes and what they own.
#[test]
fn dispose_parent_disposes_children() {
  let before = live_counts();
  let parent = Scope::new_root();
  let child = parent.child();
  child.child().run(|| Signal::new(0));
  child.run(|| Signal::new(0));

  parent.dispose();

  assert_no_reactive_leaks(before);
}

// An effect whose scope was disposed doesn't run again when a signal it read changes.
#[test]
fn disposed_effect_no_longer_runs() {
  let source = Signal::new(0);
  let (runs, read) = counting_reader(source);
  let scope = Scope::new_root();
  scope.run(|| effect(read));

  scope.dispose();
  source.set(1);
  flush_effects();

  assert_eq!(runs.get(), 1, "only the run on creation");
}

// A signal in a parent scope stops listing an effect from a disposed child scope as a subscriber.
#[test]
fn signal_outlives_dependent_effect_scope() {
  let parent = Scope::new_root();
  let child = parent.child();
  let theme = parent.run(|| Signal::new(0));
  let (_, read) = counting_reader(theme);
  child.run(|| effect(read));
  assert_eq!(Runtime::subscriber_count(theme.id()), 1);

  child.dispose();

  assert_eq!(Runtime::subscriber_count(theme.id()), 0);
}

// Disposing a child scope removes it from its parent, so disposing the parent later doesn't
// dispose it twice.
#[test]
fn disposed_child_is_removed_from_parent() {
  let before = live_counts();
  let parent = Scope::new_root();
  let child = parent.child();

  child.dispose();
  parent.dispose();

  assert_no_reactive_leaks(before);
}

// An effect that outlives a memo it read no longer refreshes it, and unsubscribing from it on the
// next run does nothing.
#[test]
fn reader_skips_a_disposed_memo_it_read_before() {
  let source = Signal::new(1);
  let parent = Scope::new_root();
  let child = parent.child();
  let short_lived = child.run(|| Memo::new(move || source.get()));
  let long_lived = Memo::new(move || source.get() * 10);
  let reads_short_lived = Rc::new(Cell::new(true));
  let reads_in_effect = Rc::clone(&reads_short_lived);
  let seen = Rc::new(Cell::new(0));
  let seen_in_effect = Rc::clone(&seen);
  parent.run(|| {
    effect(move || {
      if reads_in_effect.get() {
        short_lived.get();
      }
      seen_in_effect.set(long_lived.get());
    })
  });

  child.dispose();
  reads_short_lived.set(false);
  source.set(2);
  flush_effects();

  assert_eq!(seen.get(), 20);
}

// An effect that disposes an earlier effect's scope during a flush doesn't make the flush skip the
// effects created after it.
#[test]
fn disposing_during_flush_keeps_later_effects_running() {
  let source = Signal::new(0);
  let first_scope = Scope::new_root();
  let (_, first) = counting_reader(source);
  first_scope.run(|| effect(first));
  effect(move || {
    if source.get() > 0 {
      first_scope.dispose();
    }
  });
  let (last_runs, last) = counting_reader(source);
  effect(last);

  source.set(1);
  flush_effects();

  assert_eq!(last_runs.get(), 2);
}

// An effect that disposes its own scope while running is freed, and its last reads don't leave it
// subscribed.
#[test]
fn effect_can_dispose_its_own_scope() {
  let before = live_counts();
  let source = Signal::new(0);
  let scope = Scope::new_root();
  scope.run(|| {
    effect(move || {
      if source.get() > 0 {
        scope.dispose();
      }
    })
  });

  source.set(1);
  flush_effects();

  assert_eq!(Runtime::subscriber_count(source.id()), 0);
  assert_eq!(live_counts().effects, before.effects);
}

// Inner scopes

// An effect that creates an effect on every run disposes the one from its previous run first, so
// only the latest inner effect is alive and runs.
#[test]
fn effect_disposes_inner_effects_before_rerun() {
  let source = Signal::new(0);
  let inner_runs = Rc::new(Cell::new(0));
  let inner_runs_in_effect = Rc::clone(&inner_runs);
  effect(move || {
    source.get();
    let inner_runs = Rc::clone(&inner_runs_in_effect);
    effect(move || {
      inner_runs.set(inner_runs.get() + 1);
      source.get();
    });
  });
  let after_creation = live_counts();

  source.set(1);
  flush_effects();
  source.set(2);
  flush_effects();

  assert_eq!(live_counts(), after_creation);
  assert_eq!(inner_runs.get(), 3, "one run per inner effect, each created once");
}

// A memo that creates a signal while computing disposes the one from its previous compute first.
#[test]
fn memo_disposes_inner_nodes_before_recompute() {
  let source = Signal::new(0);
  let memo = Memo::new(move || Signal::new(source.get()).get());
  memo.get();
  let after_first_compute = live_counts();

  source.set(1);
  memo.get();

  assert_eq!(live_counts(), after_first_compute);
}

// Disposing a scope disposes what an effect created in it built during its runs.
#[test]
fn dispose_frees_nodes_created_by_an_owned_effect() {
  let before = live_counts();
  let scope = Scope::new_root();
  scope.run(|| {
    effect(|| {
      Signal::new(0);
      effect(|| {});
    })
  });

  scope.dispose();

  assert_no_reactive_leaks(before);
}

// Disposed handles

/// Runs `action`, expects it to panic, and returns the panic message.
fn panic_message(action: impl FnOnce()) -> String {
  let payload = catch_unwind(AssertUnwindSafe(action)).expect_err("the action should panic");
  payload
    .downcast_ref::<String>()
    .cloned()
    .or_else(|| payload.downcast_ref::<&str>().map(|message| message.to_string()))
    .expect("the panic message should be a string")
}

// Reading a signal whose scope was disposed panics, and in debug builds the message says where the
// signal was created.
#[test]
fn reading_disposed_signal_panics_with_creation_location() {
  let scope = Scope::new_root();
  let count = scope.run(|| Signal::new(0));
  scope.dispose();

  let message = panic_message(|| {
    count.get();
  });

  assert!(message.starts_with("signal used after it was disposed"), "{message}");
  if cfg!(debug_assertions) {
    assert!(message.contains(&format!("(created at {}:", file!())), "{message}");
  }
}

// Writing a signal whose scope was disposed panics, even after its slot was reused by a new signal.
#[test]
fn writing_disposed_signal_panics_after_slot_reuse() {
  let scope = Scope::new_root();
  let old = scope.run(|| Signal::new(0));
  scope.dispose();
  let new = Signal::new(0);

  let message = panic_message(|| old.set(1));

  assert!(message.starts_with("signal used after it was disposed"), "{message}");
  assert_eq!(new.get(), 0);
}

// Reading a memo whose scope was disposed panics.
#[test]
fn reading_disposed_memo_panics() {
  let scope = Scope::new_root();
  let memo = scope.run(|| Memo::new(|| 1));
  scope.dispose();

  let message = panic_message(|| {
    memo.get();
  });

  assert!(message.starts_with("memo used after its scope was disposed"), "{message}");
}

// Running a disposed scope panics instead of leaving what the closure creates without an owner.
#[test]
fn running_disposed_scope_panics() {
  let scope = Scope::new_root();
  scope.dispose();

  let message = panic_message(|| scope.run(|| {}));

  assert!(message.starts_with("scope used after it was disposed"), "{message}");
}

// Disposing a scope twice does nothing the second time, also when its parent disposed it first.
#[test]
fn dispose_twice_is_a_no_op() {
  let before = live_counts();
  let parent = Scope::new_root();
  let child = parent.child();

  parent.dispose();
  child.dispose();
  parent.dispose();

  assert_no_reactive_leaks(before);
}

// A memo that disposes its own scope while computing is freed and unsubscribed from what it read,
// and the read that triggered the compute reports the memo as disposed.
#[test]
fn memo_can_dispose_its_own_scope() {
  let before = live_counts();
  let source = Signal::new(0);
  let scope = Scope::new_root();
  let memo = scope.run(|| {
    Memo::new(move || {
      if source.get() > 0 {
        scope.dispose();
      }
    })
  });
  memo.get();

  source.set(1);
  let message = panic_message(|| memo.get());

  assert!(message.starts_with("memo used after its scope was disposed"), "{message}");
  assert_eq!(Runtime::subscriber_count(source.id()), 0);
  assert_eq!(
    live_counts(),
    LiveCounts {
      signals: before.signals + 1,
      ..before
    }
  );
}

// Leak check

// `assert_no_reactive_leaks` names each count that differs from the baseline.
#[test]
fn leak_check_names_the_counts_that_changed() {
  let baseline = live_counts();
  Scope::new_root().run(|| Signal::new(0));

  let message = panic_message(|| assert_no_reactive_leaks(baseline));

  assert_eq!(
    message,
    format!(
      "reactive nodes leaked: signals {} -> {}, scopes {} -> {}",
      baseline.signals,
      baseline.signals + 1,
      baseline.scopes,
      baseline.scopes + 1
    )
  );
}

// update_with

// An edit reported as visible marks subscribers dirty and makes the next flush rerun them.
#[test]
fn visible_update_with_marks_dependents_dirty() {
  let items = Signal::new(vec![1]);
  let observer = observe(|| {
    items.with(|_| {});
  });

  items.update_with(|items| {
    items.push(2);
    Change::Visible
  });

  assert!(Runtime::is_dirty(observer));
}

// An edit reported as hidden is kept but marks nobody dirty and records no write, so a flush after
// it reruns nothing.
#[test]
fn hidden_update_with_keeps_the_edit_and_wakes_nobody() {
  let items = Signal::new(vec![1]);
  let runs = Rc::new(Cell::new(0));
  let runs_in_effect = Rc::clone(&runs);
  effect(move || {
    runs_in_effect.set(runs_in_effect.get() + 1);
    items.with(|_| {});
  });

  items.update_with(|items| {
    items[0] = 5;
    Change::Hidden
  });
  flush_effects();

  assert_eq!(items.get(), vec![5]);
  assert_eq!(runs.get(), 1, "only the run on creation");
}

// If the closure passed to `update_with` panics, dependents are marked dirty as after a visible
// change, and the panic reaches the caller.
#[test]
fn panicking_update_with_still_marks_dependents_dirty() {
  let count = Signal::new(0);
  let observer = observe(|| {
    count.get();
  });

  let result = catch_unwind(AssertUnwindSafe(|| count.update_with(|_| panic!("boom"))));

  assert!(result.is_err());
  assert!(Runtime::is_dirty(observer));
}

// A memo whose computation panics computes again on the next read, as its docs promise.
#[test]
fn a_panicking_memo_computes_again_on_the_next_read() {
  let fails = Rc::new(Cell::new(true));
  let compute_fails = Rc::clone(&fails);
  let memo = Memo::new(move || {
    assert!(!compute_fails.get(), "compute failed");
    7
  });

  let first = catch_unwind(AssertUnwindSafe(|| memo.get()));
  fails.set(false);

  assert!(first.is_err());
  assert_eq!(memo.get(), 7);
}

// An effect that panics during a flush runs again on the next flush, with the value it missed.
#[test]
fn a_panicking_effect_runs_again_on_the_next_flush() {
  let count = Signal::new(0);
  let fails = Rc::new(Cell::new(false));
  let seen = Rc::new(Cell::new(0));
  let (run_fails, run_seen) = (Rc::clone(&fails), Rc::clone(&seen));
  effect(move || {
    let value = count.get();
    assert!(!run_fails.get(), "effect failed");
    run_seen.set(value);
  });

  fails.set(true);
  count.set(1);
  let first = catch_unwind(flush_effects);
  fails.set(false);
  flush_effects();

  assert!(first.is_err());
  assert_eq!(seen.get(), 1);
}

// A signal disposed on its own leaves its scope's list at once, so a scope never keeps a dead
// node until it is disposed.
#[test]
fn a_disposed_signal_leaves_its_scope() {
  let scope = Scope::new_root();
  let [first, second, third] = scope.run(|| [Signal::new(1), Signal::new(2), Signal::new(3)]);

  second.dispose();
  let middle_gone = Runtime::owned_count(scope.id());
  third.dispose();
  first.dispose();

  assert_eq!((middle_gone, Runtime::owned_count(scope.id())), (2, 0));
  scope.dispose();
}

// Reading a memo chain again when nothing changed checks nothing below it, and after a write each
// memo checks its one source once, nine checks for ten memos, however the reads nest.
#[test]
fn an_unchanged_chain_is_not_walked_again() {
  let source = Signal::new(0);
  let first = Memo::new(move || source.get() + 1);
  let last = (1..10).fold(first, |previous, _| Memo::new(move || previous.get() + 1));
  last.get();

  let before = Runtime::source_checks();
  last.get();
  let unchanged = Runtime::source_checks() - before;
  source.set(1);
  let before = Runtime::source_checks();
  let value = last.get();

  assert_eq!((unchanged, Runtime::source_checks() - before, value), (0, 9, 11));
}

// A chain checked after a write its first memo absorbed, so nothing recomputed, isn't checked
// again on the next read.
#[test]
fn a_checked_chain_is_not_checked_again() {
  let source = Signal::new(0);
  let parity = Memo::new(move || source.get() % 2);
  let last = (0..10).fold(parity, |previous, _| Memo::new(move || previous.get() + 1));
  last.get();

  source.set(2);
  last.get();
  let before = Runtime::source_checks();
  let value = last.get();

  assert_eq!((Runtime::source_checks() - before, value), (0, 10));
}

// A memo whose computation writes a signal one of its sources reads is checked again on the next
// read, so it never keeps a value computed from the source's old value.
#[test]
fn a_write_during_a_computation_is_seen_on_the_next_read() {
  let count = Signal::new(0);
  let tenfold = Memo::new(move || count.get() * 10);
  let seen = Memo::new(move || {
    let value = tenfold.get();
    if value == 0 {
      count.set(1);
    }
    value
  });

  assert_eq!((seen.get(), seen.get()), (0, 10));
}

/// Logs its name when dropped, to show the order a scope frees what it owns.
struct Dropped(&'static str, Rc<RefCell<Vec<&'static str>>>);

impl Drop for Dropped {
  fn drop(&mut self) {
    self.1.borrow_mut().push(self.0);
  }
}

// A disposed scope frees its effects, then its memos, then its signals, whatever order they were
// made in, as `Scope::dispose` says.
#[test]
fn a_scope_frees_effects_then_memos_then_signals() {
  let log = Rc::new(RefCell::new(Vec::new()));
  let scope = Scope::new_root();
  scope.run(|| {
    Signal::new(Dropped("signal", Rc::clone(&log)));
    let held = Dropped("memo", Rc::clone(&log));
    Memo::new(move || {
      let held = &held;
      held.0.len()
    });
    let held = Dropped("effect", Rc::clone(&log));
    effect(move || {
      let held = &held;
      assert!(!held.0.is_empty());
    });
  });
  let before = log.borrow().len();

  scope.dispose();

  assert_eq!((before, log.borrow().clone()), (0, vec!["effect", "memo", "signal"]));
}

// A child scope disposed on its own leaves its parent's list at once.
#[test]
fn a_disposed_child_scope_leaves_its_parent() {
  let parent = Scope::new_root();
  let [first, _second] = [parent.child(), parent.child()];

  first.dispose();

  assert_eq!(Runtime::child_count(parent.id()), 1);
  parent.dispose();
}

/// Reads its signal when dropped, as a value whose owner tears something reactive down might.
#[derive(PartialEq)]
struct ReadsOnDrop(Signal<i32>);

impl Drop for ReadsOnDrop {
  fn drop(&mut self) {
    self.0.get();
  }
}

// Values freed with a scope's nodes, an effect's captures, a memo's value and a signal's value, may
// use the runtime when they're dropped.
#[test]
fn values_freed_with_a_scope_may_use_the_runtime() {
  let outside = Signal::new(1);
  let scope = Scope::new_root();
  scope.run(|| {
    let reads = ReadsOnDrop(outside);
    effect(move || {
      let reads = &reads;
      reads.0.get();
    });
    let memo = Memo::new(move || Rc::new(ReadsOnDrop(outside)));
    memo.with(|_| {});
    Signal::new(ReadsOnDrop(outside));
  });

  scope.dispose();

  assert_eq!(live_counts().effects, 0);
}

// An effect pending when an earlier effect disposes its scope leaves the pending list even if a
// value freed with it panics, so the next flush runs the effects after it.
#[test]
fn a_disposed_effect_leaves_the_pending_list_even_if_a_drop_panics() {
  struct PanicsOnDrop;
  impl Drop for PanicsOnDrop {
    fn drop(&mut self) {
      panic!("drop failed");
    }
  }
  let count = Signal::new(0);
  let inner = Scope::new_root();
  effect(move || {
    if count.get() == 1 {
      inner.dispose();
    }
  });
  inner.run(|| {
    let held = PanicsOnDrop;
    effect(move || {
      let held = &held;
      let _ = (held, count.get());
    });
  });
  let seen = Rc::new(Cell::new(0));
  let seen_after = Rc::clone(&seen);
  effect(move || seen_after.set(count.get()));
  count.set(1);

  let first = catch_unwind(flush_effects);
  flush_effects();

  assert!(first.is_err());
  assert_eq!(seen.get(), 1);
}

// A write refused because the signal is being read changes nothing, so its readers stay clean.
#[test]
fn a_refused_write_marks_no_reader_dirty() {
  let count = Signal::new(0);
  let observer = observe(|| {
    count.get();
  });

  let refused = count.with(|_| catch_unwind(AssertUnwindSafe(|| count.set(1))));
  let refused_update = count.with(|_| catch_unwind(AssertUnwindSafe(|| count.update_with(|_| Change::Visible))));

  assert!(refused.is_err() && refused_update.is_err());
  assert!(!Runtime::is_dirty(observer));
}

// A memo first computed from a Drop while another panic unwinds is a normal run: it keeps the
// signal it read as a source and stays clean, so it isn't left to compute again on every read.
#[test]
fn a_memo_computed_during_another_panic_keeps_its_sources() {
  struct ReadsOnDrop(Memo<i32>);
  impl Drop for ReadsOnDrop {
    fn drop(&mut self) {
      self.0.get();
    }
  }
  let count = Signal::new(1);
  let computes = Rc::new(Cell::new(0));
  let memo_computes = Rc::clone(&computes);
  let doubled = Memo::new(move || {
    memo_computes.set(memo_computes.get() + 1);
    count.get() * 2
  });

  let unwound = catch_unwind(AssertUnwindSafe(|| {
    let _reader = ReadsOnDrop(doubled);
    panic!("unrelated");
  }));

  doubled.get();

  assert!(unwound.is_err());
  assert_eq!((Runtime::subscriber_count(count.id()), computes.get()), (1, 1));
}

// An effect checked through a memo that panics runs again on the next flush, though the flush that
// panicked already used up the write that reached it.
#[test]
fn an_effect_whose_memo_panics_while_checked_runs_on_the_next_flush() {
  let count = Signal::new(0);
  let fails = Rc::new(Cell::new(false));
  let compute_fails = Rc::clone(&fails);
  let checked = Memo::new(move || {
    let value = count.get();
    assert!(!compute_fails.get(), "compute failed");
    value
  });
  let seen = Rc::new(Cell::new(0));
  let effect_seen = Rc::clone(&seen);
  effect(move || effect_seen.set(checked.get()));

  fails.set(true);
  count.set(1);
  let first = catch_unwind(flush_effects);
  fails.set(false);
  flush_effects();

  assert!(first.is_err());
  assert_eq!(seen.get(), 1);
}

// A source that disposes itself while it is checked doesn't hide the source after it: the reader
// still sees that one change.
#[test]
fn a_source_disposed_while_checked_hides_no_other_source() {
  let count = Signal::new(0);
  let owner = Scope::new_root();
  let leaving = owner.run(|| {
    Memo::new(move || {
      let value = count.get();
      if value == 1 {
        owner.dispose();
      }
      value
    })
  });
  let tenfold = Memo::new(move || count.get() * 10);
  let read_leaving = Cell::new(false);
  let reader = Memo::new(move || {
    if !read_leaving.replace(true) {
      leaving.get();
    }
    tenfold.get()
  });
  reader.get();

  count.set(1);

  assert_eq!(reader.get(), 10);
}

// An effect whose source disposes it while the effect is checked stops there, and the flush goes on.
#[test]
fn an_effect_disposed_by_its_source_while_checked_stops() {
  let count = Signal::new(0);
  let owner = Scope::new_root();
  let disposing = Memo::new(move || {
    let value = count.get();
    if value == 1 {
      owner.dispose();
    }
    value
  });
  let runs = Rc::new(Cell::new(0));
  let effect_runs = Rc::clone(&runs);
  owner.run(|| {
    effect(move || {
      disposing.get();
      effect_runs.set(effect_runs.get() + 1);
    });
  });

  count.set(1);
  flush_effects();

  assert_eq!((runs.get(), live_counts().effects), (1, 0));
}

// A memo whose source disposes it while it is checked panics as any read of a disposed memo does.
#[test]
fn a_memo_disposed_by_its_source_while_checked_panics_as_disposed() {
  let count = Signal::new(0);
  let owner = Scope::new_root();
  let disposing = Memo::new(move || {
    let value = count.get();
    if value == 1 {
      owner.dispose();
    }
    value
  });
  let reader = owner.run(|| Memo::new(move || disposing.get()));
  reader.get();
  count.set(1);

  let message = panic_message(|| {
    reader.get();
  });

  assert!(message.starts_with("memo used after its scope was disposed"), "{message}");
}

// A signal a Drop makes while its scope is disposed from inside its own run is freed by the same
// disposal, so nothing made during it outlives the scope.
#[test]
fn a_node_made_while_its_scope_is_cleared_is_freed_with_it() {
  struct MakesOnDrop(Rc<Cell<Option<Signal<i32>>>>);
  impl Drop for MakesOnDrop {
    fn drop(&mut self) {
      self.0.set(Some(Signal::new(0)));
    }
  }
  let before = live_counts();
  let made = Rc::new(Cell::new(None));
  let scope = Scope::new_root();
  let held = MakesOnDrop(Rc::clone(&made));
  scope.run(|| {
    Signal::new(held);
    scope.dispose();
  });
  let late = made.take().expect("the drop made a signal");

  assert_eq!((late.is_alive(), live_counts()), (false, before));
}

/// Disposes its scope when dropped, as a value whose owner tears its view down might.
struct DisposesOnDrop(Scope);

impl Drop for DisposesOnDrop {
  fn drop(&mut self) {
    self.0.dispose();
  }
}

// A value freed with a child scope may dispose the parent being disposed: the parent's disposal
// finishes without reaching for the scope already freed.
#[test]
fn a_scope_disposed_again_by_a_value_it_frees_is_freed_once() {
  let before = live_counts();
  let parent = Scope::new_root();
  let child = parent.child();
  child.run(|| Signal::new(DisposesOnDrop(parent)));

  parent.dispose();

  assert_eq!(live_counts(), before);
}

// An effect disposed by a value its last run made, freed as it runs again, stops there.
#[test]
fn an_effect_disposed_while_its_last_run_is_cleared_stops() {
  let before = live_counts();
  let count = Signal::new(0);
  let owner = Scope::new_root();
  owner.run(|| {
    effect(move || {
      count.get();
      Signal::new(DisposesOnDrop(owner));
    });
  });

  count.set(1);
  flush_effects();

  assert_eq!(live_counts().effects, before.effects);
}

// A node made in a scope already disposed from inside its own run has no owner to join: it lives
// until disposed on its own.
#[test]
fn a_node_made_in_a_scope_disposed_during_its_run_has_no_owner() {
  let scope = Scope::new_root();

  let (late, memo) = scope.run(|| {
    scope.dispose();
    let late = Signal::new(1);
    (late, Memo::new(move || late.get() + 1))
  });

  assert_eq!(memo.get(), 2);
  late.dispose();
}

// Pending effects

// A write pends only the effect it reaches, not every effect.
#[test]
fn unrelated_effects_are_not_visited() {
  let first = Signal::new(0);
  let second = Signal::new(0);
  effect(move || {
    first.get();
  });
  effect(move || {
    second.get();
  });

  first.set(1);

  assert_eq!(Runtime::collect_pending_effects(), 1);
}

// A write reaches an effect behind a chain of memos, and the effect then sees the end of the chain.
#[test]
fn effect_behind_a_memo_chain_is_pending() {
  let source = Signal::new(1);
  let first = Memo::new(move || source.get() + 1);
  let second = Memo::new(move || first.get() * 2);
  let third = Memo::new(move || second.get() - 1);
  let seen = Rc::new(Cell::new(0));
  let seen_in_effect = Rc::clone(&seen);
  effect(move || seen_in_effect.set(third.get()));

  source.set(4);
  assert_eq!(Runtime::collect_pending_effects(), 1);
  flush_effects();

  assert_eq!(seen.get(), 9);
}

// An effect reached through a memo whose value doesn't change is pending but doesn't run.
#[test]
fn cutoff_still_skips_the_run() {
  let source = Signal::new(2);
  let is_even = Memo::new(move || source.get() % 2 == 0);
  let (runs, read) = counter();
  effect(move || {
    read();
    is_even.get();
  });

  source.set(4);
  assert_eq!(Runtime::collect_pending_effects(), 1);
  flush_effects();

  assert_eq!(runs.get(), 1, "only the run on creation");
}

// With nothing written since the last flush, no effect is pending.
#[test]
fn nothing_written_pends_nothing() {
  let source = Signal::new(0);
  effect(move || {
    source.get();
  });

  assert_eq!(Runtime::collect_pending_effects(), 0);
}

// Effects that each write the next one's input, in creation order, settle in one pass: an effect
// made pending during a pass runs in that pass when it comes later in creation order, so a long
// chain doesn't hit the pass limit.
#[test]
fn effect_chain_in_creation_order_settles_in_one_pass() {
  let signals: Vec<Signal<i32>> = (0..150).map(|_| Signal::new(0)).collect();
  for pair in signals.windows(2) {
    let (input, output) = (pair[0], pair[1]);
    effect(move || output.set(input.get()));
  }
  flush_effects();

  signals[0].set(7);
  flush_effects();

  assert_eq!(signals[149].get(), 7);
}
