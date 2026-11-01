use crate::defer::defer;
use crate::reactive::runtime::nodes::{Job, NodeKind};
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
    let (children, nodes) =
      RUNTIME.with(|runtime| (runtime.scopes.take_children(scope_id), runtime.scopes.take_nodes(scope_id)));
    for child in children {
      Self::dispose_scope(child);
    }
    let kinds: Vec<(Node, Option<NodeKind>)> =
      RUNTIME.with(|runtime| nodes.iter().map(|node| (*node, runtime.nodes.kind(*node))).collect());
    for kind in [NodeKind::Effect, NodeKind::Memo, NodeKind::Signal] {
      for (node, _) in kinds.iter().filter(|(_, of)| *of == Some(kind)) {
        Self::dispose_node(*node);
      }
    }
  }

  /// Frees one signal and cuts its edges, whatever scope owns it. A stale id frees nothing.
  pub fn dispose_signal(signal_id: SignalId) {
    Self::dispose_node(signal_id.into());
  }

  /// Frees `node` and cuts its edges; a memo or an effect disposes the scope its runs own first,
  /// and an effect leaves the pending list. A stale id frees nothing.
  fn dispose_node(node: Node) {
    let owned = RUNTIME.with(|runtime| match runtime.nodes.kind(node) {
      Some(NodeKind::Memo | NodeKind::Effect) => Some(runtime.nodes.with_observer(node, |observer| {
        let seq = match observer.job {
          Job::Effect { seq } => Some(seq),
          Job::Memo { .. } => None,
        };
        (observer.scope, seq)
      })),
      Some(NodeKind::Signal) | None => None,
    });
    if let Some((scope, _)) = owned {
      Self::dispose_scope(scope);
    }
    RUNTIME.with(|runtime| {
      runtime.nodes.remove(node);
      if let Some((_, Some(seq))) = owned {
        runtime.scheduler.remove_pending(seq);
      }
    });
  }
}
