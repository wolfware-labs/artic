use crate::reactive::runtime::Runtime;

/// How many reactive nodes are alive on the current thread.
///
/// Leak checks take a snapshot before some work and compare it with one taken after disposing
/// that work's scope: every count should be back where it started. The counts are raw: `scopes`
/// includes the scope each memo and effect keeps for what its closure creates.
///
/// # Examples
///
/// ```
/// use artic_core::reactive::{Signal, live_counts};
///
/// let before = live_counts();
/// Signal::new(0);
/// assert_eq!(live_counts().signals, before.signals + 1);
/// ```
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct LiveCounts {
  /// Live signals.
  pub signals: usize,
  /// Live memos.
  pub memos: usize,
  /// Live effects.
  pub effects: usize,
  /// Live scopes.
  pub scopes: usize,
}

/// Returns how many reactive nodes are alive on the current thread.
///
/// See [`LiveCounts`].
pub fn live_counts() -> LiveCounts {
  Runtime::live_counts()
}

/// Panics if the reactive nodes alive now differ from `baseline`.
///
/// Take `baseline` with [`live_counts`] before the work under test, dispose that work's scope, then
/// call this. The panic message names each count that changed. Available in this crate's tests and
/// with the `testing` feature.
///
/// # Examples
///
/// ```
/// use artic_core::reactive::{Scope, Signal, assert_no_reactive_leaks, live_counts};
///
/// let baseline = live_counts();
/// let view = Scope::new_root();
/// view.run(|| Signal::new(0));
/// view.dispose();
/// assert_no_reactive_leaks(baseline);
/// ```
#[cfg(any(test, feature = "testing"))]
#[track_caller]
pub fn assert_no_reactive_leaks(baseline: LiveCounts) {
  let now = live_counts();
  let changes: Vec<String> = [
    ("signals", baseline.signals, now.signals),
    ("memos", baseline.memos, now.memos),
    ("effects", baseline.effects, now.effects),
    ("scopes", baseline.scopes, now.scopes),
  ]
  .into_iter()
  .filter(|(_, before, after)| before != after)
  .map(|(name, before, after)| format!("{name} {before} -> {after}"))
  .collect();
  assert!(changes.is_empty(), "reactive nodes leaked: {}", changes.join(", "));
}
