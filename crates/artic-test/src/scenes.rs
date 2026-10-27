//! One scene per drawing primitive, the same for every renderer, so each renderer keeps goldens
//! of the same pictures.

use crate::text::engine;
use artic_core::Object;
use artic_core::property::PropertyVisitor;
use artic_core::tree::{NodeId, UiTree};
use artic_layout::geometry::{CornerRadius, FillRule, Path, Point, Rect, RelativePoint, RelativeScalar, RoundedRect};
use artic_layout::text::{FontFamily, ParagraphStyle, TextEngine, TextLimits, TextStyle, TextWrapping};
use artic_render::{
  BoxShadow, Color, Compositor, GradientStop, Layers, LineJoin, LinearGradientBrush, Nodes, PhysicalSize, RadialGradientBrush,
  Scene, SceneBuilder, Stroke,
};
use std::f32::consts::{FRAC_PI_2, PI};

/// The size every scene is drawn at, at scale 1.
pub const SIZE: PhysicalSize = PhysicalSize { width: 64, height: 64 };

const WHITE: Color = Color::rgb(255, 255, 255);
const BLUE: Color = Color::rgb(59, 130, 246);
const PINK: Color = Color::rgb(236, 72, 153);
const YELLOW: Color = Color::rgb(250, 204, 21);

/// The scene `draw` records, as one node of a tree.
pub fn scene(draw: impl FnOnce(&mut SceneBuilder)) -> Scene {
  let mut tree = UiTree::new();
  let root = tree.alloc(Box::new(Node));
  Compositor::default().compose(&mut tree, root, &mut Drawing(Some(draw)))
}

/// A rounded rectangle with four different radii.
#[must_use]
pub fn rounded_rect_fill() -> Scene {
  scene(|builder| {
    let radii = CornerRadius::new(0.0, 8.0, 16.0, 24.0);
    builder.fill(RoundedRect::new(Rect::new(4.0, 4.0, 56.0, 56.0), radii), BLUE);
  })
}

/// A star stroked with round joins.
#[must_use]
pub fn stroke() -> Scene {
  scene(|builder| builder.stroke(star(), PINK, Stroke::new(4.0).with_join(LineJoin::Round)))
}

/// A star filled even-odd, so its middle stays clear.
#[must_use]
pub fn path() -> Scene {
  scene(|builder| builder.fill(star(), BLUE))
}

/// A square with a diagonal linear gradient of three stops.
#[must_use]
pub fn linear_gradient() -> Scene {
  scene(|builder| builder.fill(Rect::new(4.0, 4.0, 56.0, 56.0), gradient()))
}

/// A wide rectangle with a radial gradient, an ellipse since its radii are half its width and
/// half its height.
#[must_use]
pub fn radial_gradient() -> Scene {
  scene(|builder| {
    let stops = [GradientStop::new(0.0, YELLOW), GradientStop::new(1.0, BLUE)];
    let half = RelativeScalar::relative(0.5);
    let brush = RadialGradientBrush::new(RelativePoint::relative(0.5, 0.5), half, half, stops);
    builder.fill(Rect::new(4.0, 12.0, 56.0, 40.0), brush);
  })
}

/// A translucent card over its blurred shadow with spread, on white: the shadow shows only
/// around the card.
#[must_use]
pub fn box_shadow() -> Scene {
  scene(|builder| {
    builder.fill(Rect::new(0.0, 0.0, 64.0, 64.0), WHITE);
    let card = RoundedRect::new(Rect::new(16.0, 12.0, 32.0, 32.0), CornerRadius::uniform(6.0));
    builder.box_shadow(
      card,
      BoxShadow::new(Point::new(0.0, 4.0), 8.0, Color::rgba(15, 23, 42, 128)).with_spread(2.0),
    );
    builder.fill(card, Color::rgba(59, 130, 246, 128));
  })
}

/// The linear gradient over the whole scene, clipped by a rectangle.
#[must_use]
pub fn rect_clip() -> Scene {
  scene(|builder| builder.with_clip(Rect::new(12.0, 12.0, 40.0, 40.0), fill_gradient))
}

/// The linear gradient over the whole scene, clipped by a circle.
#[must_use]
pub fn rounded_rect_clip() -> Scene {
  scene(|builder| {
    let circle = RoundedRect::new(Rect::new(4.0, 4.0, 56.0, 56.0), CornerRadius::uniform(28.0));
    builder.with_clip(circle, fill_gradient);
  })
}

/// The linear gradient over the whole scene, clipped by a star.
#[must_use]
pub fn path_clip() -> Scene {
  scene(|builder| builder.with_clip(star(), fill_gradient))
}

/// Two overlapping squares, the second inside an opacity layer nested in the first's.
#[must_use]
pub fn nested_opacity() -> Scene {
  scene(|builder| {
    builder.with_opacity(0.8, |builder| {
      builder.fill(Rect::new(4.0, 4.0, 36.0, 36.0), BLUE);
      builder.with_opacity(0.5, |builder| builder.fill(Rect::new(24.0, 24.0, 36.0, 36.0), PINK));
    });
  })
}

/// "Text" in the test font at 20 px, in solid blue.
#[must_use]
pub fn text_solid() -> Scene {
  let paragraph = engine().paragraph(
    "Text",
    &TextStyle::new(FontFamily::default(), 20.0),
    &ParagraphStyle::default(),
    TextLimits::NONE,
  );
  scene(|builder| builder.text(&paragraph, Point::new(4.0, 4.0), BLUE))
}

/// Two lines of the test font at 16 px, painted with one diagonal gradient across both.
#[must_use]
pub fn text_gradient() -> Scene {
  let style = TextStyle::new(FontFamily::default(), 16.0);
  let paragraph = engine().paragraph("Grad ient", &style, &overflowing(), TextLimits::width(56.0));
  scene(|builder| builder.text(&paragraph, Point::new(4.0, 4.0), gradient()))
}

/// A sentence of the test font at 12 px, wrapped to 56 px, in slate.
#[must_use]
pub fn text_wrapped() -> Scene {
  let style = TextStyle::new(FontFamily::default(), 12.0);
  let paragraph = engine().paragraph("Wrapped to the width", &style, &overflowing(), TextLimits::width(56.0));
  scene(|builder| builder.text(&paragraph, Point::new(4.0, 4.0), Color::rgb(51, 65, 85)))
}

fn overflowing() -> ParagraphStyle {
  ParagraphStyle {
    wrapping: TextWrapping::WrapWithOverflow,
    ..ParagraphStyle::default()
  }
}

fn gradient() -> LinearGradientBrush {
  let stops = [
    GradientStop::new(0.0, BLUE),
    GradientStop::new(0.5, YELLOW),
    GradientStop::new(1.0, PINK),
  ];
  LinearGradientBrush::new(RelativePoint::relative(0.0, 0.0), RelativePoint::relative(1.0, 1.0), stops)
}

fn fill_gradient(builder: &mut SceneBuilder) {
  builder.fill(Rect::new(0.0, 0.0, 64.0, 64.0), gradient());
}

fn star() -> Path {
  let point = |index: u8| {
    let angle = f32::from(index) * 4.0 * PI / 5.0 - FRAC_PI_2;
    Point::new(32.0 + 28.0 * angle.cos(), 32.0 + 28.0 * angle.sin())
  };
  (1..5)
    .fold(Path::builder(point(0)), |path, index| path.line_to(point(index)))
    .close()
    .fill_rule(FillRule::EvenOdd)
    .build()
}

#[derive(Debug)]
struct Node;

impl Object for Node {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

struct Drawing<F>(Option<F>);

impl<F: FnOnce(&mut SceneBuilder)> Nodes for Drawing<F> {
  fn layers(&self, _arena: &UiTree, _node: NodeId) -> Layers {
    Layers::default()
  }

  fn record(&mut self, _arena: &UiTree, _node: NodeId, builder: &mut SceneBuilder) {
    if let Some(draw) = self.0.take() {
      draw(builder);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use artic_render::{Command, SceneItem};

  // Every scene records at least one drawing command.
  #[test]
  fn every_scene_draws() {
    let scenes: [fn() -> Scene; 13] = [
      rounded_rect_fill,
      stroke,
      path,
      linear_gradient,
      radial_gradient,
      box_shadow,
      rect_clip,
      rounded_rect_clip,
      path_clip,
      nested_opacity,
      text_solid,
      text_gradient,
      text_wrapped,
    ];
    for (index, scene) in scenes.into_iter().enumerate() {
      let draws = scene().items().iter().any(|item| match item {
        SceneItem::Fragment(fragment) => fragment
          .commands()
          .iter()
          .any(|command| !matches!(command, Command::Push(_) | Command::Pop)),
        _ => false,
      });
      assert!(draws, "scene {index} draws nothing");
    }
  }
}
