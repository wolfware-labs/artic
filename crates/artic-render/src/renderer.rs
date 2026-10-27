mod error;

pub use error::RenderError;

use crate::Scene;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::any::Any;

/// Draws [`Scene`]s on the surfaces a host offers.
///
/// A renderer is an adapter: `vello`, skia or a CPU rasterizer, in its own crate. It only draws; it
/// never shapes text, measures or lays out, since the scene arrives with positioned glyphs and
/// final coordinates.
pub trait Renderer {
  /// Whether this renderer can draw on targets of `kind`. Checked once, before the host starts, so
  /// a mismatched host and renderer fail with a clear error.
  fn accepts(&self, kind: TargetKind) -> bool;

  /// Draws `scene` on `surface`.
  ///
  /// # Errors
  ///
  /// Returns [`RenderError::Unsupported`] or [`RenderError::UnsupportedFormat`] for a target it
  /// can't draw on, [`RenderError::SurfaceLost`] if the surface went away, and
  /// [`RenderError::Backend`] for its library's own failures.
  fn render(&mut self, scene: &Scene, surface: SurfaceView<'_>) -> Result<(), RenderError>;

  /// Releases what the renderer holds for `surface`, such as its swapchain, once the host reports
  /// the surface lost: a closed window, a suspended mobile app. The default does nothing.
  fn surface_lost(&mut self, surface: SurfaceId) {
    let _ = surface;
  }
}

/// A view of the host surface `id` names, lent for one render: its size, scale and target.
#[derive(Debug)]
pub struct SurfaceView<'a> {
  /// The host's id for the surface, stable until the surface is lost.
  pub id: SurfaceId,
  /// The surface's size in device pixels.
  pub size: PhysicalSize,
  /// Device pixels per logical unit of the scene.
  pub scale: f32,
  /// How the renderer gets pixels onto it.
  pub target: Target<'a>,
}

/// A host's id for one of its surfaces.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct SurfaceId(u64);

impl SurfaceId {
  /// The surface the host numbers `id`.
  #[must_use]
  pub const fn new(id: u64) -> SurfaceId {
    SurfaceId(id)
  }

  /// The host's number for the surface.
  #[must_use]
  pub const fn get(self) -> u64 {
    self.0
  }
}

/// A size in device pixels.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub struct PhysicalSize {
  /// The width in pixels.
  pub width: u32,
  /// The height in pixels.
  pub height: u32,
}

impl PhysicalSize {
  /// Creates the size `width` by `height`.
  #[must_use]
  pub const fn new(width: u32, height: u32) -> PhysicalSize {
    PhysicalSize { width, height }
  }
}

/// How a renderer gets pixels onto a surface.
pub enum Target<'a> {
  /// Native handles, through `raw-window-handle`: OS windows, DRM/GBM screens, browser canvases.
  /// The renderer creates its swapchain from them and presents.
  ///
  /// The handles stay valid until the host reports the surface lost, so a renderer may keep a
  /// swapchain made from them across frames; it must drop it in [`Renderer::surface_lost`].
  Handles(&'a dyn RawHandles),
  /// A CPU pixel buffer of the surface's size, such as a framebuffer or an offscreen test image. The
  /// renderer fills it; the host presents it.
  Pixels(PixelBuffer<'a>),
  /// A type that a host and a renderer from the same adapter crate agree on.
  Custom(&'a mut dyn Any),
}

impl Target<'_> {
  /// Which kind of target this is.
  #[must_use]
  pub fn kind(&self) -> TargetKind {
    match self {
      Target::Handles(_) => TargetKind::Handles,
      Target::Pixels(_) => TargetKind::Pixels,
      Target::Custom(_) => TargetKind::Custom,
    }
  }
}

impl std::fmt::Debug for Target<'_> {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Target::Handles(_) => formatter.write_str("Handles(..)"),
      Target::Pixels(buffer) => formatter.debug_tuple("Pixels").field(buffer).finish(),
      Target::Custom(_) => formatter.write_str("Custom(..)"),
    }
  }
}

/// The kinds of [`Target`], which a renderer [`accepts`](Renderer::accepts).
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum TargetKind {
  /// [`Target::Handles`].
  Handles,
  /// [`Target::Pixels`].
  Pixels,
  /// [`Target::Custom`].
  Custom,
}

/// The native window and display handles of a surface: anything with both `raw-window-handle`
/// handles.
pub trait RawHandles: HasWindowHandle + HasDisplayHandle {}

impl<T: HasWindowHandle + HasDisplayHandle + ?Sized> RawHandles for T {}

/// CPU memory for a surface's pixels, rows from the top.
#[derive(Debug)]
pub struct PixelBuffer<'a> {
  /// The pixels; its length is at least `stride` times the surface's height.
  pub data: &'a mut [u8],
  /// Bytes from the start of one row to the start of the next.
  pub stride: usize,
  /// How each pixel is stored.
  pub format: PixelFormat,
}

/// How a pixel is stored in a [`PixelBuffer`].
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum PixelFormat {
  /// Four bytes: red, green, blue, alpha.
  Rgba8,
  /// Four bytes: blue, green, red, alpha, as most framebuffers and Windows store them.
  Bgra8,
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{Brush, Color, Command, Compositor, Layers, Nodes, SceneBuilder, SceneItem, SolidColorBrush};
  use artic_core::Object;
  use artic_core::property::PropertyVisitor;
  use artic_core::tree::{NodeId, UiTree};
  use artic_layout::geometry::Rect;
  use pretty_assertions::assert_eq;

  /// Fills `Pixels` targets with the color of the scene's first rectangle, ignoring layers.
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

  #[derive(Debug)]
  struct Node;

  impl Object for Node {
    fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
  }

  struct Red;

  impl Nodes for Red {
    fn layers(&self, _tree: &UiTree, _node: NodeId) -> Layers {
      Layers::default()
    }

    fn record(&mut self, _tree: &UiTree, _node: NodeId, builder: &mut SceneBuilder) {
      builder.fill(Rect::new(0.0, 0.0, 2.0, 1.0), Color::rgb(255, 0, 0));
    }
  }

  fn red_scene() -> Scene {
    let mut tree = UiTree::new();
    let root = tree.alloc(Box::new(Node));
    Compositor::default().compose(&mut tree, root, &mut Red)
  }

  fn surface(target: Target<'_>) -> SurfaceView<'_> {
    SurfaceView {
      id: SurfaceId::new(1),
      size: PhysicalSize::new(2, 1),
      scale: 1.0,
      target,
    }
  }

  // A renderer works as a trait object, and draws a composed scene into a pixel buffer.
  #[test]
  fn renderer_draws_a_scene_into_pixels() {
    let mut renderer: Box<dyn Renderer> = Box::new(FirstColor);
    let mut data = [0_u8; 8];
    let buffer = PixelBuffer {
      data: &mut data,
      stride: 8,
      format: PixelFormat::Rgba8,
    };

    renderer
      .render(&red_scene(), surface(Target::Pixels(buffer)))
      .expect("renders");
    renderer.surface_lost(SurfaceId::new(1));

    assert_eq!(data, [255, 0, 0, 255, 255, 0, 0, 255]);
    assert!(renderer.accepts(TargetKind::Pixels) && !renderer.accepts(TargetKind::Handles));
  }

  // A target the renderer can't draw on is an error naming its kind or format.
  #[test]
  fn unsupported_targets_are_errors() {
    let mut renderer = FirstColor;
    let mut other = 0_u8;
    let mut data = [0_u8; 8];
    let bgra = PixelBuffer {
      data: &mut data,
      stride: 8,
      format: PixelFormat::Bgra8,
    };

    let custom = renderer.render(&red_scene(), surface(Target::Custom(&mut other)));
    let format = renderer.render(&red_scene(), surface(Target::Pixels(bgra)));

    assert_eq!(
      custom.expect_err("custom").to_string(),
      "this renderer can't draw on Custom targets"
    );
    assert_eq!(format.expect_err("bgra").to_string(), "this renderer can't fill Bgra8 pixels");
  }
}
