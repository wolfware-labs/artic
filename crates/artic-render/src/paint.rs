use crate::{Brush, Color, GradientStop};
use artic_layout::finite;
use artic_layout::geometry::{Point, Rect};

/// A [`Brush`] resolved against the bounds of what it fills, as every renderer draws it: what
/// [`Brush::drawable`] returns.
///
/// A `Paint` from `drawable` is always drawable: points are finite, radii finite and above `0`, a
/// line's ends apart, a gradient has at least two stops, with offsets in `0..=1` and sorted, and
/// `opacity` is in `0..=1`. A renderer maps it to its own paint without checking anything. Every
/// renderer must support every variant, so a new variant is a breaking change on purpose.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
  /// One color.
  Solid {
    /// The color.
    color: Color,
    /// Multiplies the color's alpha.
    opacity: f32,
  },
  /// Colors blending along the line from `start` to `end`.
  Linear {
    /// Where the first stop's color is.
    start: Point,
    /// Where the last stop's color is.
    end: Point,
    /// The colors and where along the line they are, from `0` at `start` to `1` at `end`.
    stops: Vec<GradientStop>,
    /// Multiplies every stop's alpha.
    opacity: f32,
  },
  /// Colors blending outward from `center` to the ellipse of the two radii.
  Radial {
    /// Where the first stop's color is.
    center: Point,
    /// The ellipse's horizontal radius.
    radius_x: f32,
    /// The ellipse's vertical radius.
    radius_y: f32,
    /// The colors and where they are, from `0` at the center to `1` on the ellipse.
    stops: Vec<GradientStop>,
    /// Multiplies every stop's alpha.
    opacity: f32,
  },
}

impl Brush {
  /// The brush as it fills `bounds`, or `None` when it paints nothing.
  ///
  /// Relative points and radii resolve against `bounds`. Values out of range are clamped, as WPF
  /// does: offsets and opacity to `0..=1`, radii to `0` or more, NaN counting as `0`. Stops are
  /// sorted by offset, keeping their written order among equal offsets, so two stops at one offset
  /// make a hard edge. A gradient with no stops paints nothing; one with a single stop, or with
  /// nothing to spread over (ends that meet, a zero radius, a coordinate that isn't finite), fills
  /// with its last color.
  #[must_use]
  pub fn drawable(&self, bounds: Rect) -> Option<Paint> {
    let (stops, opacity, geometry) = match self {
      Brush::Solid(solid) => {
        return Some(Paint::Solid {
          color: solid.color,
          opacity: unit(solid.opacity),
        });
      }
      Brush::Linear(linear) => {
        let (start, end) = (linear.start.resolve(bounds), linear.end.resolve(bounds));
        let geometry = (finite(start) && finite(end) && start != end).then_some(Geometry::Linear { start, end });
        (&linear.stops, linear.opacity, geometry)
      }
      Brush::Radial(radial) => {
        let center = radial.center.resolve(bounds);
        let radius_x = finite::usable(radial.radius_x.resolve(bounds.width));
        let radius_y = finite::usable(radial.radius_y.resolve(bounds.height));
        let geometry = (finite(center) && radius_x > 0.0 && radius_y > 0.0).then_some(Geometry::Radial {
          center,
          radius_x,
          radius_y,
        });
        (&radial.stops, radial.opacity, geometry)
      }
    };
    let opacity = unit(opacity);
    let mut stops: Vec<GradientStop> = stops
      .iter()
      .map(|stop| GradientStop::new(unit(stop.offset), stop.color))
      .collect();
    stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    let color = stops.last()?.color;
    Some(match geometry.filter(|_| stops.len() > 1) {
      Some(Geometry::Linear { start, end }) => Paint::Linear {
        start,
        end,
        stops,
        opacity,
      },
      Some(Geometry::Radial {
        center,
        radius_x,
        radius_y,
      }) => Paint::Radial {
        center,
        radius_x,
        radius_y,
        stops,
        opacity,
      },
      None => Paint::Solid { color, opacity },
    })
  }
}

/// Where a gradient lays its stops, once that is known to be drawable.
enum Geometry {
  Linear { start: Point, end: Point },
  Radial { center: Point, radius_x: f32, radius_y: f32 },
}

/// `value` in `0..=1`, NaN counting as `0`.
fn unit(value: f32) -> f32 {
  if value > 0.0 { value.min(1.0) } else { 0.0 }
}

fn finite(point: Point) -> bool {
  point.x.is_finite() && point.y.is_finite()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{LinearGradientBrush, RadialGradientBrush, SolidColorBrush};
  use artic_layout::geometry::{RelativePoint, RelativeScalar};
  use pretty_assertions::assert_eq;

  const RED: Color = Color::rgb(255, 0, 0);
  const GREEN: Color = Color::rgb(0, 255, 0);
  const BLUE: Color = Color::rgb(0, 0, 255);

  fn red_to_blue() -> [GradientStop; 2] {
    [GradientStop::new(0.0, RED), GradientStop::new(1.0, BLUE)]
  }

  fn solid(color: Color) -> Option<Paint> {
    Some(Paint::Solid { color, opacity: 1.0 })
  }

  // Each brush resolves against the rectangle: its points and radii become absolute, its opacity
  // stays beside the colors.
  #[test]
  fn brushes_resolve_against_the_bounds() {
    let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
    let linear = LinearGradientBrush {
      opacity: 0.5,
      ..LinearGradientBrush::new(
        RelativePoint::relative(0.0, 0.5),
        RelativePoint::fixed(100.0, 25.0),
        red_to_blue(),
      )
    };
    let radial = RadialGradientBrush::new(
      RelativePoint::relative(0.5, 0.5),
      RelativeScalar::relative(0.5),
      RelativeScalar::fixed(10.0),
      red_to_blue(),
    );

    assert_eq!(
      Brush::from(SolidColorBrush {
        color: RED,
        opacity: 0.5
      })
      .drawable(rect),
      Some(Paint::Solid {
        color: RED,
        opacity: 0.5
      })
    );
    assert_eq!(
      Brush::from(linear).drawable(rect),
      Some(Paint::Linear {
        start: Point::new(10.0, 45.0),
        end: Point::new(110.0, 45.0),
        stops: red_to_blue().to_vec(),
        opacity: 0.5,
      })
    );
    assert_eq!(
      Brush::from(radial).drawable(rect),
      Some(Paint::Radial {
        center: Point::new(60.0, 45.0),
        radius_x: 50.0,
        radius_y: 10.0,
        stops: red_to_blue().to_vec(),
        opacity: 1.0,
      })
    );
  }

  // Offsets and opacity are clamped into 0..=1 and radii to 0 or more, NaN counting as 0, and
  // stops are sorted by offset, keeping their written order among equal offsets.
  #[test]
  fn gradient_values_are_clamped_and_sorted() {
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    let unordered = [
      GradientStop::new(2.0, BLUE),
      GradientStop::new(f32::NAN, RED),
      GradientStop::new(-1.0, GREEN),
    ];
    let linear = |opacity| {
      Brush::from(LinearGradientBrush {
        opacity,
        ..LinearGradientBrush::new(
          RelativePoint::relative(0.0, 0.0),
          RelativePoint::relative(1.0, 0.0),
          unordered,
        )
      })
    };
    let expected = |opacity: f32| {
      Some(Paint::Linear {
        start: Point::new(0.0, 0.0),
        end: Point::new(10.0, 0.0),
        stops: vec![
          GradientStop::new(0.0, RED),
          GradientStop::new(0.0, GREEN),
          GradientStop::new(1.0, BLUE),
        ],
        opacity,
      })
    };

    assert_eq!(linear(2.0).drawable(rect), expected(1.0));
    assert_eq!(linear(f32::NAN).drawable(rect), expected(0.0));
    assert_eq!(linear(-1.0).drawable(rect), expected(0.0));
    for radius in [-1.0, f32::NAN] {
      let radial = RadialGradientBrush {
        radius_y: RelativeScalar::fixed(radius),
        stops: red_to_blue().into(),
        ..RadialGradientBrush::default()
      };
      assert_eq!(Brush::from(radial).drawable(rect), solid(BLUE), "radius {radius}");
    }
  }

  // A gradient with nothing to spread over fills with its last color: a zero radius, a box with
  // no width, or a line whose ends meet. One stop fills with its color, and no stops paint nothing.
  #[test]
  fn degenerate_gradients_fill_with_their_last_color() {
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    let radial = RadialGradientBrush {
      stops: red_to_blue().into(),
      ..RadialGradientBrush::default()
    };
    let zero = RadialGradientBrush {
      radius_x: RelativeScalar::fixed(0.0),
      ..radial.clone()
    };
    let meeting = LinearGradientBrush::new(
      RelativePoint::relative(0.5, 0.5),
      RelativePoint::fixed(5.0, 5.0),
      red_to_blue(),
    );
    let single = LinearGradientBrush::new(
      RelativePoint::relative(0.0, 0.0),
      RelativePoint::relative(1.0, 0.0),
      [GradientStop::new(0.3, BLUE)],
    );

    assert_eq!(Brush::from(zero).drawable(rect), solid(BLUE));
    assert_eq!(Brush::from(radial).drawable(Rect::new(0.0, 0.0, 0.0, 10.0)), solid(BLUE));
    assert_eq!(Brush::from(meeting).drawable(rect), solid(BLUE));
    assert_eq!(Brush::from(single).drawable(rect), solid(BLUE));
    assert_eq!(Brush::from(LinearGradientBrush::default()).drawable(rect), None);
  }

  // Geometry that isn't finite, which a brush built in code can carry, fills with the last color
  // too, so no renderer is handed a NaN or infinite gradient.
  #[test]
  fn non_finite_gradients_fill_with_their_last_color() {
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    let linear = |start, end| Brush::from(LinearGradientBrush::new(start, end, red_to_blue()));
    let radial = RadialGradientBrush {
      stops: red_to_blue().into(),
      ..RadialGradientBrush::default()
    };
    let infinite_radius = RadialGradientBrush {
      radius_x: RelativeScalar::fixed(f32::INFINITY),
      ..radial.clone()
    };
    let nan_center = RadialGradientBrush {
      center: RelativePoint::relative(0.5, f32::NAN),
      ..radial
    };

    assert_eq!(
      linear(RelativePoint::relative(f32::NAN, 0.0), RelativePoint::relative(1.0, 0.0)).drawable(rect),
      solid(BLUE)
    );
    assert_eq!(
      linear(RelativePoint::relative(0.0, 0.0), RelativePoint::fixed(f32::INFINITY, 0.0)).drawable(rect),
      solid(BLUE)
    );
    assert_eq!(Brush::from(infinite_radius).drawable(rect), solid(BLUE));
    assert_eq!(Brush::from(nan_center).drawable(rect), solid(BLUE));
  }
}
