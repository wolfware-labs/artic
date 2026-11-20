mod error;

pub use error::RenderError;

use crate::Scene;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::any::Any;
use std::sync::Arc;

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

  /// Whether the size has no pixels: its width or its height is 0.
  #[must_use]
  pub const fn is_empty(self) -> bool {
    self.width == 0 || self.height == 0
  }
}

/// How a renderer gets pixels onto a surface.
pub enum Target<'a> {
  /// Native handles, through `raw-window-handle`: OS windows, DRM/GBM screens, browser canvases.
  /// The renderer creates its swapchain from them and presents.
  ///
  /// The host lends the handles' owner as an [`Arc`]. A renderer may clone it into a swapchain it
  /// keeps across frames, which keeps the window alive until the swapchain drops; it must drop the
  /// swapchain in [`Renderer::surface_lost`], or a closed window stays on screen.
  Handles(&'a Arc<dyn RawHandles>),
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
/// handles that can be shared between threads and kept alive by a renderer.
pub trait RawHandles: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static {}

impl<T: HasWindowHandle + HasDisplayHandle + Send + Sync + 'static> RawHandles for T {}

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
  use crate::{Brush, Color, Command, SceneItem, SolidColorBrush};
  use artic_layout::geometry::Rect;
  use pretty_assertions::assert_eq;
  use raw_window_handle::{DisplayHandle, HandleError, WindowHandle};

  /// A window with no handles, which counts as owned while any clone of its `Arc` lives.
  struct Window;

  impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
      Err(HandleError::Unavailable)
    }
  }

  impl HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
      Err(HandleError::Unavailable)
    }
  }

  /// Keeps the handles of every `Handles` target it draws on.
  #[derive(Default)]
  struct Keeper {
    kept: Vec<Arc<dyn RawHandles>>,
  }

  impl Renderer for Keeper {
    fn accepts(&self, kind: TargetKind) -> bool {
      kind == TargetKind::Handles
    }

    fn render(&mut self, _scene: &Scene, surface: SurfaceView<'_>) -> Result<(), RenderError> {
      let Target::Handles(handles) = surface.target else {
        return Err(RenderError::Unsupported(surface.target.kind()));
      };
      self.kept.push(Arc::clone(handles));
      Ok(())
    }

    fn surface_lost(&mut self, _surface: SurfaceId) {
      self.kept.clear();
    }
  }

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

  // A size with no pixels on either axis is empty.
  #[test]
  fn a_size_without_width_or_height_is_empty() {
    assert!(PhysicalSize::new(0, 8).is_empty());
    assert!(PhysicalSize::new(8, 0).is_empty());
    assert!(!PhysicalSize::new(8, 8).is_empty());
  }

  fn red_scene() -> Scene {
    Scene::record(|builder| builder.fill(Rect::new(0.0, 0.0, 2.0, 1.0), Color::rgb(255, 0, 0)))
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
    let mut keeper = Keeper::default();
    let mut other = 0_u8;
    let mut kept = 0_u8;
    let mut data = [0_u8; 8];
    let bgra = PixelBuffer {
      data: &mut data,
      stride: 8,
      format: PixelFormat::Bgra8,
    };

    let custom = renderer.render(&red_scene(), surface(Target::Custom(&mut other)));
    let format = renderer.render(&red_scene(), surface(Target::Pixels(bgra)));
    let unkept = keeper.render(&red_scene(), surface(Target::Custom(&mut kept)));

    assert_eq!(
      custom.expect_err("custom").to_string(),
      "this renderer can't draw on Custom targets"
    );
    assert_eq!(format.expect_err("bgra").to_string(), "this renderer can't fill Bgra8 pixels");
    assert_eq!(
      unkept.expect_err("unkept").to_string(),
      "this renderer can't draw on Custom targets"
    );
    assert!(keeper.accepts(TargetKind::Handles) && !keeper.accepts(TargetKind::Custom));
  }

  // A renderer can keep the lent handles past the call, which keeps the window alive until it drops them.
  #[test]
  fn lent_handles_outlive_the_borrow() {
    let mut renderer = Keeper::default();
    let window = Arc::new(Window);
    let handles: Arc<dyn RawHandles> = window.clone();
    assert!(handles.window_handle().is_err() && handles.display_handle().is_err());

    renderer
      .render(&red_scene(), surface(Target::Handles(&handles)))
      .expect("renders");
    drop(handles);
    assert_eq!(Arc::strong_count(&window), 2);

    renderer.surface_lost(SurfaceId::new(1));
    assert_eq!(Arc::strong_count(&window), 1);
  }
}
