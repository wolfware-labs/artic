use artic_layout::geometry::{Rect, Size};

pub(crate) fn close(a: f32, b: f32, tolerance: f32) -> bool {
  if a.is_infinite() || b.is_infinite() {
    a == b
  } else {
    (a - b).abs() <= tolerance
  }
}

pub(crate) fn sizes_match(a: Size, b: Size, tolerance: f32) -> bool {
  close(a.width, b.width, tolerance) && close(a.height, b.height, tolerance)
}

pub(crate) fn rects_match(a: Rect, b: Rect, tolerance: f32) -> bool {
  close(a.x, b.x, tolerance)
    && close(a.y, b.y, tolerance)
    && close(a.width, b.width, tolerance)
    && close(a.height, b.height, tolerance)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::layout::TOLERANCE;

  // Values closer than the tolerance match; farther ones don't.
  #[test]
  fn rects_within_tolerance_match() {
    let rect = Rect::new(0.0, 20.0, 400.0, 30.0);

    assert!(rects_match(rect, Rect::new(0.0, 20.0005, 400.0, 30.0), TOLERANCE));
    assert!(!rects_match(rect, Rect::new(0.0, 20.01, 400.0, 30.0), TOLERANCE));
    assert!(sizes_match(Size::new(100.0, 20.0), Size::new(100.0009, 20.0), TOLERANCE));
  }

  // Infinity matches the same infinity and nothing else, however large.
  #[test]
  fn infinity_matches_only_infinity() {
    assert!(close(f32::INFINITY, f32::INFINITY, TOLERANCE));
    assert!(!close(f32::INFINITY, f32::MAX, TOLERANCE));
    assert!(!close(f32::INFINITY, f32::NEG_INFINITY, TOLERANCE));
    assert!(!close(1.0, f32::INFINITY, TOLERANCE));
  }

  // An infinite tolerance still matches infinity only to itself.
  #[test]
  fn infinity_matches_only_infinity_under_any_tolerance() {
    assert!(close(f32::INFINITY, f32::INFINITY, f32::INFINITY));
    assert!(!close(f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY));
    assert!(!close(1.0, f32::INFINITY, f32::INFINITY));
  }

  // A NaN never matches, not even another NaN, so a NaN in a result always fails.
  #[test]
  fn nan_never_matches() {
    assert!(!close(f32::NAN, f32::NAN, TOLERANCE));
    assert!(!close(f32::NAN, 0.0, TOLERANCE));
    assert!(!close(0.0, f32::NAN, f32::INFINITY));
  }
}
