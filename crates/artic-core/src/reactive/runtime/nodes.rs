//! Every reactive node in one arena, each slot holding the node's kind, its edges and, for a memo
//! or an effect, its observer state.

use crate::arena::{Arena, SlotId};
use crate::reactive::runtime::ScopeId;
use crate::reactive::runtime::value_cell::ValueCell;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Node(SlotId);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct SignalId(Node);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct MemoId(Node);

impl From<SignalId> for Node {
  fn from(signal_id: SignalId) -> Node {
    signal_id.0
  }
}

impl From<MemoId> for Node {
  fn from(memo_id: MemoId) -> Node {
    memo_id.0
  }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NodeKind {
  Signal,
  Memo,
  Effect,
}

/// What an observer is for: a memo keeps a value its run computes, an effect runs in the order it
/// was created.
pub enum Job {
  Memo { value: Rc<ValueCell> },
  Effect { seq: u64 },
}

/// A memo or an effect: what it runs, and where it is in running it.
pub struct Observer {
  pub job: Job,
  pub run: Rc<dyn Fn(Node)>,
  pub dirty: bool,
  /// The epoch the observer was last found up to date in, which counts only while it isn't dirty:
  /// until a signal is written again, it needn't check its sources.
  pub verified: u64,
  pub running: bool,
  /// The scope that owns what the observer's runs create, cleared before each run.
  pub scope: ScopeId,
}

impl Observer {
  fn seq(&self) -> Option<u64> {
    match self.job {
      Job::Effect { seq } => Some(seq),
      Job::Memo { .. } => None,
    }
  }
}

enum Kind {
  Signal { value: Rc<ValueCell> },
  Observer(Observer),
}

/// The scope that owns a node, and the node's neighbors in that scope's list.
#[derive(Clone, Copy)]
struct Owner {
  scope: ScopeId,
  previous: Option<Node>,
  next: Option<Node>,
}

struct Slot {
  kind: Kind,
  sources: Vec<Node>,
  subscribers: Vec<Node>,
  owner: Option<Owner>,
}

impl Slot {
  fn node_kind(&self) -> NodeKind {
    match &self.kind {
      Kind::Signal { .. } => NodeKind::Signal,
      Kind::Observer(Observer {
        job: Job::Memo { .. }, ..
      }) => NodeKind::Memo,
      Kind::Observer(Observer {
        job: Job::Effect { .. }, ..
      }) => NodeKind::Effect,
    }
  }
}

/// A freed node's slot, its value and closures still in it.
pub struct Freed {
  _slot: Slot,
}

pub struct Nodes {
  slots: RefCell<Arena<Slot>>,
  next_seq: Cell<u64>,
}

impl Nodes {
  pub const fn new() -> Self {
    Self {
      slots: RefCell::new(Arena::new()),
      next_seq: Cell::new(0),
    }
  }

  pub fn alloc_signal(&self, value: Box<dyn Any>) -> SignalId {
    SignalId(self.alloc(Kind::Signal {
      value: Rc::new(ValueCell::new(value, "signal")),
    }))
  }

  pub fn alloc_memo(&self, value: Box<dyn Any>, compute: Rc<dyn Fn(MemoId)>, scope: ScopeId) -> MemoId {
    let value = Rc::new(ValueCell::new(value, "memo"));
    MemoId(self.alloc_observer(Job::Memo { value }, Rc::new(move |node| compute(MemoId(node))), scope))
  }

  pub fn alloc_effect(&self, run: impl FnMut() + 'static, scope: ScopeId) -> Node {
    let seq = self.next_seq.replace(self.next_seq.get() + 1);
    let run = RefCell::new(run);
    self.alloc_observer(Job::Effect { seq }, Rc::new(move |_| (run.borrow_mut())()), scope)
  }

  /// Frees `node` and cuts its edges in both directions, returning what the slot held: the caller
  /// drops it once nothing in the runtime is borrowed, since a value or a closure in it may use the
  /// runtime when dropped. A stale id frees nothing.
  #[must_use]
  pub fn remove(&self, node: Node) -> Option<Freed> {
    self.clear_sources(node);
    let mut slots = self.slots.borrow_mut();
    let mut slot = slots.free(node.0)?;
    for subscriber in std::mem::take(&mut slot.subscribers) {
      if let Some(subscriber) = slots.get_mut(subscriber.0) {
        subscriber.sources.retain(|source| *source != node);
      }
    }
    Some(Freed { _slot: slot })
  }

  /// The scope a memo's or an effect's runs own, and an effect's place in the run order. `None`
  /// for a signal or a stale id.
  pub fn observer_parts(&self, node: Node) -> Option<(ScopeId, Option<u64>)> {
    match &self.slots.borrow().get(node.0)?.kind {
      Kind::Signal { .. } => None,
      Kind::Observer(observer) => Some((observer.scope, observer.seq())),
    }
  }

  pub fn contains(&self, node: Node) -> bool {
    self.slots.borrow().contains(node.0)
  }

  pub fn kind(&self, node: Node) -> Option<NodeKind> {
    self.slots.borrow().get(node.0).map(Slot::node_kind)
  }

  /// How many nodes of `kind` are alive.
  pub fn count(&self, kind: NodeKind) -> usize {
    let slots = self.slots.borrow();
    slots
      .ids()
      .filter(|id| slots.get(*id).is_some_and(|slot| slot.node_kind() == kind))
      .count()
  }

  pub fn signal_value(&self, signal_id: SignalId) -> Rc<ValueCell> {
    match &self.slot(signal_id.0, "signal").kind {
      Kind::Signal { value } => Rc::clone(value),
      Kind::Observer(_) => unreachable!("a signal id names a signal"),
    }
  }

  pub fn memo_value(&self, memo_id: MemoId) -> Rc<ValueCell> {
    self.with_observer(memo_id.0, |observer| match &observer.job {
      Job::Memo { value } => Rc::clone(value),
      Job::Effect { .. } => unreachable!("a memo id names a memo"),
    })
  }

  /// Calls `action` with `node`'s observer state. Panics if `node` is not a live observer.
  pub fn with_observer<TResult>(&self, node: Node, action: impl FnOnce(&mut Observer) -> TResult) -> TResult {
    let mut slots = self.slots.borrow_mut();
    match slots.get_mut(node.0).map(|slot| &mut slot.kind) {
      Some(Kind::Observer(observer)) => action(observer),
      _ => panic!("observer slot not found for the specified id"),
    }
  }

  /// Puts `node` at the head of `scope`'s list, before `first`, which headed it until now.
  pub fn link(&self, node: Node, scope: ScopeId, first: Option<Node>) {
    let mut slots = self.slots.borrow_mut();
    if let Some(slot) = slots.get_mut(node.0) {
      slot.owner = Some(Owner {
        scope,
        previous: None,
        next: first,
      });
    }
    if let Some(owner) = first.and_then(|first| slots.get_mut(first.0)?.owner.as_mut()) {
      owner.previous = Some(node);
    }
  }

  /// Takes `node` out of its owner's list. When `node` headed it, returns the scope and the node
  /// that heads it now.
  pub fn unlink(&self, node: Node) -> Option<(ScopeId, Option<Node>)> {
    let mut slots = self.slots.borrow_mut();
    let Owner { scope, previous, next } = slots.get_mut(node.0)?.owner.take()?;
    if let Some(owner) = next.and_then(|next| slots.get_mut(next.0)?.owner.as_mut()) {
      owner.previous = previous;
    }
    match previous.and_then(|previous| slots.get_mut(previous.0)?.owner.as_mut()) {
      Some(owner) => {
        owner.next = next;
        None
      }
      None => Some((scope, next)),
    }
  }

  /// The node after `node` in its owner's list.
  pub fn next_owned(&self, node: Node) -> Option<Node> {
    self.slots.borrow().get(node.0)?.owner?.next
  }

  /// Whether `node` is an observer that has to run again. A signal reads nothing, and a node that
  /// is gone runs nothing, so neither ever is.
  pub fn is_dirty(&self, node: Node) -> bool {
    match self.slots.borrow().get(node.0).map(|slot| &slot.kind) {
      Some(Kind::Observer(observer)) => observer.dirty,
      Some(Kind::Signal { .. }) | None => false,
    }
  }

  pub fn mark_subscribers_dirty(&self, node: Node) {
    let mut slots = self.slots.borrow_mut();
    let Some(subscribers) = slots.get(node.0).map(|slot| slot.subscribers.clone()) else {
      return;
    };
    for subscriber in subscribers {
      if let Some(Kind::Observer(observer)) = slots.get_mut(subscriber.0).map(|slot| &mut slot.kind) {
        observer.dirty = true;
      }
    }
  }

  /// Records that `subscriber` read `source`. A node disposed during its own run records nothing,
  /// so no live node lists a dead one.
  pub fn add_edge(&self, source: Node, subscriber: Node) {
    let mut slots = self.slots.borrow_mut();
    if !slots.contains(subscriber.0) {
      return;
    }
    let Some(source_slot) = slots.get_mut(source.0) else {
      return;
    };
    if source_slot.subscribers.contains(&subscriber) {
      return;
    }
    source_slot.subscribers.push(subscriber);
    if let Some(subscriber_slot) = slots.get_mut(subscriber.0) {
      subscriber_slot.sources.push(source);
    }
  }

  pub fn source_at(&self, node: Node, index: usize) -> Option<Node> {
    self.slots.borrow().get(node.0)?.sources.get(index).copied()
  }

  pub fn clear_sources(&self, node: Node) {
    let mut slots = self.slots.borrow_mut();
    let Some(sources) = slots.get_mut(node.0).map(|slot| std::mem::take(&mut slot.sources)) else {
      return;
    };
    for source in sources {
      if let Some(source) = slots.get_mut(source.0) {
        source.subscribers.retain(|subscriber| *subscriber != node);
      }
    }
  }

  /// The effects that `written` reaches, directly or through memos, each with its creation order.
  pub fn reached_effects(&self, written: impl IntoIterator<Item = Node>) -> Vec<(u64, Node)> {
    let slots = self.slots.borrow();
    let mut to_visit: Vec<Node> = written.into_iter().collect();
    let mut visited: HashSet<Node> = to_visit.iter().copied().collect();
    let mut reached = Vec::new();
    while let Some(node) = to_visit.pop() {
      let Some(slot) = slots.get(node.0) else {
        continue;
      };
      for subscriber in &slot.subscribers {
        match slots.get(subscriber.0).map(|slot| &slot.kind) {
          Some(Kind::Observer(observer)) => match observer.seq() {
            Some(seq) => reached.push((seq, *subscriber)),
            None => {
              if visited.insert(*subscriber) {
                to_visit.push(*subscriber);
              }
            }
          },
          Some(Kind::Signal { .. }) => unreachable!("a signal reads nothing"),
          None => {}
        }
      }
    }
    reached
  }

  #[cfg(test)]
  pub fn subscriber_count(&self, node: Node) -> usize {
    self.slots.borrow().get(node.0).map_or(0, |slot| slot.subscribers.len())
  }

  fn alloc(&self, kind: Kind) -> Node {
    Node(self.slots.borrow_mut().alloc(Slot {
      kind,
      sources: Vec::new(),
      subscribers: Vec::new(),
      owner: None,
    }))
  }

  fn alloc_observer(&self, job: Job, run: Rc<dyn Fn(Node)>, scope: ScopeId) -> Node {
    self.alloc(Kind::Observer(Observer {
      job,
      run,
      dirty: true,
      verified: 0,
      running: false,
      scope,
    }))
  }

  fn slot(&self, node: Node, label: &str) -> std::cell::Ref<'_, Slot> {
    std::cell::Ref::map(self.slots.borrow(), |slots| {
      slots
        .get(node.0)
        .unwrap_or_else(|| panic!("{label} slot not found for the specified id"))
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::reactive::runtime::scope_store::ScopeStore;
  use pretty_assertions::assert_eq;

  /// A signal and two observers to wire together, in their own arena.
  fn nodes() -> (Nodes, Node, Node, Node) {
    let nodes = Nodes::new();
    let scope = ScopeStore::new().alloc(None);
    let signal = nodes.alloc_signal(Box::new(0)).into();
    let observer = || nodes.alloc_effect(|| {}, scope);
    let (first, second) = (observer(), observer());
    (nodes, signal, first, second)
  }

  fn sources(nodes: &Nodes, node: Node) -> Vec<Node> {
    (0..).map_while(|index| nodes.source_at(node, index)).collect()
  }

  // Adding the same edge twice keeps a single entry on each side.
  #[test]
  fn add_edge_ignores_duplicates() {
    let (nodes, signal, reader, _) = nodes();

    nodes.add_edge(signal, reader);
    nodes.add_edge(signal, reader);

    assert_eq!((nodes.subscriber_count(signal), sources(&nodes, reader)), (1, vec![signal]));
  }

  // Clearing a node's sources removes it from each source's subscribers.
  #[test]
  fn clear_sources_cuts_both_directions() {
    let (nodes, signal, first, second) = nodes();
    nodes.add_edge(signal, first);
    nodes.add_edge(signal, second);

    nodes.clear_sources(first);

    assert_eq!((nodes.subscriber_count(signal), sources(&nodes, first)), (1, vec![]));
    assert_eq!(sources(&nodes, second), [signal]);
  }

  // Removing a node cuts its edges on both sides: its sources forget it, and so do its subscribers.
  #[test]
  fn remove_cuts_every_edge_of_the_node() {
    let (nodes, signal, middle, reader) = nodes();
    nodes.add_edge(signal, middle);
    nodes.add_edge(middle, reader);

    drop(nodes.remove(middle));

    assert_eq!((nodes.subscriber_count(signal), sources(&nodes, reader)), (0, vec![]));
    assert!(!nodes.contains(middle));
  }

  // A reader removed while it runs records no more reads, so no live node lists a dead one.
  #[test]
  fn a_removed_reader_records_nothing() {
    let (nodes, signal, reader, _) = nodes();

    drop(nodes.remove(reader));
    nodes.add_edge(signal, reader);

    assert_eq!(nodes.subscriber_count(signal), 0);
  }
}
