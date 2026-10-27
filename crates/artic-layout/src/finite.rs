//! The number rules every layer shares, so a value means the same in layout, text and drawing:
//! addition that saturates instead of overflowing, and the values that count as a usable length.

/// `a + b`, saturating at `f32::MAX` or `f32::MIN` by sign when finite operands overflow: two
/// finite values never add up to an infinity, while an infinite operand, such as an infinite
/// available size, stays infinite.
#[must_use]
pub const fn add(a: f32, b: f32) -> f32 {
  saturated(a + b, a.is_finite() && b.is_finite())
}

/// `value` if it's finite and above 0, else 0: a length, spacing or radius that NaN, an infinity or
/// a negative value turns off.
#[must_use]
pub fn usable(value: f32) -> f32 {
  positive(value).unwrap_or(0.0)
}

/// `value` if it's finite and above 0, else `None`: a length that only draws or shapes when it's
/// there, such as a stroke's width or a font size.
#[must_use]
pub fn positive(value: f32) -> Option<f32> {
  (value.is_finite() && value > 0.0).then_some(value)
}

const fn saturated(result: f32, finite_operands: bool) -> f32 {
  if finite_operands && result.is_infinite() {
    if result > 0.0 { f32::MAX } else { f32::MIN }
  } else {
    result
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // Finite values that overflow saturate at the finite bound with the result's sign.
  #[test]
  fn finite_overflow_saturates_by_sign() {
    assert_eq!(add(f32::MAX, f32::MAX), f32::MAX);
    assert_eq!(add(f32::MIN, -f32::MAX), f32::MIN);
  }

  // An infinite operand isn't overflow: the result stays infinite.
  #[test]
  fn infinite_operands_stay_infinite() {
    assert_eq!(add(f32::INFINITY, -10.0), f32::INFINITY);
  }

  // `usable` keeps a finite value above 0 and makes everything else 0.
  #[test]
  fn usable_keeps_finite_positives_only() {
    for (value, expected) in [
      (2.0_f32, 2.0_f32),
      (0.0, 0.0),
      (-0.0, 0.0),
      (-1.0, 0.0),
      (f32::NAN, 0.0),
      (f32::INFINITY, 0.0),
      (f32::NEG_INFINITY, 0.0),
    ] {
      assert_eq!(usable(value).to_bits(), expected.to_bits(), "usable({value})");
    }
  }

  // `positive` is the value only when it's finite and above 0.
  #[test]
  fn positive_is_some_for_finite_positives_only() {
    assert_eq!(positive(2.0), Some(2.0));
    for value in [0.0, -0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
      assert_eq!(positive(value), None, "positive({value})");
    }
  }

  // Ordinary values add as usual.
  #[test]
  fn ordinary_values_are_unchanged() {
    assert_eq!(add(1.5, 2.0), 3.5);
  }
}
