// The harness against renderers that need no GPU: one that paints the scene's first fill color
// everywhere, and one that draws on window handles only.

use artic_core::Object;
use artic_core::property::PropertyVisitor;
use artic_core::tree::UiTree;
use artic_element::{AccessCx, Element, ElementTypes, RenderCx};
use artic_layout::geometry::{Rect, Size};
use artic_layout::{ArrangeCx, Layoutable, MeasureCx};
use artic_render::{
  Brush, Color, Command, Layers, PhysicalSize, PixelFormat, RenderError, Renderer, Scene, SceneBuilder, SceneItem,
  SolidColorBrush, SurfaceView, Target, TargetKind,
};
use artic_test::golden::{self, Image};
use pretty_assertions::assert_eq;
use std::cell::Cell;
use std::path::Path;

struct FirstColor;

impl Renderer for FirstColor {
  fn accepts(&self, kind: TargetKind) -> bool {
    kind == TargetKind::Pixels
  }

  fn render(&mut self, scene: &Scene, surface: SurfaceView<'_>) -> Result<(), RenderError> {
    let Target::Pixels(buffer) = surface.target else {
      return Err(RenderError::Unsupported(surface.target.kind()));
    };
    if buffer.format != PixelFormat::Rgba8 {
      return Err(RenderError::UnsupportedFormat(buffer.format));
    }
    let color = scene.items().iter().find_map(|item| match item {
      SceneItem::Fragment(fragment) => fragment.commands().iter().find_map(|command| match command {
        Command::Fill {
          brush: Brush::Solid(SolidColorBrush { color, .. }),
          ..
        } => Some(*color),
        _ => None,
      }),
      _ => None,
    });
    let Color { r, g, b, a } = color.unwrap_or_default();
    for pixel in buffer.data.as_chunks_mut::<4>().0 {
      *pixel = [r, g, b, a];
    }
    Ok(())
  }
}

struct WindowsOnly;

impl Renderer for WindowsOnly {
  fn accepts(&self, kind: TargetKind) -> bool {
    kind == TargetKind::Handles
  }

  fn render(&mut self, _scene: &Scene, surface: SurfaceView<'_>) -> Result<(), RenderError> {
    Err(RenderError::Unsupported(surface.target.kind()))
  }
}

#[derive(Debug, Default)]
struct Swatch {
  seen: Cell<Option<Size>>,
}

impl Object for Swatch {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl Layoutable for Swatch {
  fn measure(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    Size::ZERO
  }

  fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
}

impl Element for Swatch {
  fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder) {
    self.seen.set(Some(cx.size()));
    scene.fill(Rect::new(0.0, 0.0, cx.size().width, cx.size().height), Color::rgb(255, 0, 0));
  }

  fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::GenericContainer)
  }

  fn layers(&self, _size: Size) -> Layers {
    Layers::default()
  }
}

artic_element::element_type!(Swatch);

fn red_scene() -> Scene {
  Scene::record(|builder| builder.fill(Rect::new(0.0, 0.0, 1.0, 1.0), Color::rgb(255, 0, 0)))
}

// `render` reads the renderer's pixels into an image of the surface's size.
#[test]
fn render_reads_the_renderers_pixels() {
  let image = golden::render(&mut FirstColor, &red_scene(), PhysicalSize::new(3, 2), 1.0).expect("renders");

  assert_eq!(image, Image::new(3, 2, vec![[255, 0, 0, 255]; 6]));
}

// A renderer that can't draw into pixels returns its error through `render`.
#[test]
fn rejected_targets_return_the_renderers_error() {
  let error = golden::render(&mut WindowsOnly, &red_scene(), PhysicalSize::new(3, 2), 1.0).expect_err("rejects");

  assert!(matches!(error, RenderError::Unsupported(TargetKind::Pixels)), "{error:?}");
}

// `render_tree` lays the root out in the image's logical size and draws it.
#[test]
fn render_tree_draws_the_root() {
  let mut tree = UiTree::new();
  let root = tree.alloc(Box::new(Swatch::default()));

  let image = golden::render_tree(
    &mut FirstColor,
    &mut tree,
    root,
    &ElementTypes::from_link_time(),
    PhysicalSize::new(64, 32),
    2.0,
  )
  .expect("renders");

  assert_eq!(image, Image::new(64, 32, vec![[255, 0, 0, 255]; 64 * 32]));
  let seen = tree.get_as::<Swatch>(root).expect("a swatch").seen.get();
  assert_eq!(seen, Some(Size::new(32.0, 16.0)));
}

// `golden!` names a PNG in the calling crate's `tests/goldens`.
#[test]
fn golden_finds_the_calling_crates_folder() {
  let golden = artic_test::golden!("box_shadow");

  assert_eq!(
    golden.path(),
    Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("tests")
      .join("goldens")
      .join("box_shadow.png")
  );
}
