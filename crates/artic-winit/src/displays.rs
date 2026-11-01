//! The displays the host has seen, each with the id artic knows it by.

use artic_platform::DisplayId;
use std::collections::BTreeMap;

/// Each monitor the host has seen, by the windowing library's identity for it, with its display
/// id.
///
/// Generic over the monitor type `M`, so tests use their own.
#[derive(Debug)]
pub(crate) struct Displays<M> {
  /// The id of each monitor, and of `None`, a window on no monitor.
  ids: BTreeMap<Option<M>, DisplayId>,
}

impl<M> Default for Displays<M> {
  fn default() -> Self {
    Self { ids: BTreeMap::new() }
  }
}

impl<M: Ord> Displays<M> {
  /// The display id of `monitor`, the same for the same monitor; `None`, a window on no monitor,
  /// has an id of its own.
  pub(crate) fn id(&mut self, monitor: Option<M>) -> DisplayId {
    let next = DisplayId::new(self.ids.len() as u64);
    *self.ids.entry(monitor).or_insert(next)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // A monitor keeps its display id, and two monitors get two.
  #[test]
  fn monitors_get_stable_display_ids() {
    let mut displays = Displays::<u32>::default();

    let first = displays.id(Some(1));

    assert_eq!(displays.id(Some(1)), first);
    assert_ne!(displays.id(Some(2)), first);
  }

  // A window on no monitor has one display id of its own.
  #[test]
  fn no_monitor_is_one_display() {
    let mut displays = Displays::<u32>::default();

    let none = displays.id(None);

    assert_eq!(displays.id(None), none);
    assert_ne!(displays.id(Some(1)), none);
  }
}
