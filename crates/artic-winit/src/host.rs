//! The host: winit's event loop, driving an `AppHandler`.

use crate::driver::{self, Driver, Loop, Monitor};
use crate::error::Error;
use crate::window::{Step, attributes};
use artic_platform::{AppHandler, Waker, WindowDesc};
use artic_render::TargetKind;
use std::fmt::{Debug, Formatter};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::monitor::MonitorHandle;
use winit::window::{Window, WindowId};

/// The desktop host: windows, frames and the event loop on Windows, macOS and Linux (X11 and
/// Wayland), through `winit`.
///
/// It lends each window to the renderer as native handles
/// ([`TargetKind::Handles`]) and has the [`Windows`](artic_platform::Windows) capability.
pub struct Host {
  event_loop: EventLoop<()>,
}

impl Host {
  /// A host with its event loop ready to run.
  ///
  /// # Errors
  ///
  /// Fails if the event loop can't be created, such as when no display is available.
  pub fn new() -> Result<Host, Error> {
    let event_loop = EventLoop::new().map_err(|error| Error::Start(Box::new(error)))?;
    Ok(Host { event_loop })
  }
}

impl Debug for Host {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
    formatter.debug_struct("Host").finish_non_exhaustive()
  }
}

impl artic_platform::Host for Host {
  type Error = Error;

  fn targets(&self) -> TargetKind {
    TargetKind::Handles
  }

  /// Runs winit's loop until the handler exits; it blocks on every desktop platform.
  ///
  /// # Errors
  ///
  /// [`Error::Loop`] if winit's loop ends with an error, such as a lost connection to the display
  /// server. The surfaces have been reported lost by then.
  fn run(self, app: impl AppHandler + 'static) -> Result<(), Error> {
    let proxy = self.event_loop.create_proxy();
    let waker = Waker::new(move || {
      let _ = proxy.send_event(());
    });
    let mut driver = Driver::<_, Window>::new(app, waker);
    self
      .event_loop
      .run_app(&mut driver)
      .map_err(|error| Error::Loop(Box::new(error)))
  }
}

impl<A: AppHandler> ApplicationHandler for Driver<A, Window> {
  fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    self.resume(event_loop);
  }

  fn suspended(&mut self, event_loop: &ActiveEventLoop) {
    self.suspend(event_loop);
  }

  fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
    self.handle(event_loop, window_id, &event);
  }

  fn user_event(&mut self, event_loop: &ActiveEventLoop, (): ()) {
    self.wake(event_loop);
  }

  fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
    event_loop.set_control_flow(ControlFlow::Wait);
  }

  fn exiting(&mut self, event_loop: &ActiveEventLoop) {
    self.drain(event_loop);
  }
}

impl Loop for ActiveEventLoop {
  type Window = Window;

  fn create_window(&self, desc: &WindowDesc) -> Option<Window> {
    ActiveEventLoop::create_window(self, attributes(desc)).ok()
  }

  fn exit(&self) {
    ActiveEventLoop::exit(self);
  }
}

impl driver::Window for Window {
  type Id = WindowId;
  type Monitor = MonitorHandle;

  fn id(&self) -> WindowId {
    Window::id(self)
  }

  fn inner_size(&self) -> PhysicalSize<u32> {
    Window::inner_size(self)
  }

  fn scale_factor(&self) -> f64 {
    Window::scale_factor(self)
  }

  fn current_monitor(&self) -> Option<MonitorHandle> {
    Window::current_monitor(self)
  }

  fn request_redraw(&self) {
    Window::request_redraw(self);
  }

  fn apply(&self, step: Step) {
    match step {
      Step::Title(title) => self.set_title(&title),
      Step::Decorations(decorations) => self.set_decorations(decorations),
      Step::Size(size) => {
        let _ = self.request_inner_size(size);
      }
      Step::Resizable(resizable) => self.set_resizable(resizable),
      Step::Limits(min, max) => {
        self.set_min_inner_size(min);
        self.set_max_inner_size(max);
      }
    }
  }
}

/// A `MonitorHandle` compares by what each platform identifies a monitor by: the display's UUID on
/// macOS, the `HMONITOR` on Windows, the RandR CRTC on X11 and the `wl_output` global on Wayland.
/// Rearranging displays keeps an id, and monitors connected together always differ; a monitor
/// plugged in again may get a new id, and on X11 one plugged in later may take the id of one
/// unplugged from its CRTC.
impl Monitor for MonitorHandle {
  fn refresh_rate_millihertz(&self) -> Option<u32> {
    MonitorHandle::refresh_rate_millihertz(self)
  }
}
