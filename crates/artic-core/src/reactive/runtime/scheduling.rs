use crate::defer::defer;
use crate::reactive::runtime::{EffectId, Node, RUNTIME, Runtime};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::ops::Bound::{Excluded, Unbounded};

const MAX_FLUSH_PASSES: usize = 100;

pub struct Scheduler {
  is_flushing: Cell<bool>,
  batch_depth: Cell<usize>,
  pending: RefCell<BTreeMap<u64, EffectId>>,
}

impl Scheduler {
  pub const fn new() -> Self {
    Self {
      is_flushing: Cell::new(false),
      batch_depth: Cell::new(0),
      pending: RefCell::new(BTreeMap::new()),
    }
  }

  pub fn add_pending(&self, seq: u64, effect_id: EffectId) {
    self.pending.borrow_mut().insert(seq, effect_id);
  }

  pub fn remove_pending(&self, seq: u64) {
    self.pending.borrow_mut().remove(&seq);
  }

  fn has_pending(&self) -> bool {
    !self.pending.borrow().is_empty()
  }

  fn take_pending_after(&self, cursor: Option<u64>) -> Option<(u64, EffectId)> {
    let mut pending = self.pending.borrow_mut();
    let lower = cursor.map_or(Unbounded, Excluded);
    let (seq, effect_id) = pending.range((lower, Unbounded)).next().map(|(seq, id)| (*seq, *id))?;
    pending.remove(&seq);
    Some((seq, effect_id))
  }

  fn start_flush(&self) -> bool {
    self.batch_depth.get() == 0 && !self.is_flushing.replace(true)
  }

  fn end_flush(&self) {
    self.is_flushing.set(false);
  }

  fn enter_batch(&self) {
    self.batch_depth.set(self.batch_depth.get() + 1);
  }

  fn exit_batch(&self) {
    self.batch_depth.set(self.batch_depth.get() - 1);
  }
}

impl Runtime {
  pub fn batch<TResult>(action: impl FnOnce() -> TResult) -> TResult {
    RUNTIME.with(|runtime| runtime.scheduler.enter_batch());
    let result = {
      let _batch = defer(|| RUNTIME.with(|runtime| runtime.scheduler.exit_batch()));
      action()
    };
    Self::flush_effects();
    result
  }

  pub fn flush_effects() {
    if !RUNTIME.with(|runtime| runtime.scheduler.start_flush()) {
      return;
    }
    let _flushing = defer(|| RUNTIME.with(|runtime| runtime.scheduler.end_flush()));
    Self::run_passes();
  }

  #[cfg(test)]
  pub fn collect_pending_effects() -> usize {
    Self::collect_pending();
    RUNTIME.with(|runtime| runtime.scheduler.pending.borrow().len())
  }

  fn run_passes() {
    for _ in 0..MAX_FLUSH_PASSES {
      Self::collect_pending();
      if !RUNTIME.with(|runtime| runtime.scheduler.has_pending()) {
        return;
      }
      let mut cursor = None;
      while let Some((seq, effect_id)) = RUNTIME.with(|runtime| runtime.scheduler.take_pending_after(cursor)) {
        cursor = Some(seq);
        Self::refresh_effect(effect_id);
        Self::collect_pending();
      }
    }
    panic!("effects did not settle after {MAX_FLUSH_PASSES} passes; two effects probably write each other's inputs");
  }

  fn collect_pending() {
    RUNTIME.with(|runtime| {
      let mut to_visit: Vec<Node> = runtime.tracker.take_writes().into_iter().map(Node::Signal).collect();
      let mut visited: HashSet<Node> = to_visit.iter().copied().collect();
      while let Some(node) = to_visit.pop() {
        runtime.graph.with_subscribers(node, |subscribers| {
          for subscriber in subscribers {
            match subscriber {
              Node::Effect(effect_id) => runtime.scheduler.add_pending(runtime.effects.seq(*effect_id), *effect_id),
              Node::Memo(_) => {
                if visited.insert(*subscriber) {
                  to_visit.push(*subscriber);
                }
              }
              Node::Signal(_) => unreachable!("a signal reads nothing"),
            }
          }
        });
      }
    });
  }
}
