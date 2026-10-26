//! The host: winit's event loop, driving an `AppHandler`.

use crate::displays::Displays;
use crate::error::Error;
use crate::surfaces::Surfaces;
use crate::translate::{Translated, translate};
use artic_platform::{AppHandler, Axes, Chrome, HostContext, HostEvent, SurfaceInfo, SurfaceLoss, Waker, WindowDesc, Windows};
use artic_render::{PhysicalSize, SurfaceId, Target, TargetKind};
use std::fmt::{Debug, Formatter};
use std::rc::Rc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::monitor::MonitorHandle;
use winit::window::{Window, WindowAttributes, WindowId};

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
    let event_loop = EventLoop::new().map_err(Error::new)?;
    Ok(Host { event_loop })
  }
}

impl Debug for Host {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
    formatter.debug_struct("Host").finish_non_exhaustive()
  }
}

impl artic_platform::Host for Host {
  fn targets(&self) -> TargetKind {
    TargetKind::Handles
  }

  /// Runs winit's loop until the handler exits; it blocks on every desktop platform.
  ///
  /// # Panics
  ///
  /// Panics if winit's loop ends with an error, such as a lost connection to the display server.
  /// The surfaces have been reported lost by then.
  fn run(self, app: impl AppHandler + 'static) {
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
      .expect("winit's event loop ended with an error");
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

/// The width a window opens at when its description sets only its height.
const FALLBACK_WIDTH: f64 = 800.0;
/// The height a window opens at when its description sets only its width.
const FALLBACK_HEIGHT: f64 = 600.0;
/// The limit on an axis the user can resize while the other is locked. winit sets both axes of a
/// limit at once, and the platforms keep it in a 32-bit integer, so the free axis needs a finite
/// value: this one is beyond any display, more than six times the width of a 16K screen.
const UNLIMITED: f64 = 100_000.0;

/// The size a window with `desc` gets, with `current` filling the axes `desc` leaves to the host,
/// or `None` when it sets neither.
fn logical_size(desc: &WindowDesc, current: LogicalSize<f64>) -> Option<LogicalSize<f64>> {
  let (width, height) = (desc.size.width, desc.size.height);
  (width.is_some() || height.is_some()).then(|| {
    LogicalSize::new(
      width.map_or(current.width, f64::from),
      height.map_or(current.height, f64::from),
    )
  })
}

/// The smallest and largest size the user can resize a window to: both locked at `size` on an axis
/// its content sizes, and none when the user can resize every axis.
fn limits(resizable: Axes, size: LogicalSize<f64>) -> (Option<LogicalSize<f64>>, Option<LogicalSize<f64>>) {
  match (resizable.width, resizable.height) {
    (true, false) => (
      Some(LogicalSize::new(0.0, size.height)),
      Some(LogicalSize::new(UNLIMITED, size.height)),
    ),
    (false, true) => (
      Some(LogicalSize::new(size.width, 0.0)),
      Some(LogicalSize::new(size.width, UNLIMITED)),
    ),
    (false, false) => (Some(size), Some(size)),
    (true, true) => (None, None),
  }
}

/// One change a window makes to follow a new description.
#[derive(Debug, PartialEq)]
enum Step {
  Title(String),
  Decorations(bool),
  Size(LogicalSize<f64>),
  Resizable(bool),
  Limits(Option<LogicalSize<f64>>, Option<LogicalSize<f64>>),
}

/// The changes, in order, that take a window of `current` size from the description `before` to
/// `after`: only what differs, and everything when nothing was applied before. A new size or
/// resizable axes clear the old limits first, so they can't clamp the new size; a size that sets
/// neither axis requests none.
fn steps(before: Option<&WindowDesc>, after: &WindowDesc, current: LogicalSize<f64>) -> Vec<Step> {
  let differs = |part: fn(&WindowDesc, &WindowDesc) -> bool| before.is_none_or(|before| part(before, after));
  let mut steps = Vec::new();
  if differs(|before, after| before.title != after.title) {
    steps.push(Step::Title(after.title.clone()));
  }
  if differs(|before, after| before.chrome != after.chrome) {
    steps.push(Step::Decorations(after.chrome == Chrome::System));
  }
  let resized = differs(|before, after| before.size != after.size);
  if resized || differs(|before, after| before.resizable != after.resizable) {
    let requested = if resized { logical_size(after, current) } else { None };
    let size = requested.unwrap_or(current);
    steps.push(Step::Limits(None, None));
    if let Some(size) = requested {
      steps.push(Step::Size(size));
    }
    steps.push(Step::Resizable(after.resizable != Axes::NONE));
    let (min, max) = limits(after.resizable, size);
    steps.push(Step::Limits(min, max));
  }
  steps
}

/// The attributes a window opens with for `desc`.
fn attributes(desc: &WindowDesc) -> WindowAttributes {
  let mut attributes = Window::default_attributes()
    .with_title(desc.title.clone())
    .with_decorations(desc.chrome == Chrome::System)
    .with_resizable(desc.resizable != Axes::NONE);
  if let Some(size) = logical_size(desc, LogicalSize::new(FALLBACK_WIDTH, FALLBACK_HEIGHT)) {
    attributes = attributes.with_inner_size(size);
    if let (Some(min), Some(max)) = limits(desc.resizable, size) {
      attributes = attributes.with_min_inner_size(min).with_max_inner_size(max);
    }
  }
  attributes
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

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use winit::dpi::Size;

  // A description becomes winit attributes: the title, the decorations its chrome asks for, the
  // set width with the fallback height, and the height locked where content sizes it.
  #[test]
  fn attributes_follow_the_description() {
    let desc = WindowDesc {
      title: "notes".into(),
      size: artic_platform::LogicalSize {
        width: Some(300.0),
        height: None,
      },
      chrome: Chrome::None,
      resizable: Axes {
        width: true,
        height: false,
      },
    };

    let attributes = attributes(&desc);

    assert_eq!(attributes.title, "notes");
    assert!(!attributes.decorations);
    assert!(attributes.resizable);
    assert_eq!(
      attributes.inner_size,
      Some(Size::Logical(LogicalSize::new(300.0, FALLBACK_HEIGHT)))
    );
    assert_eq!(
      attributes.min_inner_size,
      Some(Size::Logical(LogicalSize::new(0.0, FALLBACK_HEIGHT)))
    );
    assert_eq!(
      attributes.max_inner_size,
      Some(Size::Logical(LogicalSize::new(UNLIMITED, FALLBACK_HEIGHT)))
    );
  }

  // A window whose content sizes both axes is locked at its size, so the user can't resize it.
  #[test]
  fn locked_window_keeps_its_size_limits() {
    let size = LogicalSize::new(300.0, 200.0);

    assert_eq!(limits(Axes::NONE, size), (Some(size), Some(size)));
    assert_eq!(limits(Axes::BOTH, size), (None, None));
  }

  /// A window locked at 300 by 200, as a window whose content sizes it is.
  fn locked() -> WindowDesc {
    WindowDesc {
      title: "notes".into(),
      size: artic_platform::LogicalSize {
        width: Some(300.0),
        height: Some(200.0),
      },
      chrome: Chrome::System,
      resizable: Axes::NONE,
    }
  }

  // An update applies only what changed, so a new title leaves a size the user chose alone.
  #[test]
  fn title_change_only_sets_the_title() {
    let renamed = WindowDesc {
      title: "notes*".into(),
      ..locked()
    };

    assert_eq!(
      steps(Some(&locked()), &renamed, LogicalSize::new(300.0, 200.0)),
      [Step::Title("notes*".into())]
    );
  }

  // A window whose size goes back to the host's keeps its current size: no resize is requested.
  #[test]
  fn unset_size_requests_no_resize() {
    let unset = WindowDesc {
      size: artic_platform::LogicalSize::default(),
      resizable: Axes::BOTH,
      ..locked()
    };

    assert!(
      !steps(Some(&locked()), &unset, LogicalSize::new(300.0, 200.0))
        .iter()
        .any(|step| matches!(step, Step::Size(_)))
    );
  }

  // A locked window that grows clears its old limits before asking for the new size, so the old
  // ones can't clamp it, then locks at the new size.
  #[test]
  fn resize_clears_old_limits_first() {
    let grown = WindowDesc {
      size: artic_platform::LogicalSize {
        width: Some(300.0),
        height: Some(400.0),
      },
      ..locked()
    };
    let size = LogicalSize::new(300.0, 400.0);

    assert_eq!(
      steps(Some(&locked()), &grown, LogicalSize::new(300.0, 200.0)),
      [
        Step::Limits(None, None),
        Step::Size(size),
        Step::Resizable(false),
        Step::Limits(Some(size), Some(size)),
      ]
    );
  }

  // A description that sets no size leaves the size to the platform.
  #[test]
  fn unset_size_is_left_to_the_platform() {
    let desc = WindowDesc {
      title: String::new(),
      size: artic_platform::LogicalSize::default(),
      chrome: Chrome::System,
      resizable: Axes::BOTH,
    };

    let attributes = attributes(&desc);

    assert_eq!(attributes.inner_size, None);
    assert!(attributes.decorations);
  }
}
