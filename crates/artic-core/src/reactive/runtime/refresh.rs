use crate::defer::defer;
use crate::reactive::runtime::{EffectId, MemoId, Node, RUNTIME, Runtime};
use std::thread;

impl Runtime {
  pub fn refresh_memo(memo_id: MemoId) {
    let node = Node::Memo(memo_id);
    if RUNTIME.with(|runtime| runtime.memos.is_computing(memo_id)) {
      panic!("memo read while it is being computed; a memo can't depend on itself");
    }
    if !Self::refresh_sources(node) {
      return;
    }
    let (scope_id, compute) = RUNTIME.with(|runtime| {
      runtime.memos.set_computing(memo_id, true);
      (runtime.scopes.scope_of(node), runtime.memos.compute(memo_id))
    });
    let _computed = defer(|| {
      RUNTIME.with(|runtime| {
        if !runtime.memos.contains(memo_id) {
          runtime.graph.clear_sources(node);
          return;
        }
        runtime.memos.set_computing(memo_id, false);
        if thread::panicking() {
          runtime.set_dirty(node, true);
        }
      });
    });
    Self::clear_scope(scope_id);
    Self::rerun(node, || Self::run_in_scope(scope_id, || compute(memo_id)));
  }

  pub fn refresh_effect(effect_id: EffectId) {
    let node = Node::Effect(effect_id);
    if RUNTIME.with(|runtime| runtime.effects.is_running(effect_id)) || !Self::refresh_sources(node) {
      return;
    }
    let (scope_id, run) = RUNTIME.with(|runtime| {
      runtime.effects.set_running(effect_id, true);
      (runtime.scopes.scope_of(node), runtime.effects.run(effect_id))
    });
    let _ran = defer(|| {
      RUNTIME.with(|runtime| {
        if !runtime.effects.contains(effect_id) {
          runtime.graph.clear_sources(node);
          return;
        }
        runtime.effects.set_running(effect_id, false);
        if thread::panicking() {
          runtime.set_dirty(node, true);
          runtime.scheduler.add_pending(runtime.effects.seq(effect_id), effect_id);
        }
      });
    });
    Self::clear_scope(scope_id);
    Self::rerun(node, || Self::run_in_scope(scope_id, || (run.borrow_mut())()));
  }

  fn refresh_sources(node: Node) -> bool {
    let mut index = 0;
    while !Self::is_dirty(node) {
      let Some(source) = RUNTIME.with(|runtime| runtime.graph.source_at(node, index)) else {
        break;
      };
      source.refresh();
      index += 1;
    }
    Self::is_dirty(node)
  }

  fn rerun(node: Node, action: impl FnOnce()) {
    RUNTIME.with(|runtime| runtime.graph.clear_sources(node));
    Self::run_tracked(node, action);
  }
}
