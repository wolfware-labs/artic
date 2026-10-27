use crate::{BoxShadow, Brush, Command, Layer, Shape, Stroke};
use artic_layout::geometry::{Point, RoundedRect, Transform};
use artic_layout::text::Paragraph;

/// Records one node's drawing, in the node's own coordinates.
///
/// Layers are only opened by the `with_*` methods, which close them when their closure returns, so
/// a recording can't leave a layer open for the nodes drawn after it.
///
/// # Examples
///
/// ```
/// use artic_layout::geometry::{CornerRadius, Rect, RoundedRect};
/// use artic_render::{Color, SceneBuilder};
///
/// let mut builder = SceneBuilder::default();
/// let card = RoundedRect::new(Rect::new(0.0, 0.0, 200.0, 100.0), CornerRadius::uniform(8.0));
/// builder.fill(card, Color::rgb(255, 255, 255));
/// builder.with_clip(card, |builder| {
///   builder.with_opacity(0.5, |builder| builder.fill(Rect::new(0.0, 0.0, 200.0, 20.0), Color::rgb(0, 0, 0)));
/// });
/// ```
#[derive(Debug, Default)]
pub struct SceneBuilder {
  commands: Vec<Command>,
}

impl SceneBuilder {
  /// Fills `shape` with `brush`, or with anything that converts into one, such as a
  /// [`Color`](crate::Color). The brush's relative points resolve against the shape's
  /// [bounds](Shape::bounds).
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_layout::geometry::{Path, Point, Rect};
  /// use artic_render::{Color, SceneBuilder};
  ///
  /// let mut builder = SceneBuilder::default();
  /// builder.fill(Rect::new(0.0, 0.0, 100.0, 40.0), Color::rgb(59, 130, 246));
  /// let triangle = Path::builder(Point::new(50.0, 0.0))
  ///   .line_to(Point::new(100.0, 100.0))
  ///   .line_to(Point::new(0.0, 100.0))
  ///   .close()
  ///   .build();
  /// builder.fill(triangle, Color::rgb(236, 72, 153));
  /// ```
  pub fn fill(&mut self, shape: impl Into<Shape>, brush: impl Into<Brush>) {
    self.commands.push(Command::Fill {
      shape: shape.into(),
      brush: brush.into(),
    });
  }

  /// Strokes the outline of `shape` with `brush`, centered on it. The brush's relative points
  /// resolve against the shape's [bounds](Shape::bounds), without the stroke's width.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_layout::geometry::{CornerRadius, Rect, RoundedRect};
  /// use artic_render::{Color, LineJoin, SceneBuilder, Stroke};
  ///
  /// let mut builder = SceneBuilder::default();
  /// let card = RoundedRect::new(Rect::new(0.5, 0.5, 199.0, 99.0), CornerRadius::uniform(8.0));
  /// builder.stroke(card, Color::rgb(203, 213, 225), Stroke::new(1.0).with_join(LineJoin::Round));
  /// ```
  pub fn stroke(&mut self, shape: impl Into<Shape>, brush: impl Into<Brush>, stroke: Stroke) {
    self.commands.push(Command::Stroke {
      shape: shape.into(),
      brush: brush.into(),
      stroke,
    });
  }

  /// Draws the shadow `rect` casts, CSS's outer `box-shadow`: it shows only outside `rect`, so a
  /// translucent `rect` keeps its own color. Record it before what casts it, so the shadow shows
  /// behind.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_layout::geometry::{CornerRadius, Point, Rect, RoundedRect};
  /// use artic_render::{BoxShadow, Color, SceneBuilder};
  ///
  /// let mut builder = SceneBuilder::default();
  /// let card = RoundedRect::new(Rect::new(0.0, 0.0, 200.0, 100.0), CornerRadius::uniform(8.0));
  /// builder.box_shadow(card, BoxShadow::new(Point::new(0.0, 2.0), 8.0, Color::rgba(0, 0, 0, 64)));
  /// builder.fill(card, Color::rgb(255, 255, 255));
  /// ```
  pub fn box_shadow(&mut self, rect: impl Into<RoundedRect>, shadow: BoxShadow) {
    self.commands.push(Command::BoxShadow {
      rect: rect.into(),
      shadow,
    });
  }

  /// Draws `paragraph` with its top-left corner at `origin`, painted with `brush`. The brush's
  /// relative points resolve against the paragraph's box, so a gradient runs across the whole
  /// text. A paragraph without glyphs records nothing.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_layout::geometry::Point;
  /// use artic_layout::text::Paragraph;
  /// use artic_render::{Color, SceneBuilder};
  ///
  /// let paragraph = Paragraph::default();
  /// let mut builder = SceneBuilder::default();
  /// builder.text(&paragraph, Point::new(8.0, 8.0), Color::rgb(15, 23, 42));
  /// ```
  pub fn text(&mut self, paragraph: &Paragraph, origin: Point, brush: impl Into<Brush>) {
    if paragraph.is_empty() {
      return;
    }
    self.commands.push(Command::Text {
      paragraph: paragraph.clone(),
      origin,
      brush: brush.into(),
    });
  }

  /// Records `draw` inside `transform`.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_layout::geometry::{Rect, Transform};
  /// use artic_render::{Color, SceneBuilder};
  ///
  /// let mut builder = SceneBuilder::default();
  /// builder.with_transform(Transform::rotate(0.5), |builder| {
  ///   builder.fill(Rect::new(0.0, 0.0, 10.0, 10.0), Color::rgb(0, 0, 0));
  /// });
  /// ```
  pub fn with_transform(&mut self, transform: Transform, draw: impl FnOnce(&mut SceneBuilder)) {
    self.with_layer(Layer::Transform(transform), draw);
  }

  /// Records `draw` clipped to `shape`: only what falls inside it shows.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_layout::geometry::{CornerRadius, Rect, RoundedRect};
  /// use artic_render::{Color, SceneBuilder};
  ///
  /// let mut builder = SceneBuilder::default();
  /// let card = RoundedRect::new(Rect::new(0.0, 0.0, 200.0, 100.0), CornerRadius::uniform(8.0));
  /// builder.with_clip(card, |builder| {
  ///   builder.fill(Rect::new(0.0, 0.0, 200.0, 20.0), Color::rgb(59, 130, 246));
  /// });
  /// ```
  pub fn with_clip(&mut self, shape: impl Into<Shape>, draw: impl FnOnce(&mut SceneBuilder)) {
    self.with_layer(Layer::Clip(shape.into()), draw);
  }

  /// Records `draw` as one group blended at `opacity`. The opacity is recorded as given and made
  /// [drawable](crate::drawable_opacity) where it's drawn: clamped to `0..=1`, NaN counting as `1`.
  ///
  /// # Examples
  ///
  /// ```
  /// use artic_layout::geometry::Rect;
  /// use artic_render::{Color, SceneBuilder};
  ///
  /// let mut builder = SceneBuilder::default();
  /// builder.with_opacity(0.5, |builder| {
  ///   builder.fill(Rect::new(0.0, 0.0, 10.0, 10.0), Color::rgb(0, 0, 0));
  /// });
  /// ```
  pub fn with_opacity(&mut self, opacity: f32, draw: impl FnOnce(&mut SceneBuilder)) {
    self.with_layer(Layer::Opacity(opacity), draw);
  }

  /// Takes the recorded commands, leaving the builder empty.
  pub(crate) fn take(&mut self) -> Vec<Command> {
    std::mem::take(&mut self.commands)
  }

  fn with_layer(&mut self, layer: Layer, draw: impl FnOnce(&mut SceneBuilder)) {
    self.commands.push(Command::Push(layer));
    draw(self);
    self.commands.push(Command::Pop);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{BoxShadow, Color, Stroke, drawable_opacity};
  use artic_layout::geometry::{CornerRadius, Path, Point, Rect, RoundedRect, Size};
  use artic_layout::text::{FontData, Glyph, GlyphRun, Line, Paragraph, Synthesis};
  use pretty_assertions::assert_eq;
  use std::sync::Arc;

  fn paragraph(runs: usize) -> Paragraph {
    let font = FontData::new(Arc::new(vec![0_u8; 4]), 0);
    let glyphs = [Glyph { id: 1, x: 0.0, y: 12.0 }];
    let run = GlyphRun::new(font, 16.0, Vec::<i16>::new(), Synthesis::default(), glyphs);
    Paragraph::new(Size::new(30.0, 20.0), Vec::<Line>::new(), vec![run; runs])
  }

  // A paragraph is recorded as one command, with its origin and brush.
  #[test]
  fn text_records_one_command() {
    let mut builder = SceneBuilder::default();
    let paragraph = paragraph(2);

    builder.text(&paragraph, Point::new(4.0, 6.0), Color::rgb(1, 2, 3));

    assert_eq!(
      builder.take(),
      [Command::Text {
        paragraph,
        origin: Point::new(4.0, 6.0),
        brush: Color::rgb(1, 2, 3).into(),
      }]
    );
  }

  // A paragraph without runs draws nothing, so nothing is recorded.
  #[test]
  fn empty_paragraph_records_nothing() {
    let mut builder = SceneBuilder::default();

    builder.text(&paragraph(0), Point::ZERO, Color::rgb(1, 2, 3));

    assert!(builder.take().is_empty());
  }

  // Fills and strokes record their shape, brush and stroke, in order, whatever the shape.
  #[test]
  fn fill_and_stroke_record_shapes() {
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    let rounded = RoundedRect::new(rect, CornerRadius::uniform(2.0));
    let path = Path::builder(Point::ZERO).line_to(Point::new(5.0, 5.0)).build();
    let black = Color::rgb(0, 0, 0);
    let mut builder = SceneBuilder::default();

    builder.fill(rect, black);
    builder.fill(rounded, black);
    builder.fill(path.clone(), black);
    builder.stroke(path.clone(), black, Stroke::new(2.0));

    assert_eq!(
      builder.take(),
      [
        Command::Fill {
          shape: rect.into(),
          brush: black.into()
        },
        Command::Fill {
          shape: rounded.into(),
          brush: black.into()
        },
        Command::Fill {
          shape: path.clone().into(),
          brush: black.into()
        },
        Command::Stroke {
          shape: path.into(),
          brush: black.into(),
          stroke: Stroke::new(2.0)
        },
      ]
    );
    assert!(builder.take().is_empty(), "take leaves the builder empty");
  }

  // A box shadow records the rectangle that casts it and the shadow, as given.
  #[test]
  fn box_shadow_records_rect_and_shadow() {
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    let shadow = BoxShadow::new(Point::new(0.0, 2.0), 8.0, Color::rgba(0, 0, 0, 64)).with_spread(-1.0);
    let mut builder = SceneBuilder::default();

    builder.box_shadow(rect, shadow.clone());

    assert_eq!(
      builder.take(),
      [Command::BoxShadow {
        rect: rect.into(),
        shadow
      }]
    );
  }

  // Each layer is pushed before its closure draws and popped when it returns, so the pushes and
  // pops of a recording always pair up.
  #[test]
  fn layers_balance_by_construction() {
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    let black = Color::rgb(0, 0, 0);
    let mut builder = SceneBuilder::default();

    builder.with_transform(Transform::translate(1.0, 0.0), |builder| {
      builder.with_clip(rect, |builder| builder.fill(rect, black));
    });

    assert_eq!(
      builder.take(),
      [
        Command::Push(Layer::Transform(Transform::translate(1.0, 0.0))),
        Command::Push(Layer::Clip(rect.into())),
        Command::Fill {
          shape: rect.into(),
          brush: black.into()
        },
        Command::Pop,
        Command::Pop,
      ]
    );
  }

  // Opacity layers nest, each recorded around what its closure draws.
  #[test]
  fn opacity_layer_nests() {
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    let black = Color::rgb(0, 0, 0);
    let mut builder = SceneBuilder::default();

    builder.with_opacity(0.5, |builder| {
      builder.with_opacity(0.25, |builder| builder.fill(rect, black));
    });

    assert_eq!(
      builder.take(),
      [
        Command::Push(Layer::Opacity(0.5)),
        Command::Push(Layer::Opacity(0.25)),
        Command::Fill {
          shape: rect.into(),
          brush: black.into()
        },
        Command::Pop,
        Command::Pop,
      ]
    );
  }

  // A layer's opacity is drawn clamped to 0..=1, NaN counting as 1, so a broken value doesn't
  // hide what it wraps; the builder records it as given, for the renderer to make drawable.
  #[test]
  fn opacity_is_made_drawable_when_drawn() {
    let mut builder = SceneBuilder::default();

    builder.with_opacity(2.0, |_| {});

    assert_eq!(
      [0.25, -1.0, 2.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY].map(drawable_opacity),
      [0.25, 0.0, 1.0, 1.0, 1.0, 0.0]
    );
    assert_eq!(builder.take()[0], Command::Push(Layer::Opacity(2.0)));
  }
}
