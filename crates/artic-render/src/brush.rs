use crate::{Color, ParseColorError};
use artic_core::{RustExpr, ToRust};
use artic_layout::geometry::{RelativePoint, RelativeScalar};
use std::str::FromStr;
use std::sync::Arc;

/// The paint a shape is filled with: one color or a gradient.
///
/// Anything that converts into a brush fills where one is expected, so a [`Color`] fills solid:
/// `builder.fill(bounds, Color::STEEL_BLUE)`. Gradient points and radii are resolved against
/// the bounds of the shape being filled. Values out of range, such as an offset past 1 or a
/// negative radius, are clamped when the brush is drawn, never rejected: [`drawable`](Brush::drawable)
/// gives the [`Paint`](crate::Paint) every renderer draws.
#[derive(Debug, Clone, PartialEq)]
pub enum Brush {
  /// One color.
  Solid(SolidColorBrush),
  /// Colors blending along a line.
  Linear(LinearGradientBrush),
  /// Colors blending outward from a center, to an ellipse.
  Radial(RadialGradientBrush),
}

impl FromStr for Brush {
  type Err = ParseColorError;

  /// Parses color text, as [`Color`] does, into a solid brush.
  fn from_str(text: &str) -> Result<Brush, ParseColorError> {
    text.parse::<Color>().map(Brush::from).map_err(|error| ParseColorError {
      type_name: "Brush",
      ..error
    })
  }
}

impl From<Color> for Brush {
  fn from(color: Color) -> Brush {
    Brush::Solid(SolidColorBrush::new(color))
  }
}

impl From<SolidColorBrush> for Brush {
  fn from(brush: SolidColorBrush) -> Brush {
    Brush::Solid(brush)
  }
}

impl From<LinearGradientBrush> for Brush {
  fn from(brush: LinearGradientBrush) -> Brush {
    Brush::Linear(brush)
  }
}

impl From<RadialGradientBrush> for Brush {
  fn from(brush: RadialGradientBrush) -> Brush {
    Brush::Radial(brush)
  }
}

/// A brush of one color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolidColorBrush {
  /// The color.
  pub color: Color,
  /// Multiplies the color's alpha, from 0 to 1.
  pub opacity: f32,
}

impl SolidColorBrush {
  /// A brush of `color`, at opacity 1.
  #[must_use]
  pub const fn new(color: Color) -> SolidColorBrush {
    SolidColorBrush { color, opacity: 1.0 }
  }
}

impl Default for SolidColorBrush {
  fn default() -> SolidColorBrush {
    SolidColorBrush::new(Color::default())
  }
}

/// Colors blending along the line from `start` to `end`, and holding the end colors beyond it.
///
/// The default runs from `0%,0%` to `100%,100%`, corner to corner of the box being filled.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradientBrush {
  /// Where offset 0 is.
  pub start: RelativePoint,
  /// Where offset 1 is.
  pub end: RelativePoint,
  /// The colors, and where along the line each is.
  pub stops: Arc<[GradientStop]>,
  /// Multiplies the alpha of every stop, from 0 to 1.
  pub opacity: f32,
}

impl LinearGradientBrush {
  /// A gradient from `start` to `end` through `stops`, at opacity 1.
  #[must_use]
  pub fn new(start: RelativePoint, end: RelativePoint, stops: impl Into<Arc<[GradientStop]>>) -> LinearGradientBrush {
    LinearGradientBrush {
      start,
      end,
      stops: stops.into(),
      opacity: 1.0,
    }
  }
}

impl Default for LinearGradientBrush {
  fn default() -> LinearGradientBrush {
    LinearGradientBrush::new(
      RelativePoint::relative(0.0, 0.0),
      RelativePoint::relative(1.0, 1.0),
      Vec::new(),
    )
  }
}

/// Colors blending outward from `center` to an ellipse of radii `radius_x` and `radius_y`, and
/// holding the last color beyond it.
///
/// A relative `radius_x` is a fraction of the filled box's width, and `radius_y` of its height.
/// The default is centered at `50%,50%` with radii of `50%`: an ellipse touching every side.
#[derive(Debug, Clone, PartialEq)]
pub struct RadialGradientBrush {
  /// Where offset 0 is.
  pub center: RelativePoint,
  /// The horizontal radius, where offset 1 is.
  pub radius_x: RelativeScalar,
  /// The vertical radius, where offset 1 is.
  pub radius_y: RelativeScalar,
  /// The colors, and where from the center out each is.
  pub stops: Arc<[GradientStop]>,
  /// Multiplies the alpha of every stop, from 0 to 1.
  pub opacity: f32,
}

impl RadialGradientBrush {
  /// A gradient from `center` out to the ellipse of these radii through `stops`, at opacity 1.
  #[must_use]
  pub fn new(
    center: RelativePoint,
    radius_x: RelativeScalar,
    radius_y: RelativeScalar,
    stops: impl Into<Arc<[GradientStop]>>,
  ) -> RadialGradientBrush {
    RadialGradientBrush {
      center,
      radius_x,
      radius_y,
      stops: stops.into(),
      opacity: 1.0,
    }
  }
}

impl Default for RadialGradientBrush {
  fn default() -> RadialGradientBrush {
    RadialGradientBrush::new(
      RelativePoint::relative(0.5, 0.5),
      RelativeScalar::relative(0.5),
      RelativeScalar::relative(0.5),
      Vec::new(),
    )
  }
}

/// A color at a point along a gradient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
  /// Where the color is, from 0 at the gradient's start to 1 at its end.
  pub offset: f32,
  /// The color there.
  pub color: Color,
}

impl GradientStop {
  /// The stop of `color` at `offset`.
  #[must_use]
  pub const fn new(offset: f32, color: Color) -> GradientStop {
    GradientStop { offset, color }
  }
}

/// A gradient's stops, written as the `Arc` the field holds.
struct Stops<'a>(&'a [GradientStop]);

impl ToRust for Stops<'_> {
  fn to_rust(&self, out: &mut RustExpr) {
    out.raw("::std::sync::Arc::from(");
    self.0.to_rust(out);
    out.raw(")");
  }
}

impl ToRust for Brush {
  fn to_rust(&self, out: &mut RustExpr) {
    let (variant, brush): (&str, &dyn ToRust) = match self {
      Brush::Solid(brush) => ("Brush::Solid", brush),
      Brush::Linear(brush) => ("Brush::Linear", brush),
      Brush::Radial(brush) => ("Brush::Radial", brush),
    };
    out.call(env!("CARGO_CRATE_NAME"), variant, &[brush]);
  }
}

impl ToRust for SolidColorBrush {
  fn to_rust(&self, out: &mut RustExpr) {
    let SolidColorBrush { color, opacity } = self;
    out.struct_literal(
      env!("CARGO_CRATE_NAME"),
      "SolidColorBrush",
      &[("color", color), ("opacity", opacity)],
    );
  }
}

impl ToRust for LinearGradientBrush {
  fn to_rust(&self, out: &mut RustExpr) {
    let LinearGradientBrush {
      start,
      end,
      stops,
      opacity,
    } = self;
    out.struct_literal(
      env!("CARGO_CRATE_NAME"),
      "LinearGradientBrush",
      &[("start", start), ("end", end), ("stops", &Stops(stops)), ("opacity", opacity)],
    );
  }
}

impl ToRust for RadialGradientBrush {
  fn to_rust(&self, out: &mut RustExpr) {
    let RadialGradientBrush {
      center,
      radius_x,
      radius_y,
      stops,
      opacity,
    } = self;
    out.struct_literal(
      env!("CARGO_CRATE_NAME"),
      "RadialGradientBrush",
      &[
        ("center", center),
        ("radius_x", radius_x),
        ("radius_y", radius_y),
        ("stops", &Stops(stops)),
        ("opacity", opacity),
      ],
    );
  }
}

impl ToRust for GradientStop {
  fn to_rust(&self, out: &mut RustExpr) {
    out.call(env!("CARGO_CRATE_NAME"), "GradientStop::new", &[&self.offset, &self.color]);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;

  // Color text parses into a solid brush, and a failure names Brush.
  #[test]
  fn brush_from_color_string_is_solid() {
    assert_eq!("SteelBlue".parse(), Ok(Brush::Solid(SolidColorBrush::new(Color::STEEL_BLUE))));
    assert_eq!("#3B82F6".parse(), Ok(Brush::from(Color::rgb(0x3B, 0x82, 0xF6))));
    assert_eq!(
      "redd".parse::<Brush>().expect_err("unknown").to_string(),
      r##"invalid Brush 'redd': 'redd' is not a color name; expected "#RGB", "#ARGB", "#RRGGBB", "#AARRGGBB" or a color name"##
    );
  }

  // A brush built from its stops alone fills its box, at full opacity.
  #[test]
  fn brush_defaults_fill_the_box() {
    let linear = LinearGradientBrush::default();
    let radial = RadialGradientBrush::default();

    assert_eq!(
      (linear.start, linear.end, linear.opacity),
      (RelativePoint::relative(0.0, 0.0), RelativePoint::relative(1.0, 1.0), 1.0)
    );
    assert_eq!(
      (radial.center, radial.radius_x, radial.radius_y, radial.opacity),
      (
        RelativePoint::relative(0.5, 0.5),
        RelativeScalar::relative(0.5),
        RelativeScalar::relative(0.5),
        1.0
      )
    );
    assert_eq!(SolidColorBrush::default().opacity, 1.0);
    assert!(linear.stops.is_empty() && radial.stops.is_empty());
  }
}
