//! The UI thread's task queue.
//!
//! Everything that touches the UI runs as a task on the UI thread: an input handler, a binding
//! update, the continuation of background work. [`Dispatcher::post_local`] queues a task and
//! [`Dispatcher::run_pending`] runs the queue in order, flushing effects after each task, so every
//! task is one flush point and its writes reach effects once.
//!
//! ```
//! use artic_core::dispatcher::Dispatcher;
//! use artic_core::reactive::{Signal, effect};
//! use std::cell::Cell;
//! use std::rc::Rc;
//!
//! let count = Signal::new(0);
//! let runs = Rc::new(Cell::new(0));
//! let runs_in_effect = Rc::clone(&runs);
//! effect(move || {
//!   count.get();
//!   runs_in_effect.set(runs_in_effect.get() + 1);
//! });
//!
//! Dispatcher::post_local(move || {
//!   count.set(1);
//!   count.set(2);
//! });
//! Dispatcher::run_pending();
//! assert_eq!(runs.get(), 2, "one run on creation, one for the whole task");
//! ```
//!
//! The queue belongs to the thread that posts to it. Posting from another thread with `post_local`
//! fills that thread's own queue, which the UI never runs; work from other threads will reach the
//! UI through a separate, thread-safe `post`.

use crate::defer::defer;
use crate::reactive::flush_effects;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::marker::PhantomData;

type Task = Box<dyn FnOnce()>;

struct Queue {
  tasks: RefCell<VecDeque<Task>>,
  is_running: Cell<bool>,
}

thread_local! {
  static QUEUE: Queue = const {
    Queue {
      tasks: RefCell::new(VecDeque::new()),
      is_running: Cell::new(false),
    }
  };
}

/// The queue of work waiting to run on the UI thread.
///
/// `Dispatcher` has no instances: its functions work on a queue that belongs to the current thread,
/// so any code on the UI thread can post work, including code running inside a task.
#[derive(Debug)]
pub struct Dispatcher(PhantomData<*const ()>);

impl Dispatcher {
  /// Adds `task` to the end of the current thread's queue.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::dispatcher::Dispatcher;
  ///
  /// assert!(Dispatcher::is_idle());
  /// Dispatcher::post_local(|| println!("later, on the UI thread"));
  /// assert!(!Dispatcher::is_idle());
  /// ```
  pub fn post_local(task: impl FnOnce() + 'static) {
    QUEUE.with(|queue| queue.tasks.borrow_mut().push_back(Box::new(task)));
  }

  /// Runs the current thread's tasks in the order they were posted, flushing effects after each one,
  /// until the queue is empty.
  ///
  /// Each task is a flush point: the writes it makes reach effects once, when it returns, as if it
  /// ran inside [`batch`](crate::reactive::batch). A task posted while the queue runs joins the end
  /// of the queue and runs in the same call, so a task that always posts itself again keeps this
  /// call from returning.
  ///
  /// Calling `run_pending` from inside a task does nothing: the call already running drains the
  /// queue.
  ///
  /// # Panics
  ///
  /// If a task panics, the panic continues to the caller without flushing effects, and the tasks
  /// after it stay queued. The dispatcher stays usable: the next call runs them, and its flushes
  /// deliver the panicked task's writes.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_core::dispatcher::Dispatcher;
  /// use std::cell::RefCell;
  /// use std::rc::Rc;
  ///
  /// let log = Rc::new(RefCell::new(Vec::new()));
  /// let first = Rc::clone(&log);
  /// let second = Rc::clone(&log);
  /// Dispatcher::post_local(move || first.borrow_mut().push("first"));
  /// Dispatcher::post_local(move || second.borrow_mut().push("second"));
  ///
  /// Dispatcher::run_pending();
  /// assert_eq!(*log.borrow(), ["first", "second"]);
  /// assert!(Dispatcher::is_idle());
  /// ```
  pub fn run_pending() {
    if QUEUE.with(|queue| queue.is_running.replace(true)) {
      return;
    }
    let _running = defer(|| QUEUE.with(|queue| queue.is_running.set(false)));
    while let Some(task) = QUEUE.with(|queue| queue.tasks.borrow_mut().pop_front()) {
      task();
      flush_effects();
    }
  }

  /// Returns `true` if the current thread's queue holds no task.
  #[must_use]
  pub fn is_idle() -> bool {
    QUEUE.with(|queue| queue.tasks.borrow().is_empty())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::reactive::{Signal, effect};
  use pretty_assertions::assert_eq;
  use static_assertions::assert_not_impl_any;
  use std::panic::catch_unwind;
  use std::rc::Rc;

  assert_not_impl_any!(Dispatcher: Send, Sync);

  // Posting a task makes the queue busy.
  #[test]
  fn posting_makes_the_queue_busy() {
    assert!(Dispatcher::is_idle());

    Dispatcher::post_local(|| {});

    assert!(!Dispatcher::is_idle());
  }

  type Log = Rc<RefCell<Vec<&'static str>>>;

  /// Returns a task that appends `entry` to `log`.
  fn append(log: &Log, entry: &'static str) -> impl FnOnce() + 'static {
    let log = Rc::clone(log);
    move || log.borrow_mut().push(entry)
  }

  // Tasks run in the order they were posted, and the queue is idle afterwards.
  #[test]
  fn tasks_run_in_fifo_order() {
    let log = Log::default();
    Dispatcher::post_local(append(&log, "a"));
    Dispatcher::post_local(append(&log, "b"));
    Dispatcher::post_local(append(&log, "c"));

    Dispatcher::run_pending();

    assert_eq!(*log.borrow(), ["a", "b", "c"]);
    assert!(Dispatcher::is_idle());
  }

  // A task posted by a running task runs in the same call, after the tasks already queued.
  #[test]
  fn task_posted_during_run_executes() {
    let log = Log::default();
    let posted = append(&log, "posted");
    let first = append(&log, "first");
    Dispatcher::post_local(move || {
      first();
      Dispatcher::post_local(posted);
    });
    Dispatcher::post_local(append(&log, "second"));

    Dispatcher::run_pending();

    assert_eq!(*log.borrow(), ["first", "second", "posted"]);
  }

  // Effects flush once after each task: two writes in one task run an effect once, and a second task
  // runs it again.
  #[test]
  fn effects_flush_after_each_task() {
    let name = Signal::new("");
    let seen = Rc::new(RefCell::new(Vec::new()));
    let seen_in_effect = Rc::clone(&seen);
    effect(move || seen_in_effect.borrow_mut().push(name.get()));
    Dispatcher::post_local(move || {
      name.set("Ada");
      name.set("Ada L.");
    });
    Dispatcher::post_local(move || name.set("Grace"));

    Dispatcher::run_pending();

    assert_eq!(*seen.borrow(), ["", "Ada L.", "Grace"]);
  }

  // Calling `run_pending` from inside a task does nothing: the task finishes before the next one
  // starts.
  #[test]
  fn nested_run_pending_does_nothing() {
    let log = Log::default();
    let (start, end) = (append(&log, "first starts"), append(&log, "first ends"));
    Dispatcher::post_local(move || {
      start();
      Dispatcher::run_pending();
      end();
    });
    Dispatcher::post_local(append(&log, "second"));

    Dispatcher::run_pending();

    assert_eq!(*log.borrow(), ["first starts", "first ends", "second"]);
  }

  // A panicking task stops the call without flushing, leaves the later tasks queued, and the next
  // call runs them and flushes the panicked task's writes.
  #[test]
  fn panicking_task_leaves_the_dispatcher_usable() {
    let count = Signal::new(0);
    let seen = Rc::new(Cell::new(0));
    let seen_in_effect = Rc::clone(&seen);
    effect(move || seen_in_effect.set(count.get()));
    let log = Log::default();
    Dispatcher::post_local(move || {
      count.set(1);
      panic!("task failed");
    });
    Dispatcher::post_local(append(&log, "after"));

    let result = catch_unwind(Dispatcher::run_pending);
    assert!(result.is_err());
    assert_eq!(seen.get(), 0, "no flush after a panicking task");
    assert!(!Dispatcher::is_idle());

    Dispatcher::run_pending();
    assert_eq!(*log.borrow(), ["after"]);
    assert_eq!(seen.get(), 1);
  }
}
