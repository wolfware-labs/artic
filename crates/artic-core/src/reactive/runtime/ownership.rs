use crate::defer::defer;
use crate::reactive::runtime::nodes::NodeKind;
use crate::reactive::runtime::{Node, RUNTIME, Runtime, ScopeId, SignalId};

impl Runtime {
  pub fn create_scope(parent: Option<ScopeId>) -> ScopeId {
    RUNTIME.with(|runtime| runtime.scopes.alloc(parent))
  }

  pub fn contains_scope(scope_id: ScopeId) -> bool {
    RUNTIME.with(|runtime| runtime.scopes.contains(scope_id))
  }

  pub fn run_in_scope<TResult>(scope_id: ScopeId, action: impl FnOnce() -> TResult) -> TResult {
    RUNTIME.with(|runtime| runtime.scopes.push_active(scope_id));
    let _active = defer(|| RUNTIME.with(|runtime| runtime.scopes.pop_active()));
    action()
  }

  #[cfg(test)]
  pub fn child_count(scope_id: ScopeId) -> usize {
    RUNTIME.with(|runtime| runtime.scopes.child_count(scope_id))
  }

  #[cfg(test)]
  pub fn owned_count(scope_id: ScopeId) -> usize {
    RUNTIME.with(|runtime| runtime.owned(scope_id).len())
  }

  #[cfg(test)]
  pub fn active_scope() -> Option<ScopeId> {
    RUNTIME.with(|runtime| runtime.scopes.active())
  }

  pub fn dispose_scope(scope_id: ScopeId) {
    if !RUNTIME.with(|runtime| runtime.scopes.contains(scope_id)) {
      return;
    }
    Self::clear_scope(scope_id);
    RUNTIME.with(|runtime| {
      if let Some(parent) = runtime.scopes.parent(scope_id) {
        runtime.scopes.remove_child(parent, scope_id);
      }
      runtime.scopes.free(scope_id);
    });
  }

  pub(super) fn clear_scope(scope_id: ScopeId) {
    let (children, nodes) = RUNTIME.with(|runtime| (runtime.scopes.take_children(scope_id), runtime.owned(scope_id)));
    for child in children {
      Self::dispose_scope(child);
    }
    let mut nodes: Vec<(NodeKind, Node)> = RUNTIME.with(|runtime| {
      nodes
        .into_iter()
        .filter_map(|node| Some((runtime.nodes.kind(node)?, node)))
        .collect()
    });
    nodes.sort_by_key(|(kind, _)| match kind {
      NodeKind::Effect => 0,
      NodeKind::Memo => 1,
      NodeKind::Signal => 2,
    });
    for (_, node) in nodes {
      Self::dispose_node(node);
    }
  }

  /// Frees one signal and cuts its edges, whatever scope owns it. A stale id frees nothing.
  pub fn dispose_signal(signal_id: SignalId) {
    Self::dispose_node(signal_id.into());
  }

  /// Frees `node` and cuts its edges; a memo or an effect disposes the scope its runs own first,
  /// and an effect leaves the pending list. A stale id frees nothing.
  fn dispose_node(node: Node) {
    let parts = RUNTIME.with(|runtime| runtime.nodes.observer_parts(node));
    if let Some((scope, _)) = parts {
      Self::dispose_scope(scope);
    }
    let freed = RUNTIME.with(|runtime| {
      if let Some((_, Some(seq))) = parts {
        runtime.scheduler.remove_pending(seq);
      }
      runtime.disown(node);
      runtime.nodes.remove(node)
    });
    drop(freed);
  }
}

impl Runtime {
  /// Makes the active scope, if there is one, the owner of `node`, at the head of its list.
  pub(super) fn own(&self, node: Node) {
    if let Some(scope) = self.scopes.active() {
      self.nodes.link(node, scope, self.scopes.first(scope));
      self.scopes.set_first(scope, Some(node));
    }
  }

  /// Takes `node` out of its owner's list.
  fn disown(&self, node: Node) {
    if let Some((scope, first)) = self.nodes.unlink(node) {
      self.scopes.set_first(scope, first);
    }
  }

  /// The nodes `scope_id` owns, oldest first.
  fn owned(&self, scope_id: ScopeId) -> Vec<Node> {
    let mut owned: Vec<Node> =
      std::iter::successors(self.scopes.first(scope_id), |node| self.nodes.next_owned(*node)).collect();
    owned.reverse();
    owned
  }
}
