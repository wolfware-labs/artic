use crate::{PhysicalSize, PixelFormat, TargetKind};
use std::error::Error;

/// Why a [`Renderer`](crate::Renderer) couldn't draw a frame.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
  /// The renderer can't draw on this kind of target.
  #[error("this renderer can't draw on {0:?} targets")]
  Unsupported(TargetKind),
  /// The renderer can't fill pixels stored this way.
  #[error("this renderer can't fill {0:?} pixels")]
  UnsupportedFormat(PixelFormat),
  /// The surface went away while drawing.
  #[error("the surface was lost")]
  SurfaceLost,
  /// The surface is larger on a side than the renderer can draw; a frame of a smaller size draws.
  #[error("the surface is {}x{} pixels, over the renderer's limit of {max} on a side", size.width, size.height)]
  SurfaceTooLarge {
    /// The surface's size.
    size: PhysicalSize,
    /// The longest side, in pixels, the renderer can draw.
    max: u32,
  },
  /// The renderer's library failed.
  #[error(transparent)]
  Backend(Box<dyn Error + Send + Sync>),
}

#[cfg(test)]
mod tests {
  use super::*;

  // A surface over the limit names its size and the limit.
  #[test]
  fn too_large_names_the_size_and_the_limit() {
    let error = RenderError::SurfaceTooLarge {
      size: PhysicalSize::new(20_000, 64),
      max: 16_384,
    };

    assert_eq!(
      error.to_string(),
      "the surface is 20000x64 pixels, over the renderer's limit of 16384 on a side"
    );
  }
}
