use artic_layout::geometry::{CornerRadius, FillRule, PathElement, Point, Rect, RoundedRect, Transform};
use artic_layout::text::Paragraph;
use artic_render::{
  BoxShadow, Brush, Color, Command, GradientStop, Layer, LineCap, LineJoin, Paint, PhysicalSize, Scene, SceneItem, Shape,
  Stroke,
};
use std::sync::Arc;
use vello::kurbo::{self, Affine, BezPath, Cap, Diagonal2, Join, PathEl, Shape as _};
use vello::peniko::{self, BlendMode, Compose, Fill, Gradient, Mix};
use vello::{FontEmbolden, Glyph};

/// The tolerance vello flattens a shape with, so converting one to a path first changes nothing.
const TOLERANCE: f64 = 0.1;

/// How many standard deviations past its rectangle vello draws a blur, where it's close to clear.
const BLUR_EXTENT: f64 = 2.5;

/// How far a synthesized bold pushes each side of a glyph's outline out, per unit of font size:
/// Skia's fake bold, which strokes outlines a twenty-fourth of the size wide at small sizes.
const EMBOLDEN: f64 = 1.0 / 48.0;

/// Writes `scene` into `out`, drawn at `scale` device pixels per logical unit on a surface of
/// `size`.
pub(crate) fn translate(scene: &Scene, scale: f32, size: PhysicalSize, out: &mut vello::Scene) {
  out.reset();
  let mut translation = Translation {
    out,
    current: Affine::scale(f64::from(scale)),
    stack: Vec::new(),
    surface: kurbo::Rect::new(0.0, 0.0, f64::from(size.width), f64::from(size.height)),
  };
  for item in scene.items() {
    match item {
      SceneItem::Push(layer) => translation.push(layer),
      SceneItem::Pop => translation.pop(),
      SceneItem::Fragment(fragment) => {
        for command in fragment.commands() {
          match command {
            Command::Push(layer) => translation.push(layer),
            Command::Pop => translation.pop(),
            Command::Fill { shape, brush } => translation.fill(shape, brush),
            Command::Stroke { shape, brush, stroke } => translation.stroke(shape, brush, stroke),
            Command::BoxShadow { rect, shadow } => translation.box_shadow(rect, shadow),
            Command::Text {
              paragraph,
              origin,
              brush,
            } => translation.text(paragraph, *origin, brush),
          }
        }
      }
    }
  }
}

/// What a push left to undo at its pop.
enum Pushed {
  /// The transform in effect before it.
  Transform(Affine),
  /// A vello layer.
  Layer,
}

struct Translation<'a> {
  out: &'a mut vello::Scene,
  /// Maps the current coordinates to device pixels.
  current: Affine,
  stack: Vec<Pushed>,
  /// The whole surface in device pixels, the shape of an opacity layer.
  surface: kurbo::Rect,
}

impl Translation<'_> {
  fn push(&mut self, layer: &Layer) {
    match layer {
      Layer::Transform(transform) => {
        self.stack.push(Pushed::Transform(self.current));
        self.current *= affine(*transform);
      }
      Layer::Clip(shape) => {
        let (style, clip) = shape
          .drawable()
          .map_or((Fill::NonZero, BezPath::new()), |shape| (fill_rule(&shape), outline(&shape)));
        self.out.push_clip_layer(style, self.current, &clip);
        self.stack.push(Pushed::Layer);
      }
      Layer::Opacity(opacity) => {
        self
          .out
          .push_layer(Fill::NonZero, BlendMode::default(), *opacity, Affine::IDENTITY, &self.surface);
        self.stack.push(Pushed::Layer);
      }
    }
  }

  fn pop(&mut self) {
    match self.stack.pop() {
      Some(Pushed::Transform(previous)) => self.current = previous,
      Some(Pushed::Layer) => self.out.pop_layer(),
      None => {}
    }
  }

  fn fill(&mut self, shape: &Shape, brush: &Brush) {
    let Some(shape) = shape.drawable() else {
      return;
    };
    if let Some((brush, transform)) = paint(brush, shape.bounds()) {
      self
        .out
        .fill(fill_rule(&shape), self.current, &brush, transform, &outline(&shape));
    }
  }

  fn stroke(&mut self, shape: &Shape, brush: &Brush, stroke: &Stroke) {
    let (Some(shape), Some(stroke)) = (shape.drawable(), stroke.drawable()) else {
      return;
    };
    if let Some((brush, transform)) = paint(brush, shape.bounds()) {
      self
        .out
        .stroke(&kurbo_stroke(&stroke), self.current, &brush, transform, &outline(&shape));
    }
  }

  fn box_shadow(&mut self, rect: &RoundedRect, shadow: &BoxShadow) {
    let (Some(caster), Some(shadow)) = (Shape::from(*rect).drawable(), shadow.drawable()) else {
      return;
    };
    let Some(shape) = shadow.shape(*rect) else {
      return;
    };
    let mut outside = self.surface.to_path(TOLERANCE);
    outside.extend(self.current * outline(&caster));
    self.out.push_clip_layer(Fill::EvenOdd, Affine::IDENTITY, &outside);
    self.shadow(shape, shadow.blur, color(shadow.color, 1.0));
    self.out.pop_layer();
  }

  fn text(&mut self, paragraph: &Paragraph, origin: Point, brush: &Brush) {
    let size = paragraph.size();
    let Some((brush, transform)) = paint(brush, Rect::new(origin.x, origin.y, size.width, size.height)) else {
      return;
    };
    for run in paragraph.runs() {
      let font = run.font();
      if skrifa::FontRef::from_index((**font.bytes()).as_ref(), font.index()).is_err() {
        continue;
      }
      let blob = peniko::Blob::from_raw_parts(Arc::clone(font.bytes()), font.id());
      let synthesis = run.synthesis();
      let embolden = if synthesis.embolden {
        let amount = f64::from(run.size()) * EMBOLDEN;
        FontEmbolden::new(Diagonal2::new(amount, amount))
      } else {
        FontEmbolden::default()
      };
      let skew = (synthesis.skew != 0.0).then(|| Affine::skew(f64::from(synthesis.skew.to_radians().tan()), 0.0));
      let glyphs = run.glyphs().iter().map(|glyph| Glyph {
        id: glyph.id,
        x: glyph.x + origin.x,
        y: glyph.y + origin.y,
      });
      self
        .out
        .draw_glyphs(&peniko::FontData::new(blob, font.index()))
        .font_size(run.size())
        .transform(self.current)
        .glyph_transform(skew)
        .brush_transform(transform)
        .font_embolden(embolden)
        .hint(false)
        .normalized_coords(run.coords())
        .brush(&brush)
        .draw(Fill::NonZero, glyphs);
    }
  }

  fn shadow(&mut self, shape: RoundedRect, blur: f32, color: peniko::Color) {
    if blur == 0.0 {
      let outline = outline(&shape.into());
      self.out.fill(Fill::NonZero, self.current, color, None, &outline);
      return;
    }
    let blurs = blurs(shape, blur);
    let whole = blurs
      .iter()
      .map(|blur| blur.area)
      .reduce(|whole, area| whole.union(area))
      .unwrap_or_default();
    let back = half_pixel_back(self.current);
    if let [blur] = blurs.as_slice() {
      self
        .out
        .draw_blurred_rounded_rect_in(&whole, self.current, blur.rect + back, color, blur.radius, blur.std_dev);
      return;
    }
    let plus = BlendMode::new(Mix::Normal, Compose::Plus);
    self
      .out
      .push_layer(Fill::NonZero, BlendMode::default(), 1.0, self.current, &whole);
    for blur in blurs {
      self.out.push_layer(Fill::NonZero, plus, 1.0, self.current, &blur.area);
      self
        .out
        .draw_blurred_rounded_rect_in(&whole, self.current, blur.rect + back, color, blur.radius, blur.std_dev);
      self.out.pop_layer();
    }
    self.out.pop_layer();
  }
}

/// The same transform in kurbo's coefficient order, which is also CSS's.
pub(crate) fn affine(transform: Transform) -> Affine {
  let Transform { xx, yx, xy, yy, dx, dy } = transform;
  Affine::new([xx, yx, xy, yy, dx, dy].map(f64::from))
}

fn kurbo_rect(rect: Rect) -> kurbo::Rect {
  kurbo::Rect::from_origin_size(
    (f64::from(rect.x), f64::from(rect.y)),
    (f64::from(rect.width), f64::from(rect.height)),
  )
}

fn outline(shape: &Shape) -> BezPath {
  match shape {
    Shape::Rect(rect) => kurbo_rect(*rect).to_path(TOLERANCE),
    Shape::RoundedRect(rounded) => rounded.path().elements().iter().map(|element| path_el(*element)).collect(),
    Shape::Path(path) => path.elements().iter().map(|element| path_el(*element)).collect(),
  }
}

fn fill_rule(shape: &Shape) -> Fill {
  match shape {
    Shape::Path(path) => match path.fill_rule() {
      FillRule::EvenOdd => Fill::EvenOdd,
      FillRule::NonZero => Fill::NonZero,
    },
    Shape::Rect(_) | Shape::RoundedRect(_) => Fill::NonZero,
  }
}

fn kurbo_stroke(stroke: &Stroke) -> kurbo::Stroke {
  let join = match stroke.join {
    LineJoin::Miter => Join::Miter,
    LineJoin::Round => Join::Round,
    LineJoin::Bevel => Join::Bevel,
  };
  let cap = match stroke.cap {
    LineCap::Flat => Cap::Butt,
    LineCap::Round => Cap::Round,
    LineCap::Square => Cap::Square,
  };
  kurbo::Stroke::new(f64::from(stroke.width))
    .with_join(join)
    .with_caps(cap)
    .with_miter_limit(f64::from(stroke.miter_limit))
}

fn path_el(element: PathElement) -> PathEl {
  match element {
    PathElement::MoveTo(point) => PathEl::MoveTo(kurbo_point(point)),
    PathElement::LineTo(point) => PathEl::LineTo(kurbo_point(point)),
    PathElement::QuadTo(control, end) => PathEl::QuadTo(kurbo_point(control), kurbo_point(end)),
    PathElement::CubicTo(first, second, end) => PathEl::CurveTo(kurbo_point(first), kurbo_point(second), kurbo_point(end)),
    PathElement::Close => PathEl::ClosePath,
  }
}

/// One of vello's blurred rounded rectangles: `rect` with corners of `radius`, blurred with
/// `std_dev`, drawn only inside `area`.
#[derive(Debug, PartialEq)]
struct Blur {
  area: kurbo::Rect,
  rect: kurbo::Rect,
  radius: f64,
  std_dev: f64,
}

fn blurs(shape: RoundedRect, blur: f32) -> Vec<Blur> {
  let RoundedRect { rect, radius } = shape;
  let rect = kurbo_rect(rect);
  let std_dev = f64::from(blur) / 2.0;
  let area = rect.inflate(BLUR_EXTENT * std_dev, BLUR_EXTENT * std_dev);
  let blur = |area, radius: f32| Blur {
    area,
    rect,
    radius: f64::from(radius),
    std_dev,
  };
  let CornerRadius {
    top_left,
    top_right,
    bottom_right,
    bottom_left,
  } = radius;
  if [top_right, bottom_right, bottom_left]
    .iter()
    .all(|&corner| corner == top_left)
  {
    return vec![blur(area, top_left)];
  }
  let middle = rect.center();
  vec![
    blur(kurbo::Rect::new(area.x0, area.y0, middle.x, middle.y), top_left),
    blur(kurbo::Rect::new(middle.x, area.y0, area.x1, middle.y), top_right),
    blur(kurbo::Rect::new(middle.x, middle.y, area.x1, area.y1), bottom_right),
    blur(kurbo::Rect::new(area.x0, middle.y, middle.x, area.y1), bottom_left),
  ]
}

fn half_pixel_back(current: Affine) -> kurbo::Vec2 {
  let inverse = current.inverse();
  let back = inverse * kurbo::Point::new(-0.5, -0.5) - inverse * kurbo::Point::ORIGIN;
  if back.is_finite() { back } else { kurbo::Vec2::ZERO }
}

fn paint(brush: &Brush, rect: Rect) -> Option<(peniko::Brush, Option<Affine>)> {
  brush.drawable(rect).map(peniko_brush)
}

/// The peniko brush that draws `paint`, with the brush transform a radial gradient needs: peniko
/// draws circles, so the ellipse is the unit circle scaled by the two radii and moved to the center.
fn peniko_brush(paint: Paint) -> (peniko::Brush, Option<Affine>) {
  let stops = |stops: &[GradientStop], opacity: f32| -> Vec<(f32, peniko::Color)> {
    stops.iter().map(|stop| (stop.offset, color(stop.color, opacity))).collect()
  };
  match paint {
    Paint::Solid { color: solid, opacity } => (peniko::Brush::Solid(color(solid, opacity)), None),
    Paint::Linear {
      start,
      end,
      stops: linear,
      opacity,
    } => (
      peniko::Brush::Gradient(
        Gradient::new_linear(kurbo_point(start), kurbo_point(end)).with_stops(stops(&linear, opacity).as_slice()),
      ),
      None,
    ),
    Paint::Radial {
      center,
      radius_x,
      radius_y,
      stops: radial,
      opacity,
    } => {
      let ellipse =
        Affine::translate(kurbo_point(center).to_vec2()) * Affine::scale_non_uniform(f64::from(radius_x), f64::from(radius_y));
      (
        peniko::Brush::Gradient(Gradient::new_radial((0.0, 0.0), 1.0).with_stops(stops(&radial, opacity).as_slice())),
        Some(ellipse),
      )
    }
  }
}

fn color(color: Color, opacity: f32) -> peniko::Color {
  peniko::Color::from_rgba8(color.r, color.g, color.b, color.a).multiply_alpha(opacity)
}

fn kurbo_point(point: Point) -> kurbo::Point {
  kurbo::Point::new(f64::from(point.x), f64::from(point.y))
}

#[cfg(test)]
mod tests {
  use super::*;
  use artic_layout::geometry::Size;
  use artic_layout::geometry::{CornerRadius, FillRule, Path, Point, RelativePoint, RelativeScalar, RoundedRect};
  use artic_layout::text::{FontData, Glyph, GlyphRun, Line, Paragraph, Synthesis};
  use artic_render::{BoxShadow, LinearGradientBrush, RadialGradientBrush, SolidColorBrush};
  use pretty_assertions::assert_eq;
  use vello::low_level::vello_encoding::Transform as Encoded;

  // artic's transform and kurbo's map points the same way.
  #[test]
  fn affine_matches_transform() {
    let transform = Transform::scale(2.0, 3.0)
      .then(Transform::rotate(0.5))
      .then(Transform::translate(4.0, -1.0));
    let point = Point::new(1.5, -2.0);

    let ours = transform.apply(point);
    let theirs = affine(transform) * kurbo::Point::new(1.5, -2.0);

    assert!((f64::from(ours.x) - theirs.x).abs() < 1e-5, "{ours} vs {theirs:?}");
    assert!((f64::from(ours.y) - theirs.y).abs() < 1e-5, "{ours} vs {theirs:?}");
    assert_eq!(affine(Transform::IDENTITY), Affine::IDENTITY);
  }

  const RED: Color = Color::rgb(255, 0, 0);
  const BLUE: Color = Color::rgb(0, 0, 255);

  fn opaque(r: u8, g: u8, b: u8) -> peniko::Color {
    peniko::Color::from_rgba8(r, g, b, 255)
  }

  fn red_to_blue() -> [GradientStop; 2] {
    [GradientStop::new(0.0, RED), GradientStop::new(1.0, BLUE)]
  }

  // Each brush becomes the vello brush of the same colors, its points and radii resolved against
  // the rectangle and its opacity applied to every alpha.
  #[test]
  fn brush_to_peniko_roundtrip() {
    let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
    let (red, blue) = (opaque(255, 0, 0), opaque(0, 0, 255));
    let solid = SolidColorBrush {
      color: RED,
      opacity: 0.5,
    };
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
      paint(&solid.into(), rect),
      Some((peniko::Brush::Solid(red.with_alpha(0.5)), None))
    );
    assert_eq!(
      paint(&linear.into(), rect),
      Some((
        peniko::Brush::Gradient(
          Gradient::new_linear((10.0, 45.0), (110.0, 45.0))
            .with_stops([(0.0, red.with_alpha(0.5)), (1.0, blue.with_alpha(0.5))])
        ),
        None
      ))
    );
    assert_eq!(
      paint(&radial.into(), rect),
      Some((
        peniko::Brush::Gradient(Gradient::new_radial((0.0, 0.0), 1.0).with_stops([(0.0, red), (1.0, blue)])),
        Some(Affine::translate((60.0, 45.0)) * Affine::scale_non_uniform(50.0, 10.0))
      ))
    );
  }

  fn encoded(draw: impl FnOnce(&mut Translation<'_>)) -> vello::Scene {
    let mut out = vello::Scene::new();
    draw(&mut Translation {
      out: &mut out,
      current: Affine::IDENTITY,
      stack: Vec::new(),
      surface: kurbo::Rect::ZERO,
    });
    out
  }

  // Each shape becomes a kurbo path: a rectangle as vello would make it, a rounded rectangle
  // filling the same box, or the path's elements one for one. A path keeps its
  // fill rule; rectangles fill by non-zero.
  #[test]
  fn shapes_convert_to_kurbo() {
    let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
    let corners = kurbo::Rect::new(10.0, 20.0, 110.0, 70.0);
    let rounded = RoundedRect::new(rect, CornerRadius::new(1.0, 2.0, 3.0, 4.0));
    let path = Path::builder(Point::new(0.0, 0.0))
      .line_to(Point::new(10.0, 0.0))
      .quad_to(Point::new(10.0, 10.0), Point::new(0.0, 10.0))
      .cubic_to(Point::new(0.0, 5.0), Point::new(5.0, 5.0), Point::new(5.0, 0.0))
      .close()
      .fill_rule(FillRule::NonZero)
      .build();
    let mut expected = BezPath::new();
    expected.move_to((0.0, 0.0));
    expected.line_to((10.0, 0.0));
    expected.quad_to((10.0, 10.0), (0.0, 10.0));
    expected.curve_to((0.0, 5.0), (5.0, 5.0), (5.0, 0.0));
    expected.close_path();

    assert_eq!(outline(&rect.into()), corners.to_path(TOLERANCE));
    assert_eq!(outline(&rounded.into()).control_box(), corners);
    assert_eq!(outline(&path.clone().into()), expected);
    assert_eq!(
      [
        fill_rule(&rect.into()),
        fill_rule(&rounded.into()),
        fill_rule(&path.into()),
        fill_rule(&Path::builder(Point::ZERO).build().into()),
      ],
      [Fill::NonZero, Fill::NonZero, Fill::NonZero, Fill::EvenOdd]
    );
  }

  // A rounded rectangle keeps the radii `RoundedRect::fitted` gives it, corner by corner, even a
  // radius over half the shortest side, which a side with a square corner leaves room for.
  #[test]
  fn rounded_rects_keep_their_fitted_radii() {
    let tab = RoundedRect::new(Rect::new(0.0, 0.0, 10.0, 40.0), CornerRadius::new(4.0, 0.0, 20.0, 0.0));
    let shape = Shape::from(tab).drawable().expect("drawable");

    let outline = outline(&shape);

    assert_eq!(outline.control_box(), kurbo::Rect::new(0.0, 0.0, 10.0, 40.0));
    assert!(!outline.contains((0.2, 0.2).into()), "top-left, radius 2");
    assert!(outline.contains((9.9, 0.1).into()), "top-right, square");
    assert!(!outline.contains((8.5, 37.5).into()), "bottom-right, radius 10");
    assert!(outline.contains((0.1, 39.9).into()), "bottom-left, square");
    assert!(outline.contains((5.0, 20.0).into()), "middle");
  }

  // A shape that draws nothing by the rules of `Shape::drawable` reaches vello as no path at all,
  // and so does a path that is only its start point.
  #[test]
  fn invalid_shapes_and_strokes_draw_nothing() {
    let red = Brush::from(RED);
    let invalid: [Shape; 5] = [
      Rect::new(f32::NAN, 0.0, 1.0, 1.0).into(),
      Rect::new(0.0, 0.0, f32::INFINITY, 1.0).into(),
      RoundedRect::new(Rect::new(0.0, 0.0, -1.0, 1.0), CornerRadius::ZERO).into(),
      Path::builder(Point::ZERO)
        .line_to(Point::new(f32::NEG_INFINITY, 1.0))
        .build()
        .into(),
      Path::builder(Point::ZERO).build().into(),
    ];

    for shape in &invalid {
      assert_eq!(encoded(|t| t.fill(shape, &red)).encoding().n_paths, 0, "{shape:?}");
    }
    assert_eq!(
      encoded(|t| t.fill(&Rect::new(0.0, 0.0, 1.0, 1.0).into(), &red))
        .encoding()
        .n_paths,
      1,
      "a valid fill draws"
    );
    let square: Shape = Rect::new(0.0, 0.0, 1.0, 1.0).into();
    for shape in &invalid {
      assert_eq!(
        encoded(|t| t.stroke(shape, &red, &Stroke::default())).encoding().n_paths,
        0,
        "{shape:?}"
      );
    }
    for width in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
      assert_eq!(
        encoded(|t| t.stroke(&square, &red, &Stroke::new(width))).encoding().n_paths,
        0,
        "width {width}"
      );
    }
    assert_eq!(
      encoded(|t| t.stroke(&square, &red, &Stroke::new(1.0).with_miter_limit(f32::NAN)))
        .encoding()
        .n_paths,
      1,
      "a valid stroke draws, its miter limit made drawable"
    );
  }

  // A stroke keeps its width and miter limit, its join maps by name, and its cap is both of
  // kurbo's: Flat is Butt.
  #[test]
  fn stroke_converts_join_cap_and_miter() {
    let ours = |join, cap| Stroke::new(2.0).with_join(join).with_cap(cap).with_miter_limit(4.0);
    let theirs = |join, cap| kurbo::Stroke::new(2.0).with_join(join).with_caps(cap).with_miter_limit(4.0);

    assert_eq!(
      kurbo_stroke(&Stroke::default()),
      kurbo::Stroke::new(1.0)
        .with_join(Join::Miter)
        .with_caps(Cap::Butt)
        .with_miter_limit(10.0)
    );
    assert_eq!(
      kurbo_stroke(&ours(LineJoin::Round, LineCap::Round)),
      theirs(Join::Round, Cap::Round)
    );
    assert_eq!(
      kurbo_stroke(&ours(LineJoin::Bevel, LineCap::Square)),
      theirs(Join::Bevel, Cap::Square)
    );
    assert_eq!(
      kurbo_stroke(&ours(LineJoin::Miter, LineCap::Flat)),
      theirs(Join::Miter, Cap::Butt)
    );
  }

  // A shadow is blurred with half its blur as the standard deviation, over the area vello's own
  // blur covers, 2.5 standard deviations past the shadow's rectangle. Equal radii make one blur.
  // Unequal radii make four, one per quarter of that area, each with its corner's radius.
  #[test]
  fn box_shadow_grows_moves_and_blurs() {
    let card = RoundedRect::new(Rect::new(10.0, 20.0, 100.0, 50.0), CornerRadius::uniform(4.0));
    let shadow = BoxShadow::new(Point::new(3.0, 5.0), 8.0, RED).with_spread(2.0);
    let grown = kurbo::Rect::new(11.0, 23.0, 115.0, 77.0);
    let tab = RoundedRect::new(Rect::new(0.0, 0.0, 20.0, 10.0), CornerRadius::new(1.0, 2.0, 3.0, 0.0));
    let quarter = |x0, y0, x1, y1, radius| Blur {
      area: kurbo::Rect::new(x0, y0, x1, y1),
      rect: kurbo::Rect::new(0.0, 0.0, 20.0, 10.0),
      radius,
      std_dev: 2.0,
    };

    assert_eq!(
      blurs(shadow.shape(card).expect("drawn"), shadow.blur),
      [Blur {
        area: grown.inflate(10.0, 10.0),
        rect: grown,
        radius: 6.0,
        std_dev: 4.0
      }]
    );
    assert_eq!(
      blurs(tab, 4.0),
      [
        quarter(-5.0, -5.0, 10.0, 5.0, 1.0),
        quarter(10.0, -5.0, 25.0, 5.0, 2.0),
        quarter(10.0, 5.0, 25.0, 15.0, 3.0),
        quarter(-5.0, 5.0, 10.0, 15.0, 0.0),
      ]
    );
  }

  // A shadow that draws nothing by the rules of `BoxShadow` reaches vello as no path at all.
  #[test]
  fn invalid_box_shadows_draw_nothing() {
    let card = RoundedRect::from(Rect::new(0.0, 0.0, 10.0, 10.0));
    let shadow = |x| BoxShadow::new(Point::new(x, 0.0), 2.0, RED);
    let paths = |rect: RoundedRect, shadow: BoxShadow| encoded(|t| t.box_shadow(&rect, &shadow)).encoding().n_paths;

    assert_eq!(paths(card, shadow(f32::NAN)), 0, "NaN offset");
    assert_eq!(paths(card, shadow(f32::INFINITY)), 0, "infinite offset");
    assert_eq!(paths(card, shadow(0.0).with_spread(-5.0)), 0, "shrunk to nothing");
    assert_eq!(
      paths(Rect::new(0.0, 0.0, -1.0, 10.0).into(), shadow(0.0)),
      0,
      "negative width"
    );
    assert!(paths(card, shadow(0.0)) > 0, "a valid shadow draws");
    assert!(
      paths(
        RoundedRect::new(card.rect, CornerRadius::new(1.0, 2.0, 3.0, 4.0)),
        shadow(0.0)
      ) > 0,
      "unequal radii draw"
    );
  }

  fn face(bytes: &'static [u8]) -> FontData {
    FontData::new(Arc::new(bytes), 0)
  }

  fn run(font: &FontData, synthesis: Synthesis) -> GlyphRun {
    let glyphs = [Glyph { id: 36, x: 0.0, y: 12.0 }, Glyph { id: 37, x: 9.0, y: 12.0 }];
    GlyphRun::new(font.clone(), 16.0, Vec::<i16>::new(), synthesis, glyphs)
  }

  fn paragraph(runs: Vec<GlyphRun>) -> Paragraph {
    Paragraph::new(Size::new(30.0, 20.0), Vec::<Line>::new(), runs)
  }

  // Each run draws from a blob that carries the font's artic id and index, so vello's glyph cache
  // knows a face by the same id in every frame. Glyphs are moved by the origin.
  #[test]
  fn font_id_becomes_the_blob_id() {
    let font = face(artic_test_fonts::REGULAR);
    let text = paragraph(vec![run(&font, Synthesis::default())]);

    let out = encoded(|translation| translation.text(&text, Point::new(4.0, 6.0), &Color::rgb(0, 0, 0).into()));

    let runs = &out.encoding().resources.glyph_runs;
    assert_eq!(runs.len(), 1);
    assert_eq!((runs[0].font.data.id(), runs[0].font.index), (font.id(), 0));
    assert_eq!(runs[0].font_size, 16.0);
    assert!(!runs[0].hint);
    let glyphs = &out.encoding().resources.glyphs;
    assert_eq!((glyphs[0].x, glyphs[0].y, glyphs[1].x), (4.0, 18.0, 13.0));
  }

  // Runs from different faces draw as separate glyph runs, each with its own face's id.
  #[test]
  fn each_run_draws_with_its_own_font() {
    let (regular, bold) = (face(artic_test_fonts::REGULAR), face(artic_test_fonts::BOLD));
    let text = paragraph(vec![run(&regular, Synthesis::default()), run(&bold, Synthesis::default())]);

    let out = encoded(|translation| translation.text(&text, Point::ZERO, &Color::rgb(0, 0, 0).into()));

    let ids: Vec<u64> = out
      .encoding()
      .resources
      .glyph_runs
      .iter()
      .map(|run| run.font.data.id())
      .collect();
    assert_eq!(ids, [regular.id(), bold.id()]);
  }

  // A synthesized bold pushes each side of the outlines out by a forty-eighth of the size; a
  // synthesized slant shears them.
  #[test]
  fn synthesis_becomes_embolden_and_skew() {
    let font = face(artic_test_fonts::REGULAR);
    let synthesis = Synthesis {
      embolden: true,
      skew: 14.0,
    };
    let text = paragraph(vec![run(&font, synthesis)]);

    let out = encoded(|translation| translation.text(&text, Point::ZERO, &Color::rgb(0, 0, 0).into()));

    let drawn = &out.encoding().resources.glyph_runs[0];
    assert_eq!(drawn.font_embolden.amount, kurbo::Diagonal2::new(16.0 / 48.0, 16.0 / 48.0));
    let skew = Affine::skew(f64::from(14.0_f32.to_radians().tan()), 0.0);
    assert_eq!(drawn.glyph_transform, Some(Encoded::from_kurbo(&skew)));
  }

  // Glyphs draw in the current coordinates, so a transform such as the display scale applies.
  #[test]
  fn text_follows_the_current_transform() {
    let font = face(artic_test_fonts::REGULAR);
    let text = paragraph(vec![run(&font, Synthesis::default())]);
    let mut out = vello::Scene::new();

    Translation {
      out: &mut out,
      current: Affine::scale(2.0),
      stack: Vec::new(),
      surface: kurbo::Rect::ZERO,
    }
    .text(&text, Point::ZERO, &Color::rgb(0, 0, 0).into());

    let drawn = &out.encoding().resources.glyph_runs[0];
    assert_eq!(drawn.transform, Encoded::from_kurbo(&Affine::scale(2.0)));
  }

  // A brush that paints nothing, such as a gradient without stops, draws no glyphs.
  #[test]
  fn text_without_paint_draws_nothing() {
    let font = face(artic_test_fonts::REGULAR);
    let text = paragraph(vec![run(&font, Synthesis::default())]);
    let nothing = LinearGradientBrush::new(RelativePoint::relative(0.0, 0.0), RelativePoint::relative(1.0, 0.0), []);

    let out = encoded(|translation| translation.text(&text, Point::ZERO, &nothing.into()));

    assert!(out.encoding().resources.glyph_runs.is_empty());
  }

  // A face vello can't read, such as bytes that aren't a font or an index past a file's faces, is
  // skipped instead of panicking the frame; the readable runs of the same paragraph still draw.
  #[test]
  fn unreadable_face_draws_nothing() {
    let regular = face(artic_test_fonts::REGULAR);
    let garbage = FontData::new(Arc::new(vec![0_u8; 4]), 0);
    let missing = FontData::new(Arc::new(artic_test_fonts::REGULAR), 3);
    let text = paragraph(vec![
      run(&garbage, Synthesis::default()),
      run(&missing, Synthesis::default()),
      run(&regular, Synthesis::default()),
    ]);

    let out = encoded(|translation| translation.text(&text, Point::ZERO, &Color::rgb(0, 0, 0).into()));

    let ids: Vec<u64> = out
      .encoding()
      .resources
      .glyph_runs
      .iter()
      .map(|run| run.font.data.id())
      .collect();
    assert_eq!(ids, [regular.id()]);
  }
}
