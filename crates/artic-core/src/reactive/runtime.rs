mod effect_store;
mod graph;
mod memo_store;
mod node;
mod ownership;
mod refresh;
mod scheduling;
mod scope_store;
mod signal_store;
mod tracking;
mod value_cell;

use crate::defer::defer;
use crate::reactive::{Change, LiveCounts};
pub use effect_store::EffectId;
use effect_store::EffectStore;
use graph::DependencyGraph;
pub use memo_store::MemoId;
use memo_store::MemoStore;
pub use node::Node;
use scheduling::Scheduler;
pub use scope_store::ScopeId;
use scope_store::ScopeStore;
pub use signal_store::SignalId;
use signal_store::SignalStore;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use tracking::Tracker;

thread_local! {
  static RUNTIME: Runtime  = const {
    Runtime::new()
  };
}

pub struct Runtime {
  signals: SignalStore,
  memos: MemoStore,
  effects: EffectStore,
  scopes: ScopeStore,
  graph: DependencyGraph,
  tracker: Tracker,
  scheduler: Scheduler,
}

impl Runtime {
  const fn new() -> Self {
    Self {
      signals: SignalStore::new(),
      memos: MemoStore::new(),
      effects: EffectStore::new(),
      scopes: ScopeStore::new(),
      graph: DependencyGraph::new(),
      tracker: Tracker::new(),
      scheduler: Scheduler::new(),
    }
  }

  pub fn create_signal<TValue>(value: TValue) -> SignalId
  where
    TValue: Any,
  {
    let signal_id = Self::create_unowned_signal(value);
    RUNTIME.with(|runtime| runtime.scopes.add_to_active(Node::Signal(signal_id)));
    signal_id
  }

  /// Creates a signal no scope owns: whoever made it disposes it with `dispose_signal`.
  pub fn create_unowned_signal<TValue>(value: TValue) -> SignalId
  where
    TValue: Any,
  {
    RUNTIME.with(|runtime| runtime.signals.alloc_value(value))
  }

  pub fn contains_signal(signal_id: SignalId) -> bool {
    RUNTIME.with(|runtime| runtime.signals.contains(signal_id))
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
    Self::record_read(Node::Signal(signal_id));
    Self::with_untracked_signal_value(signal_id, action)
  }

  pub(crate) fn with_untracked_signal_value<TValue, TResult>(
    signal_id: SignalId,
    action: impl FnOnce(&TValue) -> TResult,
  ) -> TResult
  where
    TValue: Any,
  {
    RUNTIME.with(|runtime| runtime.signals.with_signal_value(signal_id, action))
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
    RUNTIME.with(|runtime| {
      let _written = defer(|| runtime.record_write(signal_id));
      runtime.signals.with_signal_mut_value(signal_id, action);
    })
  }

  pub fn update_signal_value_with<TValue>(signal_id: SignalId, action: impl FnOnce(&mut TValue) -> Change)
  where
    TValue: Any,
  {
    RUNTIME.with(|runtime| {
      let written = defer(|| runtime.record_write(signal_id));
      if runtime.signals.with_signal_mut_value(signal_id, action) == Change::Hidden {
        written.cancel();
      }
    })
  }

  #[cfg(test)]
  pub fn create_observer() -> Node {
    let node = Node::Memo(Self::create_memo(Box::new(()), Rc::new(|_| {})));
    RUNTIME.with(|runtime| runtime.set_dirty(node, false));
    node
  }

  #[cfg(test)]
  pub fn subscriber_count(signal_id: SignalId) -> usize {
    RUNTIME.with(|runtime| {
      let mut count = 0;
      runtime
        .graph
        .with_subscribers(Node::Signal(signal_id), |subscribers| count = subscribers.len());
      count
    })
  }

  #[cfg(test)]
  pub fn clear_sources(node: Node) {
    RUNTIME.with(|runtime| runtime.graph.clear_sources(node));
  }

  #[cfg(test)]
  pub fn sources(node: Node) -> Vec<Node> {
    RUNTIME.with(|runtime| (0..).map_while(|index| runtime.graph.source_at(node, index)).collect())
  }

  pub fn is_dirty(node: Node) -> bool {
    RUNTIME.with(|runtime| match node {
      Node::Memo(memo_id) => runtime.memos.is_dirty(memo_id),
      Node::Effect(effect_id) => runtime.effects.is_dirty(effect_id),
      Node::Signal(_) => false,
    })
  }

  pub fn create_memo(initial_value: Box<dyn Any>, compute: Rc<dyn Fn(MemoId)>) -> MemoId {
    RUNTIME.with(|runtime| {
      let memo_id = runtime.memos.alloc(initial_value, compute);
      let node = Node::Memo(memo_id);
      runtime.scopes.add_to_active(node);
      runtime.scopes.alloc_for(node);
      memo_id
    })
  }

  pub fn contains_memo(memo_id: MemoId) -> bool {
    RUNTIME.with(|runtime| runtime.memos.contains(memo_id))
  }

  pub fn with_memo_value<TValue, TResult>(memo_id: MemoId, action: impl FnOnce(&TValue) -> TResult) -> TResult
  where
    TValue: Any,
  {
    RUNTIME.with(|runtime| runtime.memos.value(memo_id)).with(action)
  }

  pub fn set_memo_value<TValue>(memo_id: MemoId, value: TValue)
  where
    TValue: Any,
  {
    let cell = RUNTIME.with(|runtime| runtime.memos.value(memo_id));
    let _changed = defer(|| RUNTIME.with(|runtime| runtime.mark_subscribers_dirty(Node::Memo(memo_id))));
    cell.with_mut(|current: &mut TValue| *current = value);
  }

  pub fn create_effect(run: Rc<RefCell<dyn FnMut()>>) {
    let effect_id = RUNTIME.with(|runtime| {
      let effect_id = runtime.effects.alloc(run);
      let node = Node::Effect(effect_id);
      runtime.scopes.add_to_active(node);
      runtime.scopes.alloc_for(node);
      effect_id
    });
    Self::refresh_effect(effect_id);
  }

  pub fn live_counts() -> LiveCounts {
    RUNTIME.with(|runtime| LiveCounts {
      signals: runtime.signals.count(),
      memos: runtime.memos.count(),
      effects: runtime.effects.count(),
      scopes: runtime.scopes.count(),
    })
  }

  /// Records a write to `signal_id`, for the next flush, and marks its readers dirty.
  fn record_write(&self, signal_id: SignalId) {
    self.tracker.record_write(signal_id);
    self.mark_subscribers_dirty(Node::Signal(signal_id));
  }

  fn mark_subscribers_dirty(&self, node: Node) {
    self.graph.with_subscribers(node, |subscribers| {
      for subscriber in subscribers {
        self.set_dirty(*subscriber, true);
      }
    });
  }

  fn set_dirty(&self, node: Node, is_dirty: bool) {
    match node {
      Node::Memo(memo_id) => self.memos.set_dirty(memo_id, is_dirty),
      Node::Effect(effect_id) => self.effects.set_dirty(effect_id, is_dirty),
      Node::Signal(_) => unreachable!("a signal reads nothing, so it is never dirty"),
    }
  }
}
