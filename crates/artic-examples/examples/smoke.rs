// Opens one empty window through the artic-winit host, asks for a frame after each frame, and
// exits after `ARTIC_SMOKE_FRAMES` frames, or when the window is closed if the variable isn't set.
// A window lost before that many frames, such as one that couldn't be created, exits with code 1,
// and so does a value that isn't a whole number.
// Its first frame prints the display server the window is on, so CI can check which it got. It draws
// nothing; the `frames` example draws through the frame pipeline.
//
// Run with `ARTIC_SMOKE_FRAMES=120 cargo run -p artic-examples --example smoke`.

use artic_examples::Report;
use artic_platform::{
  AppHandler, Axes, Chrome, Host as _, HostContext, HostEvent, LogicalSize, SurfaceInfo, SurfaceLoss, Waker, WindowDesc,
};
use artic_render::{RawHandles, SurfaceId, Target};
use raw_window_handle::RawDisplayHandle;
use std::fmt::{self, Display, Formatter};
use std::time::Duration;

/// The variable that says how many frames to run.
const FRAMES_VARIABLE: &str = "ARTIC_SMOKE_FRAMES";

fn main() -> Result<(), Report> {
  artic_winit::Host::new()?.run(Smoke::from_env()?)?;
  Ok(())
}

/// Counts frames and exits after `limit`, if there is one.
struct Smoke {
  limit: Option<u32>,
  frames: u32,
}

impl Smoke {
  /// A smoke run limited by `ARTIC_SMOKE_FRAMES`.
  fn from_env() -> Result<Smoke, BadFrames> {
    let value = std::env::var_os(FRAMES_VARIABLE).map(|value| value.to_string_lossy().into_owned());
    Ok(Smoke {
      limit: limit(value.as_deref())?,
      frames: 0,
    })
  }
}

/// How many frames to run, from `ARTIC_SMOKE_FRAMES`: until the window is closed when it isn't set.
fn limit(value: Option<&str>) -> Result<Option<u32>, BadFrames> {
  value
    .map(|text| text.parse().map_err(|_| BadFrames(text.to_owned())))
    .transpose()
}

/// `ARTIC_SMOKE_FRAMES` set to something other than a whole number of frames.
#[derive(Debug, PartialEq, Eq)]
struct BadFrames(String);

impl Display for BadFrames {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    write!(f, "`{FRAMES_VARIABLE}` is `{}`, not a whole number of frames", self.0)
  }
}

impl std::error::Error for BadFrames {}

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

  fn frame(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, target: Target<'_>, _time: Duration) {
    self.frames += 1;
    if self.frames == 1
      && let Target::Handles(handles) = target
    {
      println!("display server: {}", display_server(handles));
    }
    if self.limit.is_some_and(|limit| self.frames >= limit) {
      cx.exit();
    } else {
      cx.request_frame(surface);
    }
  }
}

/// The display server a window's handles are for.
fn display_server(handles: &dyn RawHandles) -> &'static str {
  match handles.display_handle().map(|handle| handle.as_raw()) {
    Ok(RawDisplayHandle::Wayland(_)) => "Wayland",
    Ok(RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)) => "X11",
    Ok(RawDisplayHandle::Windows(_)) => "Windows",
    Ok(RawDisplayHandle::AppKit(_)) => "AppKit",
    Ok(_) => "another",
    Err(_) => "unavailable",
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // Without the variable the run lasts until the window is closed; with it, that many frames.
  #[test]
  fn limit_reads_a_whole_number() {
    assert_eq!(limit(None), Ok(None));
    assert_eq!(limit(Some("120")), Ok(Some(120)));
  }

  // A value that isn't a whole number of frames is reported with the variable's name.
  #[test]
  fn limit_rejects_other_text() {
    let error = limit(Some("lots")).expect_err("not a number");

    assert_eq!(
      error.to_string(),
      "`ARTIC_SMOKE_FRAMES` is `lots`, not a whole number of frames"
    );
  }
}
