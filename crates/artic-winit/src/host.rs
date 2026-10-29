//! The host: winit's event loop, driving an `AppHandler`.

use crate::displays::Displays;
use crate::error::Error;
use crate::surfaces::Surfaces;
use crate::translate::{Translated, translate};
use crate::window::{Step, attributes, steps};
use artic_platform::{AppHandler, HostContext, HostEvent, SurfaceInfo, SurfaceLoss, Waker, WindowDesc, Windows};
use artic_render::{PhysicalSize, SurfaceId, Target, TargetKind};
use std::fmt::{Debug, Formatter};
use std::rc::Rc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::monitor::MonitorHandle;
use winit::window::{Window, WindowId};

/// The desktop host: windows, frames and the event loop on Windows, macOS and Linux (X11 and
/// Wayland), through `winit`.
///
/// It lends each window to the renderer as native handles
/// ([`TargetKind::Handles`]) and has the [`Windows`] capability.
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
    let mut driver = Driver {
      app,
      surfaces: Surfaces::default(),
      displays: Displays::default(),
      opened: Vec::new(),
      proxy: self.event_loop.create_proxy(),
      start: Instant::now(),
      started: false,
      suspended: false,
      exiting: false,
      reporting: false,
    };
    self
      .event_loop
      .run_app(&mut driver)
      .map_err(|error| Error::Loop(Box::new(error)))
  }
}

/// winit's handler: translates its callbacks into calls on the `AppHandler`.
struct Driver<A> {
  app: A,
  surfaces: Surfaces<WindowId, Rc<Window>>,
  /// Each monitor's display id, by winit's `MonitorHandle`, which compares by what each platform
  /// identifies a monitor by: the display's UUID on macOS, the `HMONITOR` on Windows, the RandR
  /// CRTC on X11 and the `wl_output` global on Wayland. Rearranging displays keeps an id, and
  /// monitors connected together always differ; a monitor plugged in again may get a new id, and
  /// on X11 one plugged in later may take the id of one unplugged from its CRTC.
  displays: Displays<MonitorHandle>,
  opened: Vec<SurfaceId>,
  proxy: EventLoopProxy<()>,
  start: Instant,
  started: bool,
  suspended: bool,
  exiting: bool,
  reporting: bool,
}

impl<A: AppHandler> Driver<A> {
  /// Calls the handler with a context, then reports the surfaces the call opened or lost. A call
  /// made while reporting leaves its surfaces to that report, so reporting never nests.
  fn call(&mut self, event_loop: &ActiveEventLoop, call: impl FnOnce(&mut A, &mut dyn HostContext)) {
    let Driver {
      app,
      surfaces,
      opened,
      suspended,
      exiting,
      ..
    } = self;
    let mut cx = Context {
      event_loop,
      surfaces,
      opened,
      suspended: *suspended,
      exiting,
    };
    call(app, &mut cx);
    if !self.reporting {
      self.reporting = true;
      self.report(event_loop);
      self.reporting = false;
    }
  }

  /// Reports newly opened surfaces and lost ones until a call opens or loses no more.
  fn report(&mut self, event_loop: &ActiveEventLoop) {
    loop {
      let opened = std::mem::take(&mut self.opened);
      let lost = self.surfaces.take_lost();
      if opened.is_empty() && lost.is_empty() {
        return;
      }
      for surface in opened {
        self.refresh(event_loop, surface, None);
      }
      for (surface, window) in lost {
        self.call(event_loop, |app, cx| app.surface_lost(cx, surface, SurfaceLoss::Gone));
        drop(window);
      }
    }
  }

  /// Reads `surface`'s facts from its window, with `size` and `scale` from an event when it has
  /// them, and reports them if they changed. The monitor is read each time, since a move between
  /// displays doesn't always send an event. Nothing is reported while suspended: resuming reports
  /// every surface.
  fn refresh(&mut self, event_loop: &ActiveEventLoop, surface: SurfaceId, changed: Option<(PhysicalSize, Option<f32>)>) {
    if self.suspended {
      return;
    }
    let Some(window) = self.surfaces.window(surface).map(Rc::clone) else {
      return;
    };
    let inner = window.inner_size();
    let (size, scale) = changed.unwrap_or((
      PhysicalSize {
        width: inner.width,
        height: inner.height,
      },
      None,
    ));
    let monitor = window.current_monitor();
    let refresh_rate = monitor
      .as_ref()
      .and_then(MonitorHandle::refresh_rate_millihertz)
      .map(|millihertz| millihertz as f32 / 1000.0);
    let info = SurfaceInfo {
      size,
      scale: scale.unwrap_or(window.scale_factor() as f32),
      display: self.displays.id(monitor),
      refresh_rate,
    };
    if self.surfaces.update(surface, info) {
      self.call(event_loop, |app, cx| app.surface_changed(cx, surface, info));
    }
  }
}

impl<A: AppHandler> ApplicationHandler for Driver<A> {
  fn resumed(&mut self, event_loop: &ActiveEventLoop) {
    if !self.started {
      self.started = true;
      let proxy = self.proxy.clone();
      let waker = Waker::new(move || {
        let _ = proxy.send_event(());
      });
      self.call(event_loop, |app, cx| app.started(cx, waker));
      return;
    }
    self.suspended = false;
    self.surfaces.forget_facts();
    self.call(event_loop, |app, cx| app.event(cx, HostEvent::Resumed));
    let surfaces: Vec<SurfaceId> = self.surfaces.windows().map(|(surface, _)| surface).collect();
    for surface in surfaces {
      if self.exiting {
        return;
      }
      if self.surfaces.window(surface).is_none() {
        continue;
      }
      self.refresh(event_loop, surface, None);
    }
  }

  fn suspended(&mut self, event_loop: &ActiveEventLoop) {
    if self.suspended {
      return;
    }
    self.suspended = true;
    let surfaces: Vec<SurfaceId> = self.surfaces.windows().map(|(surface, _)| surface).collect();
    for surface in surfaces {
      if self.surfaces.window(surface).is_none() {
        continue;
      }
      self.call(event_loop, |app, cx| app.surface_lost(cx, surface, SurfaceLoss::Suspended));
    }
    self.call(event_loop, |app, cx| app.event(cx, HostEvent::Suspended));
  }

  fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
    let Some(surface) = self.surfaces.id(window_id) else {
      return;
    };
    let Some(window) = self.surfaces.window(surface).map(Rc::clone) else {
      return;
    };
    match translate(&event, &window) {
      Translated::Changed { size, scale } => self.refresh(event_loop, surface, Some((size, scale))),
      Translated::Moved => self.refresh(event_loop, surface, None),
      Translated::Close => self.call(event_loop, |app, cx| app.event(cx, HostEvent::CloseRequested { surface })),
      Translated::Destroyed => {
        self.surfaces.close(surface);
        self.report(event_loop);
      }
      Translated::Redraw if self.suspended => {}
      Translated::Redraw => {
        self.refresh(event_loop, surface, None);
        if self.surfaces.window(surface).is_none() {
          return;
        }
        let time = self.start.elapsed();
        self.call(event_loop, |app, cx| app.frame(cx, surface, Target::Handles(&*window), time));
      }
      Translated::Ignored => {}
    }
  }

  fn user_event(&mut self, event_loop: &ActiveEventLoop, (): ()) {
    self.call(event_loop, |app, cx| app.woken(cx));
  }

  fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
    event_loop.set_control_flow(ControlFlow::Wait);
  }

  fn exiting(&mut self, event_loop: &ActiveEventLoop) {
    self.exiting = true;
    loop {
      let open = self.surfaces.drain();
      if open.is_empty() {
        return;
      }
      for (surface, window) in open {
        self.call(event_loop, |app, cx| app.surface_lost(cx, surface, SurfaceLoss::Gone));
        drop(window);
      }
    }
  }
}

/// What a handler call can ask of the host.
struct Context<'a> {
  event_loop: &'a ActiveEventLoop,
  surfaces: &'a mut Surfaces<WindowId, Rc<Window>>,
  opened: &'a mut Vec<SurfaceId>,
  suspended: bool,
  exiting: &'a mut bool,
}

impl HostContext for Context<'_> {
  fn request_frame(&mut self, surface: SurfaceId) {
    if self.suspended {
      return;
    }
    if let Some(window) = self.surfaces.window(surface) {
      window.request_redraw();
    }
  }

  fn exit(&mut self) {
    *self.exiting = true;
    self.event_loop.exit();
  }

  fn windows(&mut self) -> Option<&mut dyn Windows> {
    Some(self)
  }
}

impl Windows for Context<'_> {
  fn open(&mut self, desc: &WindowDesc) -> SurfaceId {
    if *self.exiting {
      return self.surfaces.reserve();
    }
    let attributes = attributes(desc);
    match self.event_loop.create_window(attributes) {
      Ok(window) => {
        let surface = self.surfaces.insert(window.id(), Rc::new(window));
        self.surfaces.apply(surface, desc.clone());
        self.opened.push(surface);
        surface
      }
      Err(_) => {
        let surface = self.surfaces.reserve();
        self.surfaces.lose(surface);
        surface
      }
    }
  }

  fn update(&mut self, surface: SurfaceId, desc: &WindowDesc) {
    let before = self.surfaces.apply(surface, desc.clone());
    let Some(window) = self.surfaces.window(surface) else {
      return;
    };
    let current = window.inner_size().to_logical::<f64>(window.scale_factor());
    for step in steps(before.as_ref(), desc, current) {
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
  }

  fn close(&mut self, surface: SurfaceId) {
    self.surfaces.close(surface);
  }
}
