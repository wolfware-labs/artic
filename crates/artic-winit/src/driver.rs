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
  fn refresh<L: Loop<Window = W>>(&mut self, event_loop: &L, surface: SurfaceId, changed: Option<(PhysicalSize, Option<f32>)>) {
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

#[cfg(test)]
mod tests {
  use super::*;
  use artic_platform::{Axes, Chrome, LogicalSize};
  use pretty_assertions::assert_eq;
  use std::cell::{Cell, RefCell};
  use std::time::Duration;
  use winit::dpi::PhysicalPosition;
  use winit::raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle};

  type Log = Rc<RefCell<Vec<String>>>;

  /// A window's state, which the test changes and reads.
  struct State {
    size: Cell<(u32, u32)>,
    monitor: Cell<Option<u8>>,
    redraws: Cell<usize>,
    steps: RefCell<Vec<Step>>,
  }

  struct FakeWindow {
    id: u32,
    state: Rc<State>,
  }

  impl HasWindowHandle for FakeWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
      Err(HandleError::Unavailable)
    }
  }

  impl HasDisplayHandle for FakeWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
      Err(HandleError::Unavailable)
    }
  }

  impl Window for FakeWindow {
    type Id = u32;
    type Monitor = FakeMonitor;

    fn id(&self) -> u32 {
      self.id
    }

    fn inner_size(&self) -> WinitSize<u32> {
      let (width, height) = self.state.size.get();
      WinitSize::new(width, height)
    }

    fn scale_factor(&self) -> f64 {
      1.0
    }

    fn current_monitor(&self) -> Option<FakeMonitor> {
      self.state.monitor.get().map(FakeMonitor)
    }

    fn request_redraw(&self) {
      self.state.redraws.set(self.state.redraws.get() + 1);
    }

    fn apply(&self, step: Step) {
      self.state.steps.borrow_mut().push(step);
    }
  }

  #[derive(PartialEq, Eq, PartialOrd, Ord)]
  struct FakeMonitor(u8);

  impl Monitor for FakeMonitor {
    fn refresh_rate_millihertz(&self) -> Option<u32> {
      Some(60_000)
    }
  }

  /// Hands out windows, keeping each one's state for the test, and refuses them while `refuse` is
  /// set.
  #[derive(Default)]
  struct FakeLoop {
    windows: RefCell<Vec<Rc<State>>>,
    refuse: Cell<bool>,
    exited: Cell<bool>,
  }

  impl FakeLoop {
    fn window(&self, index: usize) -> Rc<State> {
      Rc::clone(&self.windows.borrow()[index])
    }
  }

  impl Loop for FakeLoop {
    type Window = FakeWindow;

    fn create_window(&self, _attributes: WindowAttributes) -> Option<FakeWindow> {
      if self.refuse.get() {
        return None;
      }
      let state = Rc::new(State {
        size: Cell::new((30, 20)),
        monitor: Cell::new(Some(0)),
        redraws: Cell::new(0),
        steps: RefCell::new(Vec::new()),
      });
      let mut windows = self.windows.borrow_mut();
      windows.push(Rc::clone(&state));
      Some(FakeWindow {
        id: windows.len() as u32 - 1,
        state,
      })
    }

    fn exit(&self) {
      self.exited.set(true);
    }
  }

  /// Records each call as a line, then runs `react` with the line and the context.
  struct Script<R> {
    log: Log,
    react: R,
  }

  impl<R: FnMut(&str, &mut dyn HostContext)> Script<R> {
    fn record(&mut self, cx: &mut dyn HostContext, line: String) {
      self.log.borrow_mut().push(line.clone());
      (self.react)(&line, cx);
    }
  }

  impl<R: FnMut(&str, &mut dyn HostContext)> AppHandler for Script<R> {
    fn started(&mut self, cx: &mut dyn HostContext, _waker: Waker) {
      self.record(cx, "started".into());
    }

    fn woken(&mut self, cx: &mut dyn HostContext) {
      self.record(cx, "woken".into());
    }

    fn surface_changed(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, info: SurfaceInfo) {
      let SurfaceInfo {
        size,
        scale,
        display,
        refresh_rate,
      } = info;
      let line = format!(
        "changed {} {}x{} @{scale} on {display:?} {refresh_rate:?}",
        surface.get(),
        size.width,
        size.height
      );
      self.record(cx, line);
    }

    fn surface_lost(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, loss: SurfaceLoss) {
      self.record(cx, format!("lost {} {loss:?}", surface.get()));
    }

    fn event(&mut self, cx: &mut dyn HostContext, event: HostEvent) {
      let line = match event {
        HostEvent::CloseRequested { surface } => format!("close {}", surface.get()),
        other => format!("{other:?}").to_lowercase(),
      };
      self.record(cx, line);
    }

    fn frame(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, _target: Target<'_>, _time: Duration) {
      self.record(cx, format!("frame {}", surface.get()));
    }
  }

  /// A driver whose app runs `react`, and its log.
  fn driver<R: FnMut(&str, &mut dyn HostContext)>(react: R) -> (Driver<Script<R>, FakeWindow>, Log) {
    let log = Log::default();
    let app = Script {
      log: Rc::clone(&log),
      react,
    };
    (Driver::new(app, Waker::new(|| {})), log)
  }

  /// A started driver, its loop and its log.
  struct Fixture<R> {
    driver: Driver<Script<R>, FakeWindow>,
    event_loop: FakeLoop,
    log: Log,
  }

  /// A started driver whose app opened `windows` windows, with an empty log; its app runs `react`
  /// on every later call.
  fn running(
    windows: usize,
    mut react: impl FnMut(&str, &mut dyn HostContext),
  ) -> Fixture<impl FnMut(&str, &mut dyn HostContext)> {
    let (mut driver, log) = driver(move |line, cx| {
      if line == "started" {
        (0..windows).for_each(|_| _ = open(cx));
      } else {
        react(line, cx);
      }
    });
    let event_loop = FakeLoop::default();
    driver.resume(&event_loop);
    log.borrow_mut().clear();
    Fixture { driver, event_loop, log }
  }

  fn desc(title: &str) -> WindowDesc {
    WindowDesc {
      title: title.into(),
      size: LogicalSize {
        width: Some(30.0),
        height: Some(20.0),
      },
      chrome: Chrome::System,
      resizable: Axes::BOTH,
    }
  }

  fn open(cx: &mut dyn HostContext) -> SurfaceId {
    cx.windows().expect("the driver has windows").open(&desc("main"))
  }

  fn take(log: &Log) -> Vec<String> {
    std::mem::take(&mut *log.borrow_mut())
  }

  fn changed(surface: u64, size: &str, display: u64) -> String {
    format!("changed {surface} {size} @1 on DisplayId({display}) Some(60.0)")
  }

  // The first resume starts the app, and its first window reports its facts after `started`
  // returns; a later resume reports them again.
  #[test]
  fn the_first_resume_starts() {
    let (mut driver, log) = driver(|line, cx| {
      if line == "started" {
        open(cx);
      }
    });
    let event_loop = FakeLoop::default();

    driver.resume(&event_loop);
    assert_eq!(take(&log), ["started".to_string(), changed(1, "30x20", 0)]);
    driver.resume(&event_loop);
    assert_eq!(take(&log), ["resumed".to_string(), changed(1, "30x20", 0)]);
  }

  // A redraw reports the facts that changed since the last report, then draws.
  #[test]
  fn a_redraw_reports_new_facts_then_draws() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(1, |_, _| {});

    event_loop.window(0).size.set((40, 20));
    driver.handle(&event_loop, 0, &WindowEvent::RedrawRequested);
    assert_eq!(take(&log), [changed(1, "40x20", 0), "frame 1".into()]);
    driver.handle(&event_loop, 0, &WindowEvent::RedrawRequested);
    assert_eq!(take(&log), ["frame 1"]);
  }

  // A window the app closes when told of its new facts gets no frame.
  #[test]
  fn a_window_closed_while_refreshing_draws_no_frame() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(1, |line, cx| {
      if line.starts_with("changed 1 40x20") {
        cx.windows().expect("the driver has windows").close(SurfaceId::new(1));
      }
    });

    event_loop.window(0).size.set((40, 20));
    driver.handle(&event_loop, 0, &WindowEvent::RedrawRequested);

    assert_eq!(take(&log), [changed(1, "40x20", 0), "lost 1 Gone".into()]);
  }

  // A move reads the window's monitor, and reports only when it changed.
  #[test]
  fn a_moved_window_reads_its_monitor() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(1, |_, _| {});
    let moved = WindowEvent::Moved(PhysicalPosition::new(1, 2));

    event_loop.window(0).monitor.set(Some(1));
    driver.handle(&event_loop, 0, &moved);
    assert_eq!(take(&log), [changed(1, "30x20", 1)]);
    driver.handle(&event_loop, 0, &moved);
    assert_eq!(take(&log), Vec::<String>::new());
  }

  // A resize reports the size it carries, not the one the window last had.
  #[test]
  fn a_resize_carries_its_size() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(1, |_, _| {});

    driver.handle(&event_loop, 0, &WindowEvent::Resized(WinitSize::new(50, 10)));

    assert_eq!(take(&log), [changed(1, "50x10", 0)]);
  }

  // Suspending loses every surface once, then reports, draws and schedules nothing.
  #[test]
  fn suspended_reports_and_draws_nothing() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(2, |line, cx| {
      if line == "woken" {
        cx.request_frame(SurfaceId::new(1));
      }
    });

    driver.suspend(&event_loop);
    assert_eq!(take(&log), ["lost 1 Suspended", "lost 2 Suspended", "suspended"]);
    driver.suspend(&event_loop);
    driver.handle(&event_loop, 0, &WindowEvent::RedrawRequested);
    driver.handle(&event_loop, 0, &WindowEvent::Resized(WinitSize::new(50, 10)));
    driver.wake(&event_loop);
    assert_eq!(
      (take(&log), event_loop.window(0).redraws.get()),
      (vec!["woken".to_string()], 0)
    );
  }

  // A window closed while the others are lost to a suspension is lost for good, never also as
  // suspended.
  #[test]
  fn a_window_closed_while_suspending_is_gone() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(2, |line, cx| {
      if line == "lost 1 Suspended" {
        cx.windows().expect("the driver has windows").close(SurfaceId::new(2));
      }
    });

    driver.suspend(&event_loop);

    assert_eq!(take(&log), ["lost 1 Suspended", "lost 2 Gone", "suspended"]);
  }

  // Resuming stops reporting surfaces once the app asks to exit.
  #[test]
  fn resuming_stops_when_the_app_exits() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(2, |line, cx| {
      if line == "resumed" {
        cx.exit();
      }
    });

    driver.resume(&event_loop);

    assert_eq!((take(&log), event_loop.exited.get()), (vec!["resumed".to_string()], true));
  }

  // A window the platform destroyed is lost, dropped once its loss is reported, and its events
  // are ignored after.
  #[test]
  fn a_destroyed_window_is_lost() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(1, |_, _| {});
    let state = event_loop.window(0);

    driver.handle(&event_loop, 0, &WindowEvent::Destroyed);
    assert_eq!((take(&log), Rc::strong_count(&state)), (vec!["lost 1 Gone".to_string()], 2));
    driver.handle(&event_loop, 0, &WindowEvent::RedrawRequested);
    assert_eq!(take(&log), Vec::<String>::new());
  }

  // A close request reaches the app, which decides whether to close.
  #[test]
  fn a_close_request_reaches_the_app() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(1, |_, _| {});

    driver.handle(&event_loop, 0, &WindowEvent::CloseRequested);

    assert_eq!(take(&log), ["close 1"]);
  }

  // A window the platform refuses is reported lost after the call that opened it.
  #[test]
  fn a_refused_window_is_lost() {
    let (mut driver, log) = driver(|line, cx| {
      if line == "started" {
        open(cx);
      }
    });
    let event_loop = FakeLoop::default();
    event_loop.refuse.set(true);

    driver.resume(&event_loop);

    assert_eq!(take(&log), ["started", "lost 1 Gone"]);
  }

  // A window opened while losses are reported is reported after them, never in the middle.
  #[test]
  fn windows_opened_while_reporting_are_reported_after() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(2, |line, cx| match line {
      "woken" => {
        let windows = cx.windows().expect("the driver has windows");
        windows.close(SurfaceId::new(1));
        windows.close(SurfaceId::new(2));
      }
      "lost 1 Gone" => _ = open(cx),
      _ => {}
    });

    driver.wake(&event_loop);

    assert_eq!(
      take(&log),
      [
        "woken".to_string(),
        "lost 1 Gone".into(),
        "lost 2 Gone".into(),
        changed(3, "30x20", 0)
      ]
    );
  }

  // Exiting loses every open window, and a window the app opens meanwhile is never created.
  #[test]
  fn exiting_loses_every_window() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(2, |line, cx| {
      if line.starts_with("lost") {
        open(cx);
      }
    });

    driver.drain(&event_loop);

    assert_eq!(
      (take(&log), event_loop.windows.borrow().len()),
      (vec!["lost 1 Gone".to_string(), "lost 2 Gone".into()], 2)
    );
  }

  // An update makes only the changes that take the window to its new description.
  #[test]
  fn an_update_applies_only_what_changed() {
    let Fixture {
      mut driver,
      event_loop,
      log: _,
    } = running(1, |line, cx| {
      if line == "woken" {
        cx.windows()
          .expect("the driver has windows")
          .update(SurfaceId::new(1), &desc("notes"));
      }
    });

    driver.wake(&event_loop);

    assert_eq!(*event_loop.window(0).steps.borrow(), [Step::Title("notes".into())]);
  }

  // A wake reaches the app, and a frame it asks for schedules a redraw.
  #[test]
  fn a_wake_reaches_the_app() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(1, |line, cx| {
      if line == "woken" {
        cx.request_frame(SurfaceId::new(1));
      }
    });

    driver.wake(&event_loop);

    assert_eq!(
      (take(&log), event_loop.window(0).redraws.get()),
      (vec!["woken".to_string()], 1)
    );
  }

  // Events for a window the driver doesn't know are ignored.
  #[test]
  fn events_for_unknown_windows_are_ignored() {
    let Fixture {
      mut driver,
      event_loop,
      log,
    } = running(1, |_, _| {});

    driver.handle(&event_loop, 7, &WindowEvent::RedrawRequested);

    assert_eq!(take(&log), Vec::<String>::new());
  }
}
