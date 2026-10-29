//! The traits a host adapter implements and calls.

use crate::{HostEvent, SurfaceInfo, SurfaceLoss, Waker, WindowDesc};
use artic_render::{SurfaceId, Target, TargetKind};
use std::time::Duration;

/// Drives the screen: owns the event loop, reports surfaces and events to an [`AppHandler`], and
/// draws frames when it asks.
///
/// An adapter crate implements it for one family of targets, such as `artic-winit` for desktop
/// windows. A launcher passes the host and a [`Runner`](crate::Runner) to [`run`](crate::run); with
/// #55 it passes the host, a renderer and the app to `artic::run`.
pub trait Host {
  /// Why the host's loop can fail, such as a lost connection to the display server;
  /// `std::convert::Infallible` for a host whose loop can't.
  type Error: std::error::Error + Send + Sync + 'static;

  /// What kind of target the host lends in [`AppHandler::frame`], checked against the renderer
  /// before the loop starts.
  fn targets(&self) -> TargetKind;

  /// Runs the loop, calling `app` until it exits.
  ///
  /// A host whose loop can't block, such as a browser's, installs its callbacks and returns at
  /// once; the callbacks keep calling `app`, which is why it must be `'static`.
  ///
  /// # Errors
  ///
  /// The host's own failure, once the surfaces have been reported lost. `app`'s failures are its
  /// own and reach it through its own channel; a host whose loop can't block returns `Ok(())` at
  /// once.
  fn run(self, app: impl AppHandler + 'static) -> Result<(), Self::Error>;
}

/// artic's side of the loop, which a host calls.
///
/// Every call gets a [`HostContext`], the only way to act on the host: asking for a frame,
/// exiting, opening a window. Calls come on the loop's thread, one at a time.
pub trait AppHandler {
  /// The loop started. `waker` wakes it from other threads.
  fn started(&mut self, cx: &mut dyn HostContext, waker: Waker);

  /// A [`Waker`] fired: run the work posted from other threads.
  fn woken(&mut self, cx: &mut dyn HostContext);

  /// A surface appeared, or one of its facts changed.
  fn surface_changed(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, info: SurfaceInfo);

  /// A surface went away, until the app resumes or for good, as `loss` says. What a renderer holds
  /// for it can be released.
  fn surface_lost(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, loss: SurfaceLoss);

  /// Something happened on the host.
  fn event(&mut self, cx: &mut dyn HostContext, event: HostEvent);

  /// Time to draw `surface`, whose `target` is lent for this call only.
  ///
  /// `time` is monotonic, from an epoch the host picks, and is its best estimate of when the frame
  /// will be presented, or now if the platform doesn't say; animations read it. It's a `Duration`
  /// rather than an `Instant`, which panics on `wasm32-unknown-unknown`.
  fn frame(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, target: Target<'_>, time: Duration);
}

/// What artic can ask of the host during an [`AppHandler`] call.
///
/// The capabilities return `None` by default: a host implements what its target has, and artic
/// decides what to do without the rest. A new capability is a new default method, so existing
/// hosts keep compiling.
pub trait HostContext {
  /// Asks for a [`frame`](AppHandler::frame) on `surface`. Requests before the frame merge into
  /// one; the host decides when it comes.
  fn request_frame(&mut self, surface: SurfaceId);

  /// Ends the loop. Surfaces still open are reported lost before it ends.
  fn exit(&mut self);

  /// Opening and closing windows, on hosts that have them.
  fn windows(&mut self) -> Option<&mut dyn Windows> {
    None
  }
}

/// Opening, updating and closing windows.
pub trait Windows {
  /// Opens a window and returns its surface's id at once. The surface arrives through
  /// [`AppHandler::surface_changed`], or [`AppHandler::surface_lost`] if the window couldn't be
  /// created; opening never fails here. While the loop is exiting, no window opens and the id is
  /// never reported.
  fn open(&mut self, desc: &WindowDesc) -> SurfaceId;

  /// Applies `desc` to the open window: its title, size, chrome and the axes the user can resize.
  /// An axis of `desc.size` that is `None` keeps its current size.
  fn update(&mut self, surface: SurfaceId, desc: &WindowDesc);

  /// Closes the window, which then reports its surface lost.
  fn close(&mut self, surface: SurfaceId);
}
