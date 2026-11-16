//! A window's side of accessibility: whether a screen reader reads it, set by accesskit's handlers,
//! which may run on another thread, and what an update becomes when the reader restarted meanwhile.

use accesskit::{
  ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, Node, NodeId, Role, TreeInfo, TreeUpdate,
};
use artic_platform::Waker;
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

const INACTIVE: u8 = 0;
const REQUESTED: u8 = 1;
const ACTIVE: u8 = 2;

/// One window's accessibility state: `Inactive` until a screen reader starts, `Requested` until the
/// loop has reported it to artic, then `Active` until the reader stops.
///
/// One atomic for the three states, rather than a flag each, so a deactivation that lands between
/// the report and the move to `Active` leaves the window `Inactive`.
#[derive(Debug, Default)]
pub(crate) struct Activation(AtomicU8);

impl Activation {
  /// A screen reader started reading the window; the loop reports it next.
  pub(crate) fn request(&self) {
    self.0.store(REQUESTED, Ordering::Release);
  }

  /// The screen reader stopped.
  pub(crate) fn deactivate(&self) {
    self.0.store(INACTIVE, Ordering::Release);
  }

  /// Whether a start is waiting to be reported.
  pub(crate) fn is_requested(&self) -> bool {
    self.0.load(Ordering::Acquire) == REQUESTED
  }

  /// Moves a reported start to `Active`, unless the reader stopped since.
  pub(crate) fn activate_if_requested(&self) {
    let _ = self
      .0
      .compare_exchange(REQUESTED, ACTIVE, Ordering::AcqRel, Ordering::Acquire);
  }

  /// Whether a screen reader reads the window, counted from the moment the loop reported it.
  pub(crate) fn is_active(&self) -> bool {
    self.0.load(Ordering::Acquire) == ACTIVE
  }
}

/// The calls a window makes on its accesskit adapter.
pub(crate) trait Updates {
  /// Applies the update `updater` builds, only while a screen reader is connected.
  fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate);
}

/// A window's accessibility: its adapter, whether a screen reader reads it, and the root of the
/// last whole tree it handed over.
pub(crate) struct Access<A> {
  pub(crate) adapter: RefCell<A>,
  activation: Arc<Activation>,
  root: Cell<Option<NodeId>>,
}

impl<A: Updates> Access<A> {
  pub(crate) fn new(adapter: A, activation: Arc<Activation>) -> Self {
    Self {
      adapter: RefCell::new(adapter),
      activation,
      root: Cell::new(None),
    }
  }

  pub(crate) fn activation(&self) -> &Activation {
    &self.activation
  }

  /// Hands `update` over through `update_if_active`, whose closure runs under the lock the
  /// activation handler takes, and checks there again whether the reader restarted meanwhile.
  pub(crate) fn hand_over(&self, update: TreeUpdate) {
    if let Some(tree) = &update.tree {
      self.root.set(Some(tree.root));
    }
    let (activation, root) = (&self.activation, self.root.get());
    self
      .adapter
      .borrow_mut()
      .update_if_active(|| checked(update, activation.is_active(), root));
  }
}

/// The update to hand a window's adapter for `update`, which artic built while the window counted as
/// read. A reader can stop and start again before the adapter takes it: the adapter then waits for a
/// whole tree, and accesskit panics on one without `tree`. So when the window is no longer `active`
/// and `update` has no tree, the window hands over a tree of the root alone, the `root` of the last
/// whole tree it handed over; the whole update artic sends after the new start replaces it. A first
/// update without `tree`, which accesskit refuses, panics in debug builds.
fn checked(update: TreeUpdate, active: bool, root: Option<NodeId>) -> TreeUpdate {
  match root {
    Some(root) if !active && update.tree.is_none() => TreeUpdate {
      nodes: vec![(root, Node::new(Role::Window))],
      tree: Some(TreeInfo::new(root)),
      tree_id: update.tree_id,
      focus: root,
    },
    _ => {
      debug_assert!(
        root.is_some() || update.tree.is_some(),
        "a window's first update is the whole tree"
      );
      update
    }
  }
}

/// accesskit's call when a screen reader starts: the window waits for artic's whole tree, so it
/// returns none, marks the window requested and wakes the loop to report it.
pub(crate) struct Started {
  pub(crate) activation: Arc<Activation>,
  pub(crate) waker: Waker,
}

impl ActivationHandler for Started {
  fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
    self.activation.request();
    self.waker.wake();
    None
  }
}

/// accesskit's call when the screen reader stops.
pub(crate) struct Stopped(pub(crate) Arc<Activation>);

impl DeactivationHandler for Stopped {
  fn deactivate_accessibility(&mut self) {
    self.0.deactivate();
  }
}

/// Actions a screen reader asks for; artic takes none yet (#102).
pub(crate) struct NoActions;

impl ActionHandler for NoActions {
  fn do_action(&mut self, _request: ActionRequest) {}
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;
  use accesskit::TreeId;
  use std::sync::atomic::AtomicUsize;

  /// An adapter that applies updates while `connected`, keeping them.
  #[derive(Default)]
  pub(crate) struct FakeAdapter {
    connected: bool,
    pub(crate) applied: Vec<TreeUpdate>,
  }

  impl FakeAdapter {
    pub(crate) fn connected() -> Self {
      Self {
        connected: true,
        ..Self::default()
      }
    }
  }

  impl Updates for FakeAdapter {
    fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate) {
      if self.connected {
        self.applied.push(updater());
      }
    }
  }

  /// A whole tree rooted at node 1.
  pub(crate) fn whole() -> TreeUpdate {
    TreeUpdate {
      tree: Some(TreeInfo::new(NodeId(1))),
      ..incremental()
    }
  }

  // A window keeps the root of the whole trees it hands over, and when its reader restarted before an
  // incremental update reaches the adapter, it hands over that root alone instead.
  #[test]
  fn hand_over_after_a_restart_sends_the_kept_root() {
    let activation = Arc::new(Activation::default());
    activation.request();
    activation.activate_if_requested();
    let access = Access::new(FakeAdapter::connected(), Arc::clone(&activation));
    access.hand_over(whole());
    access.hand_over(incremental());

    activation.deactivate();
    activation.request();
    access.hand_over(incremental());

    let applied = &access.adapter.borrow().applied;
    assert_eq!(applied[..2], [whole(), incremental()]);
    assert_eq!(applied[2].tree, Some(TreeInfo::new(NodeId(1))));
    assert_eq!(applied[2].nodes, [(NodeId(1), Node::new(Role::Window))]);
  }

  // A whole tree handed over while no reader is connected still sets the kept root, so a later
  // update after a restart goes out as that root alone.
  #[test]
  fn root_is_kept_while_no_reader_is_connected() {
    let access = Access::new(FakeAdapter::default(), Arc::new(Activation::default()));
    access.hand_over(whole());
    access.adapter.borrow_mut().connected = true;

    access.hand_over(incremental());

    let applied = &access.adapter.borrow().applied;
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].tree, Some(TreeInfo::new(NodeId(1))));
  }

  // Actions a screen reader asks for are ignored, without a panic.
  #[test]
  fn actions_are_ignored() {
    NoActions.do_action(ActionRequest {
      action: accesskit::Action::Click,
      target_tree: TreeId::ROOT,
      target_node: NodeId(1),
      data: None,
    });
  }

  /// An incremental update of node 7, with the root, 1, focused.
  fn incremental() -> TreeUpdate {
    TreeUpdate {
      nodes: vec![(NodeId(7), Node::new(Role::Label))],
      tree: None,
      tree_id: TreeId::ROOT,
      focus: NodeId(1),
    }
  }

  // An update for a window still read goes as it is.
  #[test]
  fn update_for_a_read_window_is_unchanged() {
    assert_eq!(checked(incremental(), true, Some(NodeId(1))), incremental());
  }

  // A whole tree goes as it is even when the reader restarted, since the adapter takes it.
  #[test]
  fn whole_tree_is_unchanged() {
    assert_eq!(checked(whole(), false, Some(NodeId(1))), whole());
  }

  // A first update without a tree panics in debug builds, even for a window still read.
  #[test]
  #[cfg(debug_assertions)]
  #[should_panic(expected = "a window's first update is the whole tree")]
  fn first_update_without_a_tree_panics() {
    checked(incremental(), true, None);
  }

  // An incremental update for a reader that restarted becomes the root alone, as a whole tree.
  #[test]
  fn update_after_a_restart_is_the_root_alone() {
    let checked = checked(incremental(), false, Some(NodeId(1)));

    assert_eq!(
      checked,
      TreeUpdate {
        nodes: vec![(NodeId(1), Node::new(Role::Window))],
        tree: Some(TreeInfo::new(NodeId(1))),
        tree_id: TreeId::ROOT,
        focus: NodeId(1),
      }
    );
  }

  // The handlers mark the window: a start requests it and wakes the loop, returning no tree, and a
  // stop deactivates it.
  #[test]
  fn handlers_mark_the_window() {
    let activation = Arc::new(Activation::default());
    let wakes = Arc::new(AtomicUsize::new(0));
    let woken = Arc::clone(&wakes);
    let mut started = Started {
      activation: Arc::clone(&activation),
      waker: Waker::new(move || {
        woken.fetch_add(1, Ordering::Relaxed);
      }),
    };

    let tree = started.request_initial_tree();
    let requested = activation.is_requested();
    Stopped(Arc::clone(&activation)).deactivate_accessibility();

    assert_eq!((tree, requested, wakes.load(Ordering::Relaxed)), (None, true, 1));
    assert!(!activation.is_requested());
  }

  /// The state as the two questions see it: requested, active.
  fn state(activation: &Activation) -> (bool, bool) {
    (activation.is_requested(), activation.is_active())
  }

  // A start is requested, not active, until the loop reports it and moves it on.
  #[test]
  fn start_is_active_only_after_the_move() {
    let activation = Activation::default();
    assert_eq!(state(&activation), (false, false));

    activation.request();
    let requested = state(&activation);
    activation.activate_if_requested();

    assert_eq!((requested, state(&activation)), ((true, false), (false, true)));
  }

  // A stop between the report and the move leaves the window inactive.
  #[test]
  fn stop_before_the_move_stays_inactive() {
    let activation = Activation::default();
    activation.request();

    activation.deactivate();
    activation.activate_if_requested();

    assert_eq!(state(&activation), (false, false));
  }

  // A window that was never started isn't made active by the move.
  #[test]
  fn move_without_a_start_does_nothing() {
    let activation = Activation::default();

    activation.activate_if_requested();

    assert_eq!(state(&activation), (false, false));
  }

  // A stop ends an active window, and a later start is requested again.
  #[test]
  fn restart_is_requested_again() {
    let activation = Activation::default();
    activation.request();
    activation.activate_if_requested();

    activation.deactivate();
    let stopped = state(&activation);
    activation.request();

    assert_eq!((stopped, state(&activation)), ((false, false), (true, false)));
  }
}
