//! Golden-image tests: draw a scene into an [`Image`] and check it against a PNG checked into the
//! repository.
//!
//! A test renders one of the [`scenes`](crate::scenes), or a tree of elements, and checks the
//! image against its golden. `ARTIC_UPDATE_GOLDENS=1` writes the images as the goldens instead;
//! review the new PNGs in the pull request. On a mismatch, `<name>.actual.png` and
//! `<name>.diff.png` land beside the golden.
//!
//! ```no_run
//! use artic_render::Renderer;
//! use artic_test::{golden, scenes};
//!
//! fn box_shadow(renderer: &mut impl Renderer) {
//!   let image = golden::render(renderer, &scenes::box_shadow(), scenes::SIZE, 1.0).expect("renders");
//!   golden!("box_shadow").assert(&image);
//! }
//! ```
//!
//! A renderer's tests decide what a usable GPU is and call [`require::no_gpu`](crate::require::no_gpu)
//! instead of running a test when there's none. It skips, or fails when `ARTIC_REQUIRE_GPU` is `1`.

mod check;
mod image;

pub use check::{Golden, GoldenError, Tolerance};
pub use image::Image;

use artic_core::tree::{NodeId, UiTree};
use artic_element::ElementTypes;
use artic_platform::{Axes, DisplayId, FrameServices, Pipeline, SurfaceInfo};
use artic_render::{PhysicalSize, PixelBuffer, PixelFormat, RenderError, Renderer, Scene, SurfaceId, SurfaceView, Target};

const SURFACE: SurfaceId = SurfaceId::new(1);

/// The golden `name`: `tests/goldens/<name>.png` in the crate whose tests call it.
#[macro_export]
macro_rules! golden {
  ($name:expr) => {
    $crate::golden::Golden::at(
      ::std::path::Path::new(::core::env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
        .join(::std::format!("{}.png", $name)),
    )
  };
}

/// Draws `scene` with `renderer` into an image of `size` device pixels, at `scale` device pixels
/// per logical unit.
///
/// # Errors
///
/// Returns the renderer's error, such as [`RenderError::Unsupported`] from a renderer that can't
/// draw into `Rgba8` pixels.
pub fn render(renderer: &mut impl Renderer, scene: &Scene, size: PhysicalSize, scale: f32) -> Result<Image, RenderError> {
  pixels(size, |target| {
    renderer.render(
      scene,
      SurfaceView {
        id: SURFACE,
        size,
        scale,
        target,
      },
    )
  })
}

/// Lays out, composes and draws `root`'s tree with `renderer` into an image of `size` device
/// pixels, at `scale`. The root is arranged in the image's logical size, `size / scale`, as in a
/// window of that size. Text is shaped with the test engine.
///
/// # Errors
///
/// Returns the renderer's error.
pub fn render_tree(
  renderer: &mut impl Renderer,
  tree: &mut UiTree,
  root: NodeId,
  types: &ElementTypes,
  size: PhysicalSize,
  scale: f32,
) -> Result<Image, RenderError> {
  let info = SurfaceInfo {
    size,
    scale,
    display: DisplayId::new(1),
    refresh_rate: None,
  };
  let mut pipeline = Pipeline::new();
  let mut text = crate::text::engine();
  let mut services = FrameServices { types, text: &mut text };
  let frame = pipeline.prepare(tree, root, SURFACE, &info, &mut services, Axes::NONE);
  pixels(size, |target| pipeline.present(renderer, frame, target).map(|_| ()))
}

fn pixels(size: PhysicalSize, draw: impl FnOnce(Target<'_>) -> Result<(), RenderError>) -> Result<Image, RenderError> {
  let width = size.width as usize;
  let mut data = vec![0; width * size.height as usize * 4];
  draw(Target::Pixels(PixelBuffer {
    data: &mut data,
    stride: width * 4,
    format: PixelFormat::Rgba8,
  }))?;
  Ok(Image::new(size.width, size.height, data.as_chunks::<4>().0.to_vec()))
}

#[cfg(test)]
pub(crate) fn scratch(name: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("artic-golden-{name}-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).expect("a scratch folder");
  dir
}
