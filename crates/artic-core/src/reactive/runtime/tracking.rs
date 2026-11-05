use crate::defer::defer_unwind;
use crate::reactive::runtime::{Node, RUNTIME, Runtime, SignalId, ValueCell};
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::{BuildHasherDefault, DefaultHasher};
use std::rc::Rc;

pub struct Tracker {
  running: RefCell<Vec<Node>>,
  written: RefCell<HashSet<SignalId, BuildHasherDefault<DefaultHasher>>>,
}

impl Tracker {
  pub const fn new() -> Self {
    Self {
      running: RefCell::new(Vec::new()),
      written: RefCell::new(HashSet::with_hasher(BuildHasherDefault::new())),
    }
  }

  fn push(&self, node: Node) {
    self.running.borrow_mut().push(node);
  }

  fn pop(&self) {
    self.running.borrow_mut().pop();
  }

  fn current(&self) -> Option<Node> {
    self.running.borrow().last().copied()
  }

  pub fn record_write(&self, signal_id: SignalId) {
    self.written.borrow_mut().insert(signal_id);
  }

  pub fn take_writes(&self) -> Vec<SignalId> {
    self.written.borrow_mut().drain().collect()
  }
}

impl Runtime {
  pub fn run_tracked(node: Node, action: impl FnOnce()) {
    RUNTIME.with(|runtime| {
      runtime.nodes.with_observer(node, |observer| observer.dirty = false);
      runtime.tracker.push(node);
    });
    let _tracking = defer_unwind(|panicked| {
      RUNTIME.with(|runtime| {
        runtime.tracker.pop();
        if panicked {
          runtime.nodes.clear_sources(node);
        }
      });
    });
    action();
  }

  /// The value of the signal or memo `source`, recorded as a source of the running observer, or
  /// `None` once it's freed.
  pub(crate) fn read(source: Node) -> Option<Rc<ValueCell<dyn Any>>> {
    RUNTIME.with(|runtime| {
      let value = runtime.nodes.value(source)?;
      if let Some(reader) = runtime.tracker.current() {
        runtime.nodes.add_edge(source, reader);
      }
      Some(value)
    })
  }
}
