//! What the host does with each of winit's callbacks, over an event loop and windows it can fake.

use crate::displays::Displays;
use crate::surfaces::Surfaces;
use crate::translate::{Translated, translate};
use crate::window::{Step, attributes, steps};
use artic_platform::{AppHandler, HostContext, HostEvent, SurfaceInfo, SurfaceLoss, Waker, WindowDesc, Windows};
use artic_render::{PhysicalSize, RawHandles, SurfaceId, Target};
use std::rc::Rc;
use std::time::Instant;
use winit::dpi::PhysicalSize as WinitSize;
use winit::event::WindowEvent;
use winit::window::WindowAttributes;

/// The event loop, as the driver uses it.
pub(crate) trait Loop {
  type Window: Window;

  /// A new window with `attributes`, or `None` if the platform refused it.
  fn create_window(&self, attributes: WindowAttributes) -> Option<Self::Window>;

  fn exit(&self);
}

/// A native window, as the driver uses it.
pub(crate) trait Window: RawHandles {
  type Id: PartialEq + Copy;
  type Monitor: Monitor;

  fn id(&self) -> Self::Id;

  fn inner_size(&self) -> WinitSize<u32>;

  fn scale_factor(&self) -> f64;

  fn current_monitor(&self) -> Option<Self::Monitor>;

  fn request_redraw(&self);

  /// Makes one change toward a new description.
  fn apply(&self, step: Step);
}

/// A display, as the driver uses it, ordered by the platform's identity for it.
pub(crate) trait Monitor: Ord {
  fn refresh_rate_millihertz(&self) -> Option<u32>;
}

/// Translates winit's callbacks into calls on the `AppHandler`.
pub(crate) struct Driver<A, W: Window> {
  app: A,
  surfaces: Surfaces<W::Id, Rc<W>>,
  /// Each monitor's display id, by winit's `MonitorHandle`, which compares by what each platform
  /// identifies a monitor by: the display's UUID on macOS, the `HMONITOR` on Windows, the RandR
  /// CRTC on X11 and the `wl_output` global on Wayland. Rearranging displays keeps an id, and
  /// monitors connected together always differ; a monitor plugged in again may get a new id, and
  /// on X11 one plugged in later may take the id of one unplugged from its CRTC.
  displays: Displays<W::Monitor>,
  opened: Vec<SurfaceId>,
  /// The waker the handler gets when the loop first resumes, and `None` once it has.
  waker: Option<Waker>,
  start: Instant,
  suspended: bool,
  exiting: bool,
  reporting: bool,
}

impl<A: AppHandler, W: Window> Driver<A, W> {
  pub(crate) fn new(app: A, waker: Waker) -> Self {
    Driver {
      app,
      surfaces: Surfaces::default(),
      displays: Displays::default(),
      opened: Vec::new(),
      waker: Some(waker),
      start: Instant::now(),
      suspended: false,
      exiting: false,
      reporting: false,
    }
  }

  /// Calls the handler with a context, then reports the surfaces the call opened or lost. A call
  /// made while reporting leaves its surfaces to that report, so reporting never nests.
  fn call<L: Loop<Window = W>>(&mut self, event_loop: &L, call: impl FnOnce(&mut A, &mut dyn HostContext)) {
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
  fn report<L: Loop<Window = W>>(&mut self, event_loop: &L) {
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
  fn refresh<L: Loop<Window = W>>(
    &mut self,
    event_loop: &L,
    surface: SurfaceId,
    changed: Option<(PhysicalSize, Option<f32>)>,
  ) {
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
      .and_then(W::Monitor::refresh_rate_millihertz)
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

  /// The loop resumed: the first time, the handler starts; after a suspension, every surface is
  /// reported again.
  pub(crate) fn resume<L: Loop<Window = W>>(&mut self, event_loop: &L) {
    if let Some(waker) = self.waker.take() {
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

  /// The loop suspended: every surface is lost until it resumes.
  pub(crate) fn suspend<L: Loop<Window = W>>(&mut self, event_loop: &L) {
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

  /// An event for the window the library calls `key`.
  pub(crate) fn handle<L: Loop<Window = W>>(&mut self, event_loop: &L, key: W::Id, event: &WindowEvent) {
    let Some(surface) = self.surfaces.id(key) else {
      return;
    };
    let Some(window) = self.surfaces.window(surface).map(Rc::clone) else {
      return;
    };
    match translate(event, || window.inner_size()) {
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

  /// A `Waker` fired.
  pub(crate) fn wake<L: Loop<Window = W>>(&mut self, event_loop: &L) {
    self.call(event_loop, |app, cx| app.woken(cx));
  }

  /// The loop is exiting: every window still open is lost, including any the handler opens
  /// meanwhile.
  pub(crate) fn drain<L: Loop<Window = W>>(&mut self, event_loop: &L) {
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
struct Context<'a, L: Loop> {
  event_loop: &'a L,
  surfaces: &'a mut Surfaces<<L::Window as Window>::Id, Rc<L::Window>>,
  opened: &'a mut Vec<SurfaceId>,
  suspended: bool,
  exiting: &'a mut bool,
}

impl<L: Loop> HostContext for Context<'_, L> {
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

impl<L: Loop> Windows for Context<'_, L> {
  fn open(&mut self, desc: &WindowDesc) -> SurfaceId {
    if *self.exiting {
      return self.surfaces.reserve();
    }
    match self.event_loop.create_window(attributes(desc)) {
      Some(window) => {
        let surface = self.surfaces.insert(window.id(), Rc::new(window));
        self.surfaces.apply(surface, desc.clone());
        self.opened.push(surface);
        surface
      }
      None => {
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
      window.apply(step);
    }
  }

  fn close(&mut self, surface: SurfaceId) {
    self.surfaces.close(surface);
  }
}
