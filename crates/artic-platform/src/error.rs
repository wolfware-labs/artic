//! Why running a host failed.

use artic_render::{RenderError, TargetKind};

/// Why [`run`](crate::run) failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// The renderer can't draw on the kind of target the host offers.
  #[error("the renderer can't draw on {0:?} targets, which is what the host offers")]
  Unsupported(TargetKind),
  /// A frame failed to render.
  #[error("a frame failed to render")]
  Render(#[source] RenderError),
}
