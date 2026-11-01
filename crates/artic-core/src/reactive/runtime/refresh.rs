use crate::defer::defer_unwind;
use crate::reactive::runtime::nodes::{Job, NodeKind};
use crate::reactive::runtime::{Node, RUNTIME, Runtime};
use std::rc::Rc;

#[cfg(test)]
thread_local! {
  /// How many sources `sources_changed` has checked on this thread.
  static SOURCE_CHECKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

impl Runtime {
  /// Brings `node`, a memo or an effect, up to date: checks its sources, and runs it again if one
  /// of them changed. A memo read while it computes panics; an effect that a write reaches while it
  /// runs is left for the next flush. A refresh that panics, checking the sources or running, leaves
  /// the node dirty, and an effect pending.
  pub fn refresh(node: Node) {
    let epoch = RUNTIME.with(|runtime| runtime.epoch.get());
    let (running, is_memo, verified) = RUNTIME.with(|runtime| {
      runtime.nodes.with_observer(node, |observer| {
        let is_memo = matches!(observer.job, Job::Memo { .. });
        (observer.running, is_memo, !observer.dirty && observer.verified == epoch)
      })
    });
    if running {
      assert!(
        !is_memo,
        "memo read while it is being computed; a memo can't depend on itself"
      );
      return;
    }
    if verified {
      return;
    }
    let changed = {
      let _checking = defer_unwind(|panicked| {
        if panicked {
          RUNTIME.with(|runtime| runtime.left_by_panic(node));
        }
      });
      Self::sources_changed(node)
    };
    if !RUNTIME.with(|runtime| runtime.nodes.contains(node)) {
      return;
    }
    if !changed {
      RUNTIME.with(|runtime| runtime.nodes.with_observer(node, |observer| observer.verified = epoch));
      return;
    }
    let (scope, run) = RUNTIME.with(|runtime| {
      runtime.nodes.with_observer(node, |observer| {
        observer.running = true;
        (observer.scope, Rc::clone(&observer.run))
      })
    });
    let _ran = defer_unwind(|panicked| RUNTIME.with(|runtime| runtime.end_run(node, epoch, panicked)));
    Self::clear_scope(scope);
    if !RUNTIME.with(|runtime| runtime.nodes.contains(node)) {
      return;
    }
    RUNTIME.with(|runtime| runtime.nodes.clear_sources(node));
    Self::run_tracked(node, || Self::run_in_scope(scope, || run(node)));
  }

  /// Ends a run of `node` that started in `epoch`: it stops running and is verified in that epoch,
  /// and after a panic it's left as [`left_by_panic`](Runtime::left_by_panic) says. A node disposed
  /// during its own run is already gone, edges and all.
  fn end_run(&self, node: Node, epoch: u64, panicked: bool) {
    if !self.nodes.contains(node) {
      return;
    }
    self.nodes.with_observer(node, |observer| {
      observer.running = false;
      observer.verified = epoch;
    });
    if panicked {
      self.left_by_panic(node);
    }
  }

  /// Leaves `node`, whose refresh panicked, dirty, and an effect pending, so the next read or flush
  /// tries again.
  fn left_by_panic(&self, node: Node) {
    if !self.nodes.contains(node) {
      return;
    }
    let seq = self.nodes.with_observer(node, |observer| {
      observer.dirty = true;
      match observer.job {
        Job::Effect { seq } => Some(seq),
        Job::Memo { .. } => None,
      }
    });
    if let Some(seq) = seq {
      self.scheduler.add_pending(seq, node);
    }
  }

  #[cfg(test)]
  pub fn source_checks() -> u64 {
    SOURCE_CHECKS.get()
  }

  /// Checks `node`'s sources in order, refreshing each memo among them, until one of them marks it
  /// dirty, and says whether it is. A source disposed while it's checked leaves the list, so the
  /// next one takes its place and is checked next.
  fn sources_changed(node: Node) -> bool {
    let mut index = 0;
    while !Self::is_dirty(node) {
      let Some(source) = RUNTIME.with(|runtime| runtime.nodes.source_at(node, index)) else {
        break;
      };
      #[cfg(test)]
      SOURCE_CHECKS.set(SOURCE_CHECKS.get() + 1);
      if RUNTIME.with(|runtime| runtime.nodes.kind(source)) == Some(NodeKind::Memo) {
        Self::refresh(source);
      }
      if RUNTIME.with(|runtime| runtime.nodes.source_at(node, index)) == Some(source) {
        index += 1;
      }
    }
    Self::is_dirty(node)
  }
}
