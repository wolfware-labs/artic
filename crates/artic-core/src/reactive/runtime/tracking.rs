use crate::defer::defer;
use crate::reactive::runtime::{Node, RUNTIME, Runtime, SignalId};
use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::{BuildHasherDefault, DefaultHasher};
use std::thread;

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
      runtime.set_dirty(node, false);
      runtime.tracker.push(node);
    });
    let _tracking = defer(|| {
      RUNTIME.with(|runtime| {
        runtime.tracker.pop();
        if thread::panicking() {
          runtime.graph.clear_sources(node);
        }
      });
    });
    action();
  }

  pub fn record_read(source: Node) {
    RUNTIME.with(|runtime| {
      if let Some(reader) = runtime.tracker.current() {
        runtime.graph.add_edge(source, reader);
      }
    })
  }
}
