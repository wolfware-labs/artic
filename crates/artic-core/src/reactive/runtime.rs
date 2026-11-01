mod nodes;
mod ownership;
mod refresh;
mod scheduling;
mod scope_store;
mod tracking;
mod value_cell;

use crate::defer::defer;
use crate::reactive::{Change, LiveCounts};
pub use nodes::{MemoId, Node, SignalId};
use nodes::{NodeKind, Nodes};
use scheduling::Scheduler;
pub use scope_store::ScopeId;
use scope_store::ScopeStore;
use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;
use tracking::Tracker;

thread_local! {
  static RUNTIME: Runtime  = const {
    Runtime::new()
  };
}

pub struct Runtime {
  nodes: Nodes,
  scopes: ScopeStore,
  tracker: Tracker,
  scheduler: Scheduler,
  /// Bumped by every visible signal write, so an observer verified in the current epoch is up to
  /// date without checking its sources.
  epoch: Cell<u64>,
}

impl Runtime {
  const fn new() -> Self {
    Self {
      nodes: Nodes::new(),
      scopes: ScopeStore::new(),
      tracker: Tracker::new(),
      scheduler: Scheduler::new(),
      epoch: Cell::new(1),
    }
  }

  pub fn create_signal<TValue>(value: TValue) -> SignalId
  where
    TValue: Any,
  {
    let signal_id = Self::create_unowned_signal(value);
    RUNTIME.with(|runtime| runtime.own(signal_id.into()));
    signal_id
  }

  /// Creates a signal no scope owns: whoever made it disposes it with `dispose_signal`.
  pub fn create_unowned_signal<TValue>(value: TValue) -> SignalId
  where
    TValue: Any,
  {
    RUNTIME.with(|runtime| runtime.nodes.alloc_signal(Box::new(value)))
  }

  pub fn contains_signal(signal_id: SignalId) -> bool {
    RUNTIME.with(|runtime| runtime.nodes.contains(signal_id.into()))
  }

  pub fn get_signal_value<TValue>(signal_id: SignalId) -> TValue
  where
    TValue: Any + Clone,
  {
    Self::with_signal_value::<TValue, TValue>(signal_id, |v| v.clone())
  }

  pub fn with_signal_value<TValue, TResult>(signal_id: SignalId, action: impl FnOnce(&TValue) -> TResult) -> TResult
  where
    TValue: Any,
  {
    Self::record_read(signal_id.into());
    Self::with_untracked_signal_value(signal_id, action)
  }

  pub(crate) fn with_untracked_signal_value<TValue, TResult>(
    signal_id: SignalId,
    action: impl FnOnce(&TValue) -> TResult,
  ) -> TResult
  where
    TValue: Any,
  {
    RUNTIME.with(|runtime| runtime.nodes.signal_value(signal_id)).with(action)
  }

  pub fn set_signal_value<TValue>(signal_id: SignalId, value: TValue)
  where
    TValue: Any,
  {
    Self::with_signal_mut_value(signal_id, |v| *v = value)
  }

  pub fn with_signal_mut_value<TValue>(signal_id: SignalId, action: impl FnOnce(&mut TValue))
  where
    TValue: Any,
  {
    let cell = RUNTIME.with(|runtime| runtime.nodes.signal_value(signal_id));
    let _written = defer(|| RUNTIME.with(|runtime| runtime.record_write(signal_id)));
    cell.with_mut(action);
  }

  pub fn update_signal_value_with<TValue>(signal_id: SignalId, action: impl FnOnce(&mut TValue) -> Change)
  where
    TValue: Any,
  {
    let cell = RUNTIME.with(|runtime| runtime.nodes.signal_value(signal_id));
    let written = defer(|| RUNTIME.with(|runtime| runtime.record_write(signal_id)));
    if cell.with_mut(action) == Change::Hidden {
      written.cancel();
    }
  }

  #[cfg(test)]
  pub fn create_observer() -> Node {
    let node = Self::create_effect_node(|| {});
    RUNTIME.with(|runtime| runtime.nodes.with_observer(node, |observer| observer.dirty = false));
    node
  }

  #[cfg(test)]
  pub fn subscriber_count(signal_id: SignalId) -> usize {
    RUNTIME.with(|runtime| runtime.nodes.subscriber_count(signal_id.into()))
  }

  #[cfg(test)]
  pub fn clear_sources(node: Node) {
    RUNTIME.with(|runtime| runtime.nodes.clear_sources(node));
  }

  #[cfg(test)]
  pub fn sources(node: Node) -> Vec<Node> {
    RUNTIME.with(|runtime| (0..).map_while(|index| runtime.nodes.source_at(node, index)).collect())
  }

  #[cfg(test)]
  pub fn is_memo(node: Node) -> bool {
    RUNTIME.with(|runtime| runtime.nodes.kind(node) == Some(NodeKind::Memo))
  }

  pub fn is_dirty(node: Node) -> bool {
    RUNTIME.with(|runtime| runtime.nodes.is_dirty(node))
  }

  pub fn create_memo(initial_value: Box<dyn Any>, compute: Rc<dyn Fn(MemoId)>) -> MemoId {
    RUNTIME.with(|runtime| {
      let scope = runtime.scopes.alloc(runtime.scopes.active());
      let memo_id = runtime.nodes.alloc_memo(initial_value, compute, scope);
      runtime.own(memo_id.into());
      memo_id
    })
  }

  pub fn contains_memo(memo_id: MemoId) -> bool {
    RUNTIME.with(|runtime| runtime.nodes.contains(memo_id.into()))
  }

  pub fn with_memo_value<TValue, TResult>(memo_id: MemoId, action: impl FnOnce(&TValue) -> TResult) -> TResult
  where
    TValue: Any,
  {
    RUNTIME.with(|runtime| runtime.nodes.memo_value(memo_id)).with(action)
  }

  pub fn set_memo_value<TValue>(memo_id: MemoId, value: TValue)
  where
    TValue: Any,
  {
    let cell = RUNTIME.with(|runtime| runtime.nodes.memo_value(memo_id));
    let _changed = defer(|| RUNTIME.with(|runtime| runtime.nodes.mark_subscribers_dirty(memo_id.into())));
    cell.with_mut(|current: &mut TValue| *current = value);
  }

  pub fn create_effect(run: impl FnMut() + 'static) {
    let node = Self::create_effect_node(run);
    Self::refresh(node);
  }

  pub fn live_counts() -> LiveCounts {
    RUNTIME.with(|runtime| LiveCounts {
      signals: runtime.nodes.count(NodeKind::Signal),
      memos: runtime.nodes.count(NodeKind::Memo),
      effects: runtime.nodes.count(NodeKind::Effect),
      scopes: runtime.scopes.count(),
    })
  }

  fn create_effect_node(run: impl FnMut() + 'static) -> Node {
    RUNTIME.with(|runtime| {
      let scope = runtime.scopes.alloc(runtime.scopes.active());
      let node = runtime.nodes.alloc_effect(run, scope);
      runtime.own(node);
      node
    })
  }

  /// Records a write to `signal_id`, for the next flush, and marks its readers dirty.
  fn record_write(&self, signal_id: SignalId) {
    self.tracker.record_write(signal_id);
    self.epoch.set(self.epoch.get() + 1);
    self.nodes.mark_subscribers_dirty(signal_id.into());
  }
}
