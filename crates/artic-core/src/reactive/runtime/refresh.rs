use crate::defer::defer;
use crate::reactive::runtime::nodes::{Job, NodeKind};
use crate::reactive::runtime::{Node, RUNTIME, Runtime};
use std::rc::Rc;
use std::thread;

impl Runtime {
  /// Brings `node`, a memo or an effect, up to date: checks its sources, and runs it again if one
  /// of them changed. A memo read while it computes panics; an effect that a write reaches while it
  /// runs is left for the next flush. A run that panics leaves the node dirty, and an effect pending.
  pub fn refresh(node: Node) {
    let (running, is_memo) = RUNTIME.with(|runtime| {
      runtime
        .nodes
        .with_observer(node, |observer| (observer.running, matches!(observer.job, Job::Memo { .. })))
    });
    if running {
      assert!(
        !is_memo,
        "memo read while it is being computed; a memo can't depend on itself"
      );
      return;
    }
    if !Self::refresh_sources(node) {
      return;
    }
    let (scope, run) = RUNTIME.with(|runtime| {
      runtime.nodes.with_observer(node, |observer| {
        observer.running = true;
        (observer.scope, Rc::clone(&observer.run))
      })
    });
    let _ran = defer(|| RUNTIME.with(|runtime| runtime.end_run(node)));
    Self::clear_scope(scope);
    RUNTIME.with(|runtime| runtime.nodes.clear_sources(node));
    Self::run_tracked(node, || Self::run_in_scope(scope, || run(node)));
  }

  /// Ends a run of `node`: it stops running, and after a panic it's dirty and, for an effect,
  /// pending again. A node disposed during its own run is already gone, edges and all.
  fn end_run(&self, node: Node) {
    if !self.nodes.contains(node) {
      return;
    }
    let panicked = thread::panicking();
    let pending = self.nodes.with_observer(node, |observer| {
      observer.running = false;
      observer.dirty |= panicked;
      match observer.job {
        Job::Effect { seq } if panicked => Some(seq),
        _ => None,
      }
    });
    if let Some(seq) = pending {
      self.scheduler.add_pending(seq, node);
    }
  }

  fn refresh_sources(node: Node) -> bool {
    let mut index = 0;
    while !Self::is_dirty(node) {
      let Some(source) = RUNTIME.with(|runtime| runtime.nodes.source_at(node, index)) else {
        break;
      };
      if RUNTIME.with(|runtime| runtime.nodes.kind(source)) == Some(NodeKind::Memo) {
        Self::refresh(source);
      }
      index += 1;
    }
    Self::is_dirty(node)
  }
}
