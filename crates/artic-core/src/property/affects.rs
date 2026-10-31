use std::ops::{BitOr, BitOrAssign};

/// Which frame stages a change has to redo: layout, rendering, or none.
///
/// A [`PropertyKey`](crate::property::PropertyKey) declares what a change to its property affects,
/// and a node's dirty flags use the same type to record what it needs redone. Combine values with
/// `|`.
///
/// # Examples
///
/// ```
/// use artic_core::property::Affects;
///
/// let both = Affects::LAYOUT | Affects::RENDER;
/// assert!(both.contains(Affects::LAYOUT));
/// assert!(!Affects::RENDER.contains(Affects::LAYOUT));
/// assert!(Affects::NONE.is_empty());
/// ```
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub struct Affects(u8);

impl Affects {
  /// Nothing has to be redone.
  pub const NONE: Affects = Affects(0);
  /// Size or position may change, so the node has to be measured and arranged again.
  pub const LAYOUT: Affects = Affects(1);
  /// Only the pixels change, so the node has to be drawn again.
  pub const RENDER: Affects = Affects(1 << 1);

  /// Returns `true` if every stage in `other` is also in `self`.
  #[must_use]
  pub const fn contains(self, other: Affects) -> bool {
    self.0 & other.0 == other.0
  }

  /// Returns the stages in `self` or in `other`, like `|` but usable in a `const` or `static`.
  #[must_use]
  pub const fn union(self, other: Affects) -> Affects {
    Affects(self.0 | other.0)
  }

  /// Returns `self` with the stages in `other` removed.
  #[must_use]
  pub const fn without(self, other: Affects) -> Affects {
    Affects(self.0 & !other.0)
  }

  /// Returns `true` if no stage is set.
  #[must_use]
  pub const fn is_empty(self) -> bool {
    self.0 == 0
  }
}

impl BitOr for Affects {
  type Output = Affects;

  fn bitor(self, other: Affects) -> Affects {
    self.union(other)
  }
}

impl BitOrAssign for Affects {
  fn bitor_assign(&mut self, other: Affects) {
    *self = self.union(other);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // A union keeps a stage both sides have, and every stage either side has.
  #[test]
  fn union_keeps_the_stages_both_sides_have() {
    let merged = Affects::LAYOUT.union(Affects::RENDER).union(Affects::LAYOUT);

    assert!(merged.contains(Affects::LAYOUT));
    assert!(merged.contains(Affects::RENDER));
  }
}
