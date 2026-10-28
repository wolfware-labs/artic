// The renderer the GPU tests share, or why this machine can't run them.

use artic_test::golden;
use artic_vello::{Error, Renderer};
use std::sync::{Mutex, OnceLock, PoisonError};

/// Runs `test` with the renderer the tests share, one at a time, or skips it through
/// [`golden::no_gpu`] when there's no adapter, or only WARP, Windows' software DX12 adapter, on
/// which vello's compute shaders crash.
///
/// # Panics
///
/// When the renderer finds an adapter but can't open its device or build vello's pipelines.
pub fn with_renderer(test: impl FnOnce(&mut Renderer)) {
  static RENDERER: OnceLock<Mutex<Result<Renderer, String>>> = OnceLock::new();
  let shared = RENDERER.get_or_init(|| Mutex::new(start()));
  let mut renderer = shared.lock().unwrap_or_else(PoisonError::into_inner);
  match renderer.as_mut() {
    Ok(renderer) => test(renderer),
    Err(reason) => golden::no_gpu(reason),
  }
}

/// The renderer, or why the tests skip.
fn start() -> Result<Renderer, String> {
  match Renderer::new() {
    Ok(renderer) => {
      eprintln!("using {renderer:?}");
      Ok(renderer)
    }
    Err(error @ (Error::Adapter(_) | Error::UnsupportedAdapter(_))) => Err(chain(&error)),
    Err(error) => panic!("the renderer failed to start: {}", chain(&error)),
  }
}

/// `error`'s message, then each cause.
fn chain(error: &dyn std::error::Error) -> String {
  std::iter::successors(error.source(), |cause| cause.source())
    .fold(error.to_string(), |message, cause| format!("{message}: {cause}"))
}
