//! The number rules every layer shares, so a value means the same in layout, text and drawing:
//! addition that saturates instead of overflowing, the values that count as a usable length, and
//! reading a finite number from text.

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

/// `value` if it's finite, else 0: an offset or position, which may be negative, that NaN or an
/// infinity turns off.
#[must_use]
pub fn or_zero(value: f32) -> f32 {
  if value.is_finite() { value } else { 0.0 }
}

/// `length`, cut so that `start + length` stays finite: the length from `start` to the largest
/// finite coordinate when it would reach past it, a step shorter where that length rounds up.
#[must_use]
pub fn span(start: f32, length: f32) -> f32 {
  let cut = length.min(f32::MAX - start);
  if (start + cut).is_finite() { cut } else { cut.next_down() }
}

/// `value` if it's finite and above 0, else `None`: a length that only draws or shapes when it's
/// there, such as a stroke's width or a font size.
#[must_use]
pub fn positive(value: f32) -> Option<f32> {
  (value.is_finite() && value > 0.0).then_some(value)
}

const fn saturated(result: f32, finite_operands: bool) -> f32 {
  if finite_operands {
    result.clamp(f32::MIN, f32::MAX)
  } else {
    result
  }
}

/// Reads `text` as a finite number, as every value parser does: NaN and infinities are rejected,
/// since no length, angle or offset can be one.
///
/// # Errors
///
/// When `text` isn't a number, or is NaN or infinite.
pub fn parse(text: &str) -> Result<f32, NumberProblem> {
  let number: f32 = text.parse().map_err(|_| NumberProblem::NotANumber(text.to_owned()))?;
  if number.is_finite() {
    Ok(number)
  } else {
    Err(NumberProblem::NotFinite(text.to_owned()))
  }
}

/// What is wrong with text read as a number.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NumberProblem {
  /// It isn't a number.
  #[error("'{0}' is not a number")]
  NotANumber(String),
  /// It's NaN or infinite.
  #[error("'{0}' is not a finite number")]
  NotFinite(String),
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

  // `or_zero` keeps any finite value, negatives included, and makes NaN and infinities 0.
  #[test]
  fn or_zero_keeps_finite_values() {
    assert_eq!(or_zero(-2.5), -2.5);
    assert_eq!(or_zero(3.0), 3.0);
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
      assert_eq!(or_zero(value), 0.0, "or_zero({value})");
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

  // Text reads as a number only when it's one and it's finite, and the problem names the text.
  #[test]
  fn parse_reads_finite_numbers() {
    assert_eq!(parse("1.5"), Ok(1.5));
    assert_eq!(parse("-2"), Ok(-2.0));
    assert_eq!(parse("abc"), Err(NumberProblem::NotANumber("abc".to_owned())));
    assert_eq!(parse("NaN"), Err(NumberProblem::NotFinite("NaN".to_owned())));
    assert_eq!(parse("inf"), Err(NumberProblem::NotFinite("inf".to_owned())));
    assert_eq!(
      NumberProblem::NotANumber("abc".to_owned()).to_string(),
      "'abc' is not a number"
    );
    assert_eq!(
      NumberProblem::NotFinite("NaN".to_owned()).to_string(),
      "'NaN' is not a finite number"
    );
  }

  // Ordinary values add as usual.
  #[test]
  fn ordinary_values_are_unchanged() {
    assert_eq!(add(1.5, 2.0), 3.5);
  }

  // A span that would end past the largest finite coordinate is cut to end there; one that ends
  // before it, or starts below zero, keeps its length.
  #[test]
  fn span_ends_at_a_finite_coordinate() {
    assert_eq!(span(1.0e38, f32::MAX), 2.402_823_4e38);
    assert_eq!(span(10.0, 20.0), 20.0);
    assert_eq!(span(-f32::MAX, f32::MAX), f32::MAX);
    assert_eq!(span(f32::MAX, 10.0), 0.0);
  }

  // Where the room left rounds up, so that the cut length would end a half step past the largest
  // finite coordinate and round to infinity, the cut is a step shorter.
  #[test]
  fn span_never_rounds_past_the_largest_coordinate() {
    let start = 3.0 * 2f32.powi(103);

    assert!((start + span(start, f32::MAX)).is_finite());
  }
}
