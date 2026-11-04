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
pub(crate) use value_cell::ValueCell;

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
    RUNTIME.with(|runtime| runtime.nodes.alloc_signal(value))
  }

  pub fn contains_signal(signal_id: SignalId) -> bool {
    RUNTIME.with(|runtime| runtime.nodes.contains(signal_id.into()))
  }

  /// The value of the signal or memo `node`, or `None` once it's freed: one lookup, which a read
  /// or a write makes before it touches the value.
  pub(crate) fn value(node: Node) -> Option<Rc<ValueCell<dyn Any>>> {
    RUNTIME.with(|runtime| runtime.nodes.value(node))
  }

  /// Changes `signal_id`'s `value` through `action`, then marks its readers, also if `action`
  /// panics. Panics, before anything changes, if the value is being read or updated.
  pub(crate) fn write_signal<TValue>(signal_id: SignalId, value: &ValueCell<dyn Any>, action: impl FnOnce(&mut TValue))
  where
    TValue: Any,
  {
    let taken = value.take();
    let _written = defer(|| Self::record_write(signal_id));
    taken.write(action);
  }

  /// [`write_signal`](Runtime::write_signal), marking the readers only when `action` says the change
  /// is visible.
  pub(crate) fn update_signal_with<TValue>(
    signal_id: SignalId,
    value: &ValueCell<dyn Any>,
    action: impl FnOnce(&mut TValue) -> Change,
  ) where
    TValue: Any,
  {
    let taken = value.take();
    let written = defer(|| Self::record_write(signal_id));
    if taken.write(action) == Change::Hidden {
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

  pub fn create_memo<TValue: Any>(initial_value: TValue, compute: Rc<dyn Fn(MemoId)>) -> MemoId {
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

  pub fn set_memo_value<TValue>(memo_id: MemoId, cell: &ValueCell<dyn Any>, value: TValue)
  where
    TValue: Any,
  {
    let taken = cell.take();
    let _changed = defer(|| {
      RUNTIME
        .with(|runtime| runtime.nodes.mark_subscribers_dirty(memo_id.into()))
        .call();
    });
    taken.write(|current: &mut TValue| *current = value);
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

  /// Records a write to `signal_id`, for the next flush, marks its readers dirty and calls back
  /// the sinks that read it.
  fn record_write(signal_id: SignalId) {
    let marks = RUNTIME.with(|runtime| {
      runtime.tracker.record_write(signal_id);
      runtime.epoch.set(runtime.epoch.get() + 1);
      runtime.nodes.mark_subscribers_dirty(signal_id.into())
    });
    marks.call();
  }

  pub fn set_sink(of: SignalId, mark: Rc<dyn Fn()>) {
    RUNTIME.with(|runtime| runtime.nodes.set_sink(of, mark));
  }

  pub fn link_signal(of: SignalId, source: Option<SignalId>) {
    RUNTIME.with(|runtime| runtime.nodes.link_signal(of, source));
  }

  pub fn has_sink(signal_id: SignalId) -> bool {
    RUNTIME.with(|runtime| runtime.nodes.has_sink(signal_id))
  }
}
