// Opens a window and draws every primitive artic's scene builder records, one per tile, through
// the same `Compositor` and `Renderer` seam artic's frame pipeline uses: a card with a shadow, a
// rounded rectangle with four radii, a stroked star, linear and radial gradients, clips by
// rectangle, rounded rectangle and path, nested opacity, and wrapped text in a gradient, shaped
// with the system's fonts.
//
// Run with `cargo run -p artic-examples --example primitives`.

use artic_core::Object;
use artic_core::property::{Affects, PropertyVisitor};
use artic_core::tree::{NodeId, UiTree};
use artic_layout::geometry::{CornerRadius, Path, Point, Rect, RelativePoint, RelativeScalar, RoundedRect, Transform};
use artic_layout::text::{FontFamily, Paragraph, ParagraphStyle, TextEngine, TextLimits, TextStyle, TextWrapping};
use artic_platform::{
  AppHandler, Axes, Chrome, Host as _, HostContext, HostEvent, LogicalSize, SurfaceInfo, SurfaceLoss, Waker, WindowDesc,
};
use artic_render::Renderer as _;
use artic_render::{
  BoxShadow, Color, Compositor, GradientStop, Layers, LineJoin, LinearGradientBrush, Nodes, RadialGradientBrush, SceneBuilder,
  Stroke, SurfaceId, SurfaceView, Target,
};
use artic_vello::Renderer;
use std::f32::consts::{FRAC_PI_2, PI};
use std::time::Duration;

const TILE: f32 = 160.0;
const GAP: f32 = 24.0;
const COLUMNS: usize = 5;
const ROWS: usize = 2;
const BACKGROUND: Color = Color::rgb(241, 245, 249);
const WHITE: Color = Color::rgb(255, 255, 255);
const SLATE: Color = Color::rgb(203, 213, 225);
const BLUE: Color = Color::rgb(59, 130, 246);
const PINK: Color = Color::rgb(236, 72, 153);
const YELLOW: Color = Color::rgb(250, 204, 21);

fn main() -> Result<(), artic_winit::Error> {
  let renderer = match Renderer::new() {
    Ok(renderer) => renderer,
    Err(error) => {
      eprintln!("can't start the renderer: {}", artic_examples::chain(&error));
      std::process::exit(1);
    }
  };
  artic_winit::Host::new()?.run(Demo::new(renderer))?;
  Ok(())
}

#[derive(Debug)]
struct Node;

impl Object for Node {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

/// One node that draws every tile, laid out by hand until layout (#41) exists.
struct Primitives {
  /// The window's size in logical units.
  width: f32,
  height: f32,
  /// The text tile's sentence, shaped once.
  paragraph: Paragraph,
}

impl Nodes for Primitives {
  fn layers(&self, _tree: &UiTree, _node: NodeId) -> Layers {
    Layers::default()
  }

  fn record(&mut self, _tree: &UiTree, _node: NodeId, builder: &mut SceneBuilder) {
    builder.fill(Rect::new(0.0, 0.0, self.width, self.height), BACKGROUND);
    let tiles: [fn(&mut SceneBuilder); 9] = [
      card,
      rounded,
      star_outline,
      linear,
      radial,
      rect_clip,
      rounded_clip,
      path_clip,
      nested_opacity,
    ];
    for (index, tile) in tiles.into_iter().enumerate() {
      let (column, row) = ((index % COLUMNS) as f32, (index / COLUMNS) as f32);
      let origin = Transform::translate(GAP + column * (TILE + GAP), GAP + row * (TILE + GAP));
      builder.with_transform(origin, tile);
    }
    let (column, row) = ((tiles.len() % COLUMNS) as f32, (tiles.len() / COLUMNS) as f32);
    let origin = Point::new(GAP + column * (TILE + GAP) + 8.0, GAP + row * (TILE + GAP) + 8.0);
    builder.text(&self.paragraph, origin, blue_to_pink());
  }
}

fn card(builder: &mut SceneBuilder) {
  let card = RoundedRect::new(Rect::new(24.0, 24.0, 112.0, 112.0), CornerRadius::uniform(12.0));
  builder.box_shadow(
    card,
    BoxShadow::new(Point::new(0.0, 6.0), 16.0, Color::rgba(15, 23, 42, 96)).with_spread(2.0),
  );
  builder.fill(card, WHITE);
  builder.stroke(card, SLATE, Stroke::new(1.0));
}

fn rounded(builder: &mut SceneBuilder) {
  let radii = CornerRadius::new(0.0, 24.0, 48.0, 72.0);
  builder.fill(RoundedRect::new(Rect::new(8.0, 8.0, 144.0, 144.0), radii), BLUE);
}

fn star_outline(builder: &mut SceneBuilder) {
  builder.stroke(star(), PINK, Stroke::new(6.0).with_join(LineJoin::Round));
}

fn linear(builder: &mut SceneBuilder) {
  builder.fill(Rect::new(8.0, 8.0, 144.0, 144.0), blue_to_pink());
}

fn blue_to_pink() -> LinearGradientBrush {
  let stops = [GradientStop::new(0.0, BLUE), GradientStop::new(1.0, PINK)];
  LinearGradientBrush::new(RelativePoint::relative(0.0, 0.0), RelativePoint::relative(1.0, 1.0), stops)
}

fn radial(builder: &mut SceneBuilder) {
  let stops = [GradientStop::new(0.0, YELLOW), GradientStop::new(1.0, BLUE)];
  let half = RelativeScalar::relative(0.5);
  let brush = RadialGradientBrush::new(RelativePoint::relative(0.5, 0.5), half, half, stops);
  builder.fill(Rect::new(8.0, 8.0, 144.0, 144.0), brush);
}

fn rect_clip(builder: &mut SceneBuilder) {
  builder.with_clip(Rect::new(32.0, 32.0, 96.0, 96.0), stripes);
}

fn rounded_clip(builder: &mut SceneBuilder) {
  let circle = RoundedRect::new(Rect::new(8.0, 8.0, 144.0, 144.0), CornerRadius::uniform(72.0));
  builder.with_clip(circle, stripes);
}

fn path_clip(builder: &mut SceneBuilder) {
  builder.with_clip(star(), stripes);
}

fn nested_opacity(builder: &mut SceneBuilder) {
  builder.with_opacity(0.8, |builder| {
    builder.fill(Rect::new(8.0, 8.0, 96.0, 96.0), BLUE);
    builder.with_opacity(0.5, |builder| builder.fill(Rect::new(56.0, 56.0, 96.0, 96.0), PINK));
  });
}

fn stripes(builder: &mut SceneBuilder) {
  for band in 0..8 {
    let color = if band % 2 == 0 { BLUE } else { YELLOW };
    builder.fill(Rect::new(-TILE, band as f32 * 20.0, 3.0 * TILE, 20.0), color);
  }
}

fn star() -> Path {
  let point = |index: u8| {
    let angle = f32::from(index) * 4.0 * PI / 5.0 - FRAC_PI_2;
    Point::new(80.0 + 72.0 * angle.cos(), 80.0 + 72.0 * angle.sin())
  };
  (1..5)
    .fold(Path::builder(point(0)), |path, index| path.line_to(point(index)))
    .close()
    .build()
}

struct Demo {
  renderer: Renderer,
  compositor: Compositor,
  tree: UiTree,
  root: NodeId,
  primitives: Primitives,
  surface: Option<(SurfaceId, SurfaceInfo)>,
}

impl Demo {
  fn new(renderer: Renderer) -> Demo {
    let mut tree = UiTree::new();
    let root = tree.alloc(Box::new(Node));
    let style = TextStyle::new(FontFamily::default(), 28.0);
    let wrapping = ParagraphStyle {
      wrapping: TextWrapping::WrapWithOverflow,
      ..ParagraphStyle::default()
    };
    let paragraph =
      artic_text::Engine::system().paragraph("Text in a gradient", &style, &wrapping, TextLimits::width(TILE - 16.0));
    Demo {
      renderer,
      compositor: Compositor::default(),
      tree,
      root,
      primitives: Primitives {
        width: 0.0,
        height: 0.0,
        paragraph,
      },
      surface: None,
    }
  }
}

impl AppHandler for Demo {
  fn started(&mut self, cx: &mut dyn HostContext, _waker: Waker) {
    let windows = cx.windows().expect("a desktop host has windows");
    windows.open(&WindowDesc {
      title: "artic · primitives".into(),
      size: LogicalSize {
        width: Some(COLUMNS as f32 * (TILE + GAP) + GAP),
        height: Some(ROWS as f32 * (TILE + GAP) + GAP),
      },
      chrome: Chrome::System,
      resizable: Axes::BOTH,
    });
  }

  fn woken(&mut self, _cx: &mut dyn HostContext) {}

  fn surface_changed(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, info: SurfaceInfo) {
    self.surface = Some((surface, info));
    cx.request_frame(surface);
  }

  fn surface_lost(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, loss: SurfaceLoss) {
    self.renderer.surface_lost(surface);
    self.surface = None;
    if loss == SurfaceLoss::Gone {
      cx.exit();
    }
  }

  fn event(&mut self, cx: &mut dyn HostContext, event: HostEvent) {
    if let HostEvent::CloseRequested { .. } = event {
      cx.exit();
    }
  }

  fn frame(&mut self, _cx: &mut dyn HostContext, surface: SurfaceId, target: Target<'_>, _time: Duration) {
    let Some((_, info)) = self.surface.filter(|(id, _)| *id == surface) else {
      return;
    };
    let (width, height) = (info.size.width as f32 / info.scale, info.size.height as f32 / info.scale);
    if (width, height) != (self.primitives.width, self.primitives.height) {
      self.tree.mark_dirty(self.root, Affects::RENDER);
      (self.primitives.width, self.primitives.height) = (width, height);
    }
    let scene = self.compositor.compose(&mut self.tree, self.root, &mut self.primitives);
    let view = SurfaceView {
      id: surface,
      size: info.size,
      scale: info.scale,
      target,
    };
    if let Err(error) = self.renderer.render(&scene, view) {
      eprintln!("frame skipped: {error}");
    }
  }
}
