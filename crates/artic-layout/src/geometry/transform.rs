use crate::geometry::Point;
use std::fmt::{Display, Formatter, Result};

/// A 2D affine transform: it maps a point `(x, y)` to
/// `(xx·x + xy·y + dx, yx·x + yy·y + dy)`.
///
/// The fields are the six entries of the matrix in the order CSS's `matrix(a, b, c, d, e, f)`
/// lists them: `xx, yx, xy, yy, dx, dy`.
///
/// # Examples
///
/// ```
/// use artic_layout::geometry::{Point, Transform};
///
/// let transform = Transform::scale(2.0, 2.0).then(Transform::translate(10.0, 0.0));
/// assert_eq!(transform.apply(Point::new(1.0, 1.0)), Point::new(12.0, 2.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
  /// How much `x` contributes to the new `x`.
  pub xx: f32,
  /// How much `x` contributes to the new `y`.
  pub yx: f32,
  /// How much `y` contributes to the new `x`.
  pub xy: f32,
  /// How much `y` contributes to the new `y`.
  pub yy: f32,
  /// The horizontal offset.
  pub dx: f32,
  /// The vertical offset.
  pub dy: f32,
}

impl Transform {
  /// Leaves every point where it is.
  pub const IDENTITY: Transform = Transform::scale(1.0, 1.0);

  /// Moves points by `dx`, `dy`.
  #[must_use]
  pub const fn translate(dx: f32, dy: f32) -> Transform {
    Transform {
      dx,
      dy,
      ..Transform::IDENTITY
    }
  }

  /// Scales points by `x` horizontally and `y` vertically, around the origin.
  #[must_use]
  pub const fn scale(x: f32, y: f32) -> Transform {
    Transform {
      xx: x,
      yx: 0.0,
      xy: 0.0,
      yy: y,
      dx: 0.0,
      dy: 0.0,
    }
  }

  /// Rotates points by `radians` around the origin, clockwise on screen, since `y` grows downward.
  #[must_use]
  pub fn rotate(radians: f32) -> Transform {
    let (sin, cos) = radians.sin_cos();
    Transform {
      xx: cos,
      yx: sin,
      xy: -sin,
      yy: cos,
      dx: 0.0,
      dy: 0.0,
    }
  }

  /// The transform that applies `self`, then `next`.
  #[must_use]
  pub fn then(self, next: Transform) -> Transform {
    Transform {
      xx: next.xx * self.xx + next.xy * self.yx,
      yx: next.yx * self.xx + next.yy * self.yx,
      xy: next.xx * self.xy + next.xy * self.yy,
      yy: next.yx * self.xy + next.yy * self.yy,
      dx: next.xx * self.dx + next.xy * self.dy + next.dx,
      dy: next.yx * self.dx + next.yy * self.dy + next.dy,
    }
  }

  /// Where `point` goes.
  #[must_use]
  pub fn apply(self, point: Point) -> Point {
    Point::new(
      self.xx * point.x + self.xy * point.y + self.dx,
      self.yx * point.x + self.yy * point.y + self.dy,
    )
  }
}

impl Default for Transform {
  fn default() -> Transform {
    Transform::IDENTITY
  }
}

impl Display for Transform {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
    let Transform { xx, yx, xy, yy, dx, dy } = self;
    write!(formatter, "matrix({xx}, {yx}, {xy}, {yy}, {dx}, {dy})")
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use std::f32::consts::FRAC_PI_2;

  // The identity and the default leave points where they are.
  #[test]
  fn identity_keeps_points() {
    let point = Point::new(3.0, -4.0);

    assert_eq!(Transform::IDENTITY.apply(point), point);
    assert_eq!(Transform::default(), Transform::IDENTITY);
  }

  // Translation adds the offset; scaling multiplies around the origin.
  #[test]
  fn translate_and_scale() {
    let point = Point::new(3.0, 4.0);

    assert_eq!(Transform::translate(1.0, 2.0).apply(point), Point::new(4.0, 6.0));
    assert_eq!(Transform::scale(2.0, 0.5).apply(point), Point::new(6.0, 2.0));
  }

  // A quarter turn moves the x axis onto the y axis, which points down on screen.
  #[test]
  fn rotate_is_clockwise_on_screen() {
    let rotated = Transform::rotate(FRAC_PI_2).apply(Point::new(1.0, 0.0));

    assert!((rotated.x - 0.0).abs() < 1e-6 && (rotated.y - 1.0).abs() < 1e-6, "{rotated}");
  }

  // `then` applies the first transform, then the second; the order matters.
  #[test]
  fn then_applies_in_order() {
    let point = Point::new(1.0, 1.0);
    let scale = Transform::scale(2.0, 2.0);
    let translate = Transform::translate(10.0, 0.0);

    assert_eq!(scale.then(translate).apply(point), Point::new(12.0, 2.0));
    assert_eq!(translate.then(scale).apply(point), Point::new(22.0, 2.0));
    assert_eq!(scale.then(translate).apply(point), translate.apply(scale.apply(point)));
  }

  // Composing uses every component of both transforms: each point goes where applying one, then
  // the other, sends it.
  #[test]
  fn then_composes_every_component() {
    let first = Transform {
      xx: 2.0,
      yx: 3.0,
      xy: 4.0,
      yy: 5.0,
      dx: 6.0,
      dy: 7.0,
    };
    let next = Transform {
      xx: 7.0,
      yx: 8.0,
      xy: 9.0,
      yy: 10.0,
      dx: 11.0,
      dy: 12.0,
    };

    for point in [Point::new(0.0, 0.0), Point::new(1.0, 0.0), Point::new(0.0, 1.0)] {
      assert_eq!(first.then(next).apply(point), next.apply(first.apply(point)));
    }
  }

  // A transform prints in CSS's `matrix(...)` order.
  #[test]
  fn display_is_css_matrix() {
    assert_eq!(Transform::translate(5.0, 6.0).to_string(), "matrix(1, 0, 0, 1, 5, 6)");
  }
}
