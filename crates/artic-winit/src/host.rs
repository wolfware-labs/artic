//! The host: winit's event loop, driving an `AppHandler`.

use crate::accessibility::{Access, Activation, NoActions, Started, Stopped, Updates};
use crate::driver::{self, Driver, Loop, Monitor};
use crate::error::Error;
use crate::window::{Step, attributes};
use accesskit::TreeUpdate;
use accesskit_winit::Adapter;
use artic_platform::{AppHandler, Waker, WindowDesc};
use artic_render::TargetKind;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::monitor::MonitorHandle;
use winit::raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle};
use winit::window::{Window, WindowId};

/// The desktop host: windows, frames and the event loop on Windows, macOS and Linux (X11 and
/// Wayland), through `winit`.
///
/// It lends each window to the renderer as native handles
/// ([`TargetKind::Handles`]) and has the [`Windows`](artic_platform::Windows) and
/// [`Accessibility`](artic_platform::Accessibility) capabilities: each window has an `accesskit`
/// adapter, which hands its tree to UI Automation, NSAccessibility or AT-SPI.
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
    let mut driver = Driver::<_, HostWindow>::new(app, waker);
    self
      .event_loop
      .run_app(&mut driver)
      .map_err(|error| Error::Loop(Box::new(error)))
  }
}

impl<A: AppHandler> ApplicationHandler for Driver<A, HostWindow> {
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
  type Window = HostWindow;

  /// Creates the window hidden, makes its accessibility adapter, then shows it: accesskit needs the
  /// adapter before the window is first shown.
  fn create_window(&self, desc: &WindowDesc, waker: &Waker) -> Option<HostWindow> {
    let window = ActiveEventLoop::create_window(self, attributes(desc).with_visible(false)).ok()?;
    let activation = Arc::new(Activation::default());
    let started = Started {
      activation: Arc::clone(&activation),
      waker: waker.clone(),
    };
    let adapter = Adapter::with_direct_handlers(self, &window, started, NoActions, Stopped(Arc::clone(&activation)));
    window.set_visible(true);
    Some(HostWindow {
      window,
      access: Access::new(adapter, activation),
    })
  }

  fn exit(&self) {
    ActiveEventLoop::exit(self);
  }
}

impl Updates for Adapter {
  fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate) {
    Adapter::update_if_active(self, updater);
  }
}

/// A winit window with its accessibility.
pub(crate) struct HostWindow {
  window: Window,
  access: Access<Adapter>,
}

impl HasWindowHandle for HostWindow {
  fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
    self.window.window_handle()
  }
}

impl HasDisplayHandle for HostWindow {
  fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
    self.window.display_handle()
  }
}

impl driver::Window for HostWindow {
  type Id = WindowId;
  type Monitor = MonitorHandle;
  type Adapter = Adapter;

  fn id(&self) -> WindowId {
    self.window.id()
  }

  fn inner_size(&self) -> PhysicalSize<u32> {
    self.window.inner_size()
  }

  fn scale_factor(&self) -> f64 {
    self.window.scale_factor()
  }

  fn current_monitor(&self) -> Option<MonitorHandle> {
    self.window.current_monitor()
  }

  fn request_redraw(&self) {
    self.window.request_redraw();
  }

  fn apply(&self, step: Step) {
    let window = &self.window;
    match step {
      Step::Title(title) => window.set_title(&title),
      Step::Decorations(decorations) => window.set_decorations(decorations),
      Step::Size(size) => {
        let _ = window.request_inner_size(size);
      }
      Step::Resizable(resizable) => window.set_resizable(resizable),
      Step::Limits(min, max) => {
        window.set_min_inner_size(min);
        window.set_max_inner_size(max);
      }
    }
  }

  fn access(&self) -> &Access<Adapter> {
    &self.access
  }

  fn accessibility_event(&self, event: &WindowEvent) {
    self.access.adapter.borrow_mut().process_event(&self.window, event);
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
