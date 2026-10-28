// Opens one empty window through the artic-winit host, asks for a frame after each frame, and
// exits after `ARTIC_SMOKE_FRAMES` frames, or when the window is closed if the variable isn't set.
// A window lost before that many frames, such as one that couldn't be created, exits with code 1.
// It draws nothing; the `frames` example draws through the frame pipeline.
//
// Run with `ARTIC_SMOKE_FRAMES=120 cargo run -p artic-examples --example smoke`.

use artic_platform::{
  AppHandler, Axes, Chrome, Host as _, HostContext, HostEvent, LogicalSize, SurfaceInfo, SurfaceLoss, Waker, WindowDesc,
};
use artic_render::{SurfaceId, Target};
use std::time::Duration;

fn main() -> Result<(), artic_winit::Error> {
  artic_winit::Host::new()?.run(Smoke::from_env());
  Ok(())
}

/// Counts frames and exits after `limit`, if there is one.
struct Smoke {
  limit: Option<u32>,
  frames: u32,
}

impl Smoke {
  /// A smoke run limited by `ARTIC_SMOKE_FRAMES`.
  fn from_env() -> Smoke {
    let limit = std::env::var("ARTIC_SMOKE_FRAMES").ok().and_then(|value| value.parse().ok());
    Smoke { limit, frames: 0 }
  }
}

impl AppHandler for Smoke {
  fn started(&mut self, cx: &mut dyn HostContext, _waker: Waker) {
    let windows = cx.windows().expect("a desktop host has windows");
    windows.open(&WindowDesc {
      title: "artic smoke".into(),
      size: LogicalSize {
        width: Some(320.0),
        height: Some(240.0),
      },
      chrome: Chrome::System,
      resizable: Axes::BOTH,
    });
  }

  fn woken(&mut self, _cx: &mut dyn HostContext) {}

  fn surface_changed(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, info: SurfaceInfo) {
    println!(
      "surface {} is {}x{} at scale {}",
      surface.get(),
      info.size.width,
      info.size.height,
      info.scale
    );
    cx.request_frame(surface);
  }

  fn surface_lost(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, _loss: SurfaceLoss) {
    println!("surface {} lost after {} frames", surface.get(), self.frames);
    if self.limit.is_some_and(|limit| self.frames < limit) {
      eprintln!("the window was lost before {} frames", self.limit.unwrap_or_default());
      std::process::exit(1);
    }
    cx.exit();
  }

  fn event(&mut self, cx: &mut dyn HostContext, event: HostEvent) {
    if let HostEvent::CloseRequested { .. } = event {
      cx.exit();
    }
  }

  fn frame(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, _target: Target<'_>, _time: Duration) {
    self.frames += 1;
    if self.limit.is_some_and(|limit| self.frames >= limit) {
      cx.exit();
    } else {
      cx.request_frame(surface);
    }
  }
}
