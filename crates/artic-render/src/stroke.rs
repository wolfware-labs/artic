use artic_layout::finite;

/// How a stroke outlines a shape: its width, and the shape of its joins and ends.
///
/// It's built with [`new`](Stroke::new) and the `with_*` methods, so fields added later break no
/// caller.
///
/// # Examples
///
/// ```
/// use artic_render::{LineCap, LineJoin, Stroke};
///
/// let rounded = Stroke::new(2.0).with_join(LineJoin::Round).with_cap(LineCap::Round);
/// assert_eq!(rounded.width, 2.0);
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Stroke {
  /// The line's width, centered on the outline.
  pub width: f32,
  /// How two segments meet.
  pub join: LineJoin,
  /// How both ends of an open figure are drawn. A closed figure has no ends.
  pub cap: LineCap,
  /// How long a miter join may grow, as a multiple of the width, before it's beveled instead.
  pub miter_limit: f32,
}

/// How two segments of a stroke meet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineJoin {
  /// The outer edges meet in a point, up to the miter limit.
  #[default]
  Miter,
  /// An arc joins the outer edges.
  Round,
  /// A straight line cuts across the outer edges.
  Bevel,
}

/// How a stroke ends an open figure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineCap {
  /// The stroke stops at the end point.
  #[default]
  Flat,
  /// A half circle reaches past the end point.
  Round,
  /// A half square reaches past the end point.
  Square,
}

const DEFAULT_MITER_LIMIT: f32 = 10.0;

impl Stroke {
  /// A stroke `width` wide, with miter joins, flat caps and a miter limit of 10.
  #[must_use]
  pub fn new(width: f32) -> Stroke {
    Stroke {
      width,
      join: LineJoin::Miter,
      cap: LineCap::Flat,
      miter_limit: DEFAULT_MITER_LIMIT,
    }
  }

  /// The same stroke with `join`.
  #[must_use]
  pub fn with_join(self, join: LineJoin) -> Stroke {
    Stroke { join, ..self }
  }

  /// The same stroke with `cap` at both ends.
  #[must_use]
  pub fn with_cap(self, cap: LineCap) -> Stroke {
    Stroke { cap, ..self }
  }

  /// The same stroke with `miter_limit`.
  #[must_use]
  pub fn with_miter_limit(self, miter_limit: f32) -> Stroke {
    Stroke { miter_limit, ..self }
  }

  /// The stroke as it's drawn, or `None` when it draws nothing: a width of `0`, negative, NaN or
  /// infinite. A finite miter limit below `1` draws as `1`, and a NaN or infinite one as `10`.
  #[must_use]
  pub fn drawable(&self) -> Option<Stroke> {
    finite::positive(self.width)?;
    let miter_limit = if self.miter_limit.is_finite() {
      self.miter_limit.max(1.0)
    } else {
      DEFAULT_MITER_LIMIT
    };
    Some(self.clone().with_miter_limit(miter_limit))
  }
}

impl Default for Stroke {
  /// A stroke 1 wide.
  fn default() -> Stroke {
    Stroke::new(1.0)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // The defaults are WPF's and Avalonia's, and each `with_*` sets only its own field.
  #[test]
  fn stroke_has_the_usual_defaults() {
    let stroke = Stroke::default();

    assert_eq!(
      (stroke.width, stroke.join, stroke.cap, stroke.miter_limit),
      (1.0, LineJoin::Miter, LineCap::Flat, 10.0)
    );
    assert_eq!(
      Stroke::new(2.0)
        .with_join(LineJoin::Round)
        .with_cap(LineCap::Square)
        .with_miter_limit(4.0),
      Stroke {
        width: 2.0,
        join: LineJoin::Round,
        cap: LineCap::Square,
        miter_limit: 4.0,
      }
    );
  }

  // A width that isn't a positive finite number draws nothing; a finite miter limit below 1 draws
  // as 1, and a NaN or infinite one as the default.
  #[test]
  fn stroke_drawable_follows_the_rules() {
    let miter = |limit| {
      Stroke::new(1.0)
        .with_miter_limit(limit)
        .drawable()
        .map(|stroke| stroke.miter_limit)
    };

    for width in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
      assert_eq!(Stroke::new(width).drawable(), None, "width {width}");
    }
    assert_eq!(
      [0.5, -3.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 4.0].map(miter),
      [Some(1.0), Some(1.0), Some(10.0), Some(10.0), Some(10.0), Some(4.0)]
    );
  }
}
