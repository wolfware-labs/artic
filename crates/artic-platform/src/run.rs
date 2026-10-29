//! Running a runner on a host.

use crate::{AppHandler, Error, Host, HostContext, HostEvent, Runner, SurfaceInfo, SurfaceLoss, Waker};
use artic_render::{Renderer, SurfaceId, Target};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// Runs `runner` on `host`, after checking that its renderer can draw on what the host offers.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] before starting if the renderer can't draw on the host's
/// targets. Once a host whose `run` blocks until the loop ends returns, [`Error::Render`] with the
/// first frame that failed, or else [`Error::Host`] with the host's own error; a frame's error
/// comes first when both happen, since it ended the loop. A host whose `run` returns at once returns
/// `Ok(())`, and frames that fail later reach only the `on_error` hooks. A caller that hands the
/// runner to [`Host::run`] itself gets only the host's error, and frame errors through `on_error`.
pub fn run<H: Host, R: Renderer + 'static>(host: H, runner: Runner<R>) -> Result<(), Error> {
  let kind = host.targets();
  if !runner.accepts(kind) {
    return Err(Error::Unsupported(kind));
  }
  let failure = Rc::new(RefCell::new(None));
  let ended = host.run(Reporting {
    runner,
    failure: Rc::clone(&failure),
  });
  if let Some(error) = failure.take() {
    return Err(error);
  }
  ended.map_err(|error| Error::Host(Box::new(error)))
}

/// The runner as `run` hands it to the host: it passes every call on and keeps the first error a
/// frame failed with, for `run` to return.
struct Reporting<R> {
  runner: Runner<R>,
  failure: Rc<RefCell<Option<Error>>>,
}

impl<R: Renderer> AppHandler for Reporting<R> {
  fn started(&mut self, cx: &mut dyn HostContext, waker: Waker) {
    self.runner.started(cx, waker);
  }

  fn woken(&mut self, cx: &mut dyn HostContext) {
    self.runner.woken(cx);
  }

  fn surface_changed(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, info: SurfaceInfo) {
    self.runner.surface_changed(cx, surface, info);
  }

  fn surface_lost(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, loss: SurfaceLoss) {
    self.runner.surface_lost(cx, surface, loss);
  }

  fn event(&mut self, cx: &mut dyn HostContext, event: HostEvent) {
    self.runner.event(cx, event);
  }

  fn frame(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, target: Target<'_>, time: Duration) {
    if let Some(error) = self.runner.render_frame(cx, surface, target, time) {
      self.failure.borrow_mut().get_or_insert(error);
    }
  }
}
