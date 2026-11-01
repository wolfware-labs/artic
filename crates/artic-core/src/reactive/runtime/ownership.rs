use crate::defer::defer;
use crate::reactive::runtime::{EffectId, MemoId, Node, RUNTIME, Runtime, ScopeId, SignalId};

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
    for node in &nodes {
      if let Node::Effect(effect_id) = node {
        Self::dispose_effect(*effect_id);
      }
    }
    for node in &nodes {
      if let Node::Memo(memo_id) = node {
        Self::dispose_memo(*memo_id);
      }
    }
    for node in &nodes {
      if let Node::Signal(signal_id) = node {
        Self::dispose_signal(*signal_id);
      }
    }
  }

  fn dispose_effect(effect_id: EffectId) {
    Self::dispose_scope_of(Node::Effect(effect_id));
    RUNTIME.with(|runtime| {
      runtime.graph.remove(Node::Effect(effect_id));
      runtime.scheduler.remove_pending(runtime.effects.seq(effect_id));
      runtime.effects.free(effect_id);
    });
  }

  fn dispose_memo(memo_id: MemoId) {
    Self::dispose_scope_of(Node::Memo(memo_id));
    RUNTIME.with(|runtime| {
      runtime.graph.remove(Node::Memo(memo_id));
      runtime.memos.free(memo_id);
    });
  }

  fn dispose_scope_of(node: Node) {
    if let Some(scope_id) = RUNTIME.with(|runtime| runtime.scopes.take_scope_of(node)) {
      Self::dispose_scope(scope_id);
    }
  }

  /// Frees one signal and cuts its edges, whatever scope owns it. A stale id frees nothing.
  pub fn dispose_signal(signal_id: SignalId) {
    RUNTIME.with(|runtime| {
      runtime.graph.remove(Node::Signal(signal_id));
      runtime.signals.free(signal_id);
    });
  }
}
