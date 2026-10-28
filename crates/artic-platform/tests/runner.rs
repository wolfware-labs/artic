// The runner, driven as a host would: scheduling, frames, hooks and surfaces.

mod common;

use artic_core::property::Property;
use artic_core::reactive::{Signal, effect};
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::{Desktop, SizeToContent};
use artic_layout::geometry::Size;
use artic_platform::{AppHandler, Axes, Chrome, FrameStats, LogicalSize, Runner, Waker, WindowDesc};
use artic_render::SurfaceId;
use common::{
  Context, Counter, Leaf, LeafHandles, Recorder, Wrap, application, column, engine, info, label, leaf, pixels, surface, target,
};
use pretty_assertions::assert_eq;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use artic_platform::{Error, Host, HostEvent, SurfaceLoss, run};
use artic_render::{RenderError, TargetKind};
const FIRST: SurfaceId = SurfaceId::new(1);

/// Each surface's last frame stats, recorded through the `after_present` hook.
#[derive(Clone, Default)]
struct StatsLog(Rc<RefCell<HashMap<NodeId, FrameStats>>>);

impl StatsLog {
  fn attach(runner: &mut Runner<Counter>) -> StatsLog {
    let log = StatsLog::default();
    let stats = log.0.clone();
    runner.hooks().after_present(move |_, root, frame| {
      stats.borrow_mut().insert(root, *frame);
    });
    log
  }

  fn last(&self, root: NodeId) -> Option<FrameStats> {
    self.0.borrow().get(&root).copied()
  }
}

/// A runner with one window holding two leaves, started, sized and drawn once.
struct Fixture {
  runner: Runner<Counter>,
  stats: StatsLog,
  surface: NodeId,
  cx: Context,
  renderer: Counter,
  top: LeafHandles,
  title: Property<String>,
  is_open: Property<bool>,
}

impl Fixture {
  fn new(configure: impl FnOnce(&mut Runner<Counter>)) -> Fixture {
    let mut tree = UiTree::new();
    let top = leaf(&mut tree, 20.0);
    let bottom = leaf(&mut tree, 20.0);
    let (content, _) = column(&mut tree, &[top.id, bottom.id]);
    let (application, surfaces) = application(&mut tree, &[("main", content)]);
    let (title, is_open) = (surface(&tree, surfaces[0]).title, surface(&tree, surfaces[0]).is_open);
    let renderer = Counter::default();
    let mut runner = Runner::new(tree, renderer.clone(), Box::new(engine()), application);
    let stats = StatsLog::attach(&mut runner);
    configure(&mut runner);
    let mut cx = Context::default();
    runner.started(&mut cx, Waker::new(|| {}));
    runner.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
    let mut fixture = Fixture {
      runner,
      stats,
      surface: surfaces[0],
      cx,
      renderer,
      top,
      title,
      is_open,
    };
    fixture.frame();
    fixture.cx.clear();
    fixture
  }

  /// Runs a frame for the window and returns its stats.
  fn frame(&mut self) -> FrameStats {
    let mut buffer = pixels();
    self.runner.frame(&mut self.cx, FIRST, target(&mut buffer), Duration::ZERO);
    self.stats.last(self.surface).expect("the window drew")
  }

  /// Settles a change made outside any call, as a host event would.
  fn settle(&mut self) {
    self.runner.woken(&mut self.cx);
  }
}

// After a frame with nothing changed, an unrequested frame measures and records nothing, presents
// the last scene, and asks for no further frame, so the loop sleeps.
#[test]
fn clean_frame_is_skipped() {
  let mut fixture = Fixture::new(|_| {});

  let first = fixture.frame();
  let second = fixture.frame();

  assert_eq!(
    [first, second],
    [FrameStats {
      rendered: true,
      ..FrameStats::default()
    }; 2]
  );
  assert!(fixture.cx.frames.is_empty());
}

// An OS redraw, a frame nobody asked for, presents the last scene again.
#[test]
fn os_redraw_presents_the_last_scene() {
  let mut fixture = Fixture::new(|_| {});
  let before = fixture.renderer.rendered.get();

  fixture.frame();

  assert_eq!(fixture.renderer.rendered.get(), before + 1);
}

// A render-only change asks for a frame, which records the one node and lays out nothing.
#[test]
fn render_only_change_skips_layout() {
  let mut fixture = Fixture::new(|_| {});

  fixture.top.color.set(0xff0000);
  fixture.settle();
  let stats = fixture.frame();

  assert_eq!(fixture.cx.frames, [FIRST]);
  assert_eq!(
    stats,
    FrameStats {
      measured: 0,
      arranged: 0,
      recorded: 1,
      rendered: true
    }
  );
}

// A layout change measures the changed node and its ancestors up to the surface, arranges them
// and the parent's children, and records only the node whose size changed.
#[test]
fn layout_change_remeasures_subtree() {
  let mut fixture = Fixture::new(|_| {});

  fixture.top.height.set(30.0);
  fixture.settle();
  let stats = fixture.frame();

  assert_eq!(
    stats,
    FrameStats {
      measured: 3,
      arranged: 4,
      recorded: 1,
      rendered: true
    }
  );
}

// A resize lays out the whole window again.
#[test]
fn resize_invalidates_root() {
  let mut fixture = Fixture::new(|_| {});

  fixture.runner.surface_changed(&mut fixture.cx, FIRST, info(48, 64, 1.0));
  let stats = fixture.frame();

  assert!(fixture.cx.frames.contains(&FIRST));
  assert_eq!((stats.measured, stats.arranged), (4, 4));
}

// A new scale at the same logical size, as when a window moves to a monitor with another scale,
// arranges the window again and measures nothing.
#[test]
fn scale_change_measures_nothing() {
  let mut fixture = Fixture::new(|_| {});

  fixture.runner.surface_changed(&mut fixture.cx, FIRST, info(128, 128, 2.0));
  let stats = fixture.frame();

  assert!(fixture.cx.frames.contains(&FIRST));
  assert_eq!((stats.measured, stats.arranged), (0, 4));
}

// Nodes layout gave a new size are recorded again although no property changed.
#[test]
fn resized_node_is_recorded_again() {
  let mut fixture = Fixture::new(|_| {});

  fixture.runner.surface_changed(&mut fixture.cx, FIRST, info(48, 64, 1.0));
  let stats = fixture.frame();

  assert_eq!(stats.recorded, 4);
}

// A write a before-layout hook makes, such as the style pass's, lands in the same frame and asks
// for no second one.
#[test]
fn style_write_lands_in_same_frame() {
  let color = Rc::new(Cell::new(None));
  let hook_color = Rc::clone(&color);
  let mut fixture = Fixture::new(|runner| {
    runner.hooks().before_layout(move |_, _, _, _| {
      if let Some(property) = hook_color.take() {
        common::set_color(property, 0x00ff00);
      }
    });
  });
  color.set(Some(fixture.top.color));

  let stats = fixture.frame();

  assert_eq!(stats.recorded, 1);
  assert!(fixture.cx.frames.is_empty());
}

// An effect that a hook's write triggers runs before the frame decides what's dirty, so its own
// write lands in the same frame too.
#[test]
fn hook_write_effects_land_in_same_frame() {
  let signal = Signal::new(0_u32);
  let armed = Rc::new(Cell::new(false));
  let hook_armed = Rc::clone(&armed);
  let mut fixture = Fixture::new(|runner| {
    runner.hooks().before_layout(move |_, _, _, _| {
      if hook_armed.replace(false) {
        signal.set(1);
      }
    });
  });
  let color = fixture.top.color;
  effect(move || {
    if signal.get() > 0 {
      color.set(0x0000ff);
    }
  });
  armed.set(true);

  let stats = fixture.frame();

  assert_eq!(stats.recorded, 1);
}

// A write after the frame was presented asks for exactly one more frame.
#[test]
fn write_after_present_requests_one_frame() {
  let color = Rc::new(Cell::new(None));
  let hook_color = Rc::clone(&color);
  let mut fixture = Fixture::new(|runner| {
    runner.hooks().after_present(move |_, _, _| {
      if let Some(property) = hook_color.take() {
        common::set_color(property, 0xffffff);
      }
    });
  });
  color.set(Some(fixture.top.color));

  fixture.top.height.set(25.0);
  fixture.settle();
  fixture.cx.clear();
  fixture.frame();
  let after_write = std::mem::take(&mut fixture.cx.frames);
  fixture.frame();

  assert_eq!(after_write, [FIRST]);
  assert!(fixture.cx.frames.is_empty());
}

// A hook that wants another frame, such as a running animation, gets one after each frame.
#[test]
fn wants_frame_requests_another() {
  let mut fixture = Fixture::new(|runner| runner.hooks().wants_frame(|| true));

  fixture.frame();

  assert_eq!(fixture.cx.frames, [FIRST]);
}

// Two windows keep their own layout and drawings: a change in one doesn't make the other record
// again.
#[test]
fn windows_keep_their_own_caches() {
  let mut tree = UiTree::new();
  let left = leaf(&mut tree, 20.0);
  let right = leaf(&mut tree, 20.0);
  let (first_root, _) = column(&mut tree, &[left.id]);
  let (second_root, _) = column(&mut tree, &[right.id]);
  let (application, surfaces) = application(&mut tree, &[("first", first_root), ("second", second_root)]);
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let log = StatsLog::attach(&mut runner);
  let mut cx = Context::default();
  let second = SurfaceId::new(2);
  runner.started(&mut cx, Waker::new(|| {}));
  for surface in [FIRST, second] {
    runner.surface_changed(&mut cx, surface, info(64, 64, 1.0));
  }
  let mut buffer = pixels();
  for surface in [FIRST, second] {
    runner.frame(&mut cx, surface, target(&mut buffer), Duration::ZERO);
  }

  left.color.set(0xff0000);
  runner.woken(&mut cx);
  for surface in [FIRST, second] {
    runner.frame(&mut cx, surface, target(&mut buffer), Duration::ZERO);
  }

  assert_eq!(log.last(surfaces[0]).unwrap().recorded, 1);
  assert_eq!(log.last(surfaces[1]).unwrap().recorded, 0);
}

// A child of a hidden panel resized while hidden is recorded again once the panel shows, though
// the child itself never changed.
#[test]
fn node_resized_while_hidden_is_recorded_when_shown() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 20.0);
  let (inner, inner_visible) = column(&mut tree, &[child.id]);
  let (root, _) = column(&mut tree, &[inner]);
  let (application, surfaces) = application(&mut tree, &[("main", root)]);
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let log = StatsLog::attach(&mut runner);
  let mut cx = Context::default();
  let mut buffer = pixels();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
  inner_visible.set(false);
  runner.woken(&mut cx);
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
  runner.surface_changed(&mut cx, FIRST, info(48, 64, 1.0));
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  inner_visible.set(true);
  runner.woken(&mut cx);
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  assert_eq!(log.last(surfaces[0]).unwrap().recorded, 2);
}

// A frame before the surface's first `surface_changed` has no size to lay out in, so it draws
// nothing.
#[test]
fn frame_before_surface_info_is_ignored() {
  let mut tree = UiTree::new();
  let (root, _) = column(&mut tree, &[]);
  let (application, surfaces) = application(&mut tree, &[("main", root)]);
  let renderer = Counter::default();
  let mut runner = Runner::new(tree, renderer.clone(), Box::new(engine()), application);
  let log = StatsLog::attach(&mut runner);
  let mut cx = Context::default();
  let mut buffer = pixels();
  runner.started(&mut cx, Waker::new(|| {}));

  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  assert_eq!(log.last(surfaces[0]), None);
  assert_eq!(renderer.rendered.get(), 0);
}

// The first frame for a surface draws in full even when the host sends it unasked.
#[test]
fn first_frame_without_a_request_draws() {
  let mut tree = UiTree::new();
  let child = leaf(&mut tree, 20.0);
  let (root, _) = column(&mut tree, &[child.id]);
  let (application, surfaces) = application(&mut tree, &[("main", root)]);
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let log = StatsLog::attach(&mut runner);
  let mut cx = Context::default();
  let mut buffer = pixels();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));

  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  assert_eq!(log.last(surfaces[0]).unwrap().recorded, 3);
}

// Closing the last surface's window writes IsOpen = false, closes it through the host and exits.
#[test]
fn last_surface_closing_exits() {
  let mut fixture = Fixture::new(|_| {});

  fixture
    .runner
    .event(&mut fixture.cx, HostEvent::CloseRequested { surface: FIRST });

  assert!(!fixture.is_open.get());
  assert_eq!(fixture.cx.closed, [FIRST]);
  assert!(fixture.cx.exited);
}

// A closed window's loss, which the host reports after the close, releases what the renderer kept
// for it, and the runner forgets the window.
#[test]
fn closed_window_is_released_from_the_renderer() {
  let mut fixture = Fixture::new(|_| {});

  fixture
    .runner
    .event(&mut fixture.cx, HostEvent::CloseRequested { surface: FIRST });
  fixture.runner.surface_lost(&mut fixture.cx, FIRST, SurfaceLoss::Gone);

  assert_eq!(*fixture.renderer.lost.borrow(), [FIRST]);
  let rendered = fixture.renderer.rendered.get();
  let mut buffer = pixels();
  fixture
    .runner
    .frame(&mut fixture.cx, FIRST, target(&mut buffer), Duration::ZERO);
  assert_eq!(fixture.renderer.rendered.get(), rendered);
}

// A lost surface makes the renderer drop what it kept and asks for a frame that rebuilds it.
#[test]
fn surface_lost_render_error_resets_the_renderer() {
  let mut fixture = Fixture::new(|_| {});
  *fixture.renderer.fail.borrow_mut() = Some(RenderError::SurfaceLost);

  fixture.frame();

  assert_eq!(*fixture.renderer.lost.borrow(), [FIRST]);
  assert_eq!(fixture.cx.frames, [FIRST]);
}

/// A host whose surface is a pixel buffer and whose loop runs one frame.
struct OneFrame;

impl Host for OneFrame {
  type Error = std::convert::Infallible;

  fn targets(&self) -> TargetKind {
    TargetKind::Pixels
  }

  fn run(self, mut app: impl AppHandler + 'static) -> Result<(), std::convert::Infallible> {
    let mut cx = Context::default();
    let mut buffer = pixels();
    app.started(&mut cx, Waker::new(|| {}));
    app.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
    app.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
    Ok(())
  }
}

/// A host that offers only window handles.
struct HandlesOnly;

impl Host for HandlesOnly {
  type Error = std::convert::Infallible;

  fn targets(&self) -> TargetKind {
    TargetKind::Handles
  }

  fn run(self, _app: impl AppHandler + 'static) -> Result<(), std::convert::Infallible> {
    panic!("a host with a mismatched renderer must not start");
  }
}

/// A runner over a one-window tree drawing with `renderer`.
fn one_window(renderer: Counter) -> Runner<Counter> {
  let mut tree = UiTree::new();
  let (root, _) = column(&mut tree, &[]);
  let (application, _) = application(&mut tree, &[("main", root)]);
  Runner::new(tree, renderer, Box::new(engine()), application)
}

// A frame that fails to render ends the loop, and `run` returns the error.
#[test]
fn render_error_is_returned_from_run() {
  let renderer = Counter::default();
  *renderer.fail.borrow_mut() = Some(RenderError::Backend("no device".into()));

  let result = run(OneFrame, one_window(renderer));

  assert!(matches!(result, Err(Error::Render(RenderError::Backend(_)))));
}

// A failed frame reaches the error hooks, the way an app on a host whose `run` returns at once
// learns of it.
#[test]
fn render_error_reaches_on_error() {
  let renderer = Counter::default();
  *renderer.fail.borrow_mut() = Some(RenderError::Backend("no device".into()));
  let seen = Rc::new(Cell::new(0));
  let hook_seen = Rc::clone(&seen);
  let mut runner = one_window(renderer);
  runner.hooks().on_error(move |_| hook_seen.set(hook_seen.get() + 1));

  let _ = run(OneFrame, runner);

  assert_eq!(seen.get(), 1);
}

// A renderer that can't draw on what the host offers is rejected before the host starts.
#[test]
fn mismatched_renderer_is_rejected() {
  let result = run(HandlesOnly, one_window(Counter::default()));

  assert!(matches!(result, Err(Error::Unsupported(TargetKind::Handles))));
}

/// A host whose loop fails before calling the app.
struct LoopFails;

impl Host for LoopFails {
  type Error = std::io::Error;

  fn targets(&self) -> TargetKind {
    TargetKind::Pixels
  }

  fn run(self, _app: impl AppHandler + 'static) -> Result<(), std::io::Error> {
    Err(std::io::Error::other("display lost"))
  }
}

/// A host that runs one frame, then fails.
struct FailsAfterAFrame;

impl Host for FailsAfterAFrame {
  type Error = std::io::Error;

  fn targets(&self) -> TargetKind {
    TargetKind::Pixels
  }

  fn run(self, app: impl AppHandler + 'static) -> Result<(), std::io::Error> {
    let Ok(()) = OneFrame.run(app);
    Err(std::io::Error::other("display lost"))
  }
}

// A host whose loop fails returns its error from `run`, as the source of `Error::Host`.
#[test]
fn host_error_is_returned_from_run() {
  let result = run(LoopFails, one_window(Counter::default()));

  let Err(Error::Host(source)) = result else {
    panic!("{result:?}")
  };
  let cause = source.downcast_ref::<std::io::Error>().expect("the host's error");
  assert_eq!(
    (cause.kind(), cause.to_string()),
    (std::io::ErrorKind::Other, "display lost".to_owned())
  );
  assert_eq!(Error::Host(source).to_string(), "the host failed");
}

// When a frame failed and then the host did, `run` returns the frame's error, which is why the loop
// ended.
#[test]
fn render_error_comes_before_the_host_error() {
  let renderer = Counter::default();
  *renderer.fail.borrow_mut() = Some(RenderError::Backend("no device".into()));

  let result = run(FailsAfterAFrame, one_window(renderer));

  assert!(matches!(result, Err(Error::Render(RenderError::Backend(_)))), "{result:?}");
}

// A loop that ends with neither the host nor a frame failing returns `Ok`.
#[test]
fn run_that_draws_returns_ok() {
  assert!(run(OneFrame, one_window(Counter::default())).is_ok());
}

/// A host that draws once, then lends two frames that fail.
struct FailsTwice {
  fail: Rc<RefCell<Option<RenderError>>>,
}

impl Host for FailsTwice {
  type Error = std::convert::Infallible;

  fn targets(&self) -> TargetKind {
    TargetKind::Pixels
  }

  fn run(self, mut app: impl AppHandler + 'static) -> Result<(), std::convert::Infallible> {
    let mut cx = Context::default();
    let mut buffer = pixels();
    app.started(&mut cx, Waker::new(|| {}));
    app.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
    app.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
    for cause in ["first", "second"] {
      *self.fail.borrow_mut() = Some(RenderError::Backend(cause.into()));
      app.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
    }
    Ok(())
  }
}

// Of two failed frames, `run` returns the first, the one that ended the loop.
#[test]
fn first_render_error_is_returned() {
  let renderer = Counter::default();
  let fail = Rc::clone(&renderer.fail);

  let result = run(FailsTwice { fail }, one_window(renderer));

  let Err(Error::Render(RenderError::Backend(cause))) = result else {
    panic!("{result:?}")
  };
  assert_eq!(cause.to_string(), "first");
}

/// A host that makes every call of the seam once, recording what the app asked of it.
struct Scripted {
  cx: Rc<RefCell<Context>>,
  height: Property<f32>,
}

impl Host for Scripted {
  type Error = std::convert::Infallible;

  fn targets(&self) -> TargetKind {
    TargetKind::Pixels
  }

  fn run(self, mut app: impl AppHandler + 'static) -> Result<(), std::convert::Infallible> {
    let mut guard = self.cx.borrow_mut();
    let cx = &mut *guard;
    let mut buffer = pixels();
    app.started(cx, Waker::new(|| {}));
    app.surface_changed(cx, FIRST, info(64, 64, 1.0));
    app.frame(cx, FIRST, target(&mut buffer), Duration::ZERO);
    self.height.set(40.0);
    app.woken(cx);
    app.event(cx, HostEvent::CloseRequested { surface: FIRST });
    app.surface_lost(cx, FIRST, SurfaceLoss::Gone);
    Ok(())
  }
}

// `run`'s handler passes each host call to the runner: the window opens, frames are asked for and
// drawn, a woken write and the close's IsOpen write each ask again, a close closes and exits, a loss
// releases the renderer.
#[test]
fn run_passes_every_host_call_to_the_runner() {
  let mut tree = UiTree::new();
  let content = leaf(&mut tree, 20.0);
  let (application, _) = application(&mut tree, &[("main", content.id)]);
  let renderer = Counter::default();
  let runner = Runner::new(tree, renderer.clone(), Box::new(engine()), application);
  let cx = Rc::new(RefCell::new(Context::default()));

  let result = run(
    Scripted {
      cx: Rc::clone(&cx),
      height: content.height,
    },
    runner,
  );

  assert!(result.is_ok(), "{result:?}");
  let cx = cx.borrow();
  assert_eq!(cx.opened.len(), 1);
  assert_eq!(cx.frames, [FIRST, FIRST, FIRST]);
  assert_eq!(renderer.rendered.get(), 1);
  assert_eq!(cx.closed, [FIRST]);
  assert!(cx.exited);
  assert_eq!(*renderer.lost.borrow(), [FIRST]);
}

/// A host whose loop can't block, like a browser's: it keeps the app and returns.
struct ReturnsAtOnce {
  app: Rc<RefCell<Option<Box<dyn AppHandler>>>>,
}

impl Host for ReturnsAtOnce {
  type Error = std::convert::Infallible;

  fn targets(&self) -> TargetKind {
    TargetKind::Pixels
  }

  fn run(self, app: impl AppHandler + 'static) -> Result<(), std::convert::Infallible> {
    *self.app.borrow_mut() = Some(Box::new(app));
    Ok(())
  }
}

// `run` on a host that returns at once returns `Ok`, and a frame that fails later reaches
// `on_error`.
#[test]
fn host_that_returns_at_once_runs_ok() {
  let renderer = Counter::default();
  *renderer.fail.borrow_mut() = Some(RenderError::Backend("no device".into()));
  let seen = Rc::new(Cell::new(0));
  let hook_seen = Rc::clone(&seen);
  let mut runner = one_window(renderer);
  runner.hooks().on_error(move |_| hook_seen.set(hook_seen.get() + 1));
  let kept = Rc::new(RefCell::new(None));

  let result = run(ReturnsAtOnce { app: Rc::clone(&kept) }, runner);

  assert!(result.is_ok(), "{result:?}");
  let mut app = kept.borrow_mut().take().expect("the host kept the app");
  let mut cx = Context::default();
  let mut buffer = pixels();
  app.started(&mut cx, Waker::new(|| {}));
  app.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
  app.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
  assert_eq!(seen.get(), 1);
  assert!(cx.exited);
}

// A window the host lost before ever reporting it, such as one that failed to open, is forgotten,
// and with no window left the app exits rather than running with none.
#[test]
fn window_that_failed_to_open_exits() {
  let mut tree = UiTree::new();
  let (root, _) = column(&mut tree, &[]);
  let (application, _) = application(&mut tree, &[("main", root)]);
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let mut cx = Context::default();
  runner.started(&mut cx, Waker::new(|| {}));

  runner.surface_lost(&mut cx, FIRST, SurfaceLoss::Gone);

  assert!(cx.exited);
}

// With a window that failed to open, closing the other one, the last open window, exits.
#[test]
fn closing_the_last_open_window_exits_past_a_failed_one() {
  let mut tree = UiTree::new();
  let (first_root, _) = column(&mut tree, &[]);
  let (second_root, _) = column(&mut tree, &[]);
  let (application, _) = application(&mut tree, &[("first", first_root), ("second", second_root)]);
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let mut cx = Context::default();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
  runner.surface_lost(&mut cx, SurfaceId::new(2), SurfaceLoss::Gone);
  assert!(!cx.exited);

  runner.event(&mut cx, HostEvent::CloseRequested { surface: FIRST });

  assert!(cx.exited);
}

// A window lost while suspended keeps its place: it comes back on resume, so the app doesn't exit.
#[test]
fn suspended_window_is_kept() {
  let mut fixture = Fixture::new(|_| {});

  fixture.runner.surface_lost(&mut fixture.cx, FIRST, SurfaceLoss::Suspended);
  fixture.runner.surface_changed(&mut fixture.cx, FIRST, info(64, 64, 1.0));
  let stats = fixture.frame();

  assert!(!fixture.cx.exited);
  assert_eq!(fixture.cx.frames, [FIRST]);
  assert!(stats.rendered);
}

// A window the OS destroyed outside a suspend is gone for good, so with none left the app exits.
#[test]
fn destroyed_window_exits() {
  let mut fixture = Fixture::new(|_| {});

  fixture.runner.surface_lost(&mut fixture.cx, FIRST, SurfaceLoss::Gone);

  assert!(fixture.cx.exited);
  let rendered = fixture.renderer.rendered.get();
  let mut buffer = pixels();
  fixture
    .runner
    .frame(&mut fixture.cx, FIRST, target(&mut buffer), Duration::ZERO);
  assert_eq!(fixture.renderer.rendered.get(), rendered);
}

// A failed frame's message says a frame failed, and the renderer's error is its source, printed
// once by a chain reporter.
#[test]
fn render_error_shows_its_cause_once() {
  use std::error::Error as _;
  let error = Error::Render(RenderError::Backend("no device".into()));

  assert_eq!(error.to_string(), "a frame failed to render");
  assert_eq!(error.source().map(ToString::to_string), Some("no device".to_owned()));
}

/// A runner over `tree`'s `application`, started on a host with windows.
fn started(tree: UiTree, application: NodeId) -> (Runner<Counter>, Context) {
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let mut cx = Context::default();
  runner.started(&mut cx, Waker::new(|| {}));
  (runner, cx)
}

/// A tree with an Application holding one Surface per title, each showing a leaf 20 tall.
fn titled(titles: &[&str]) -> (UiTree, NodeId, Vec<NodeId>) {
  let mut tree = UiTree::new();
  let contents: Vec<(&str, NodeId)> = titles.iter().map(|title| (*title, leaf(&mut tree, 20.0).id)).collect();
  let (application, surfaces) = application(&mut tree, &contents);
  (tree, application, surfaces)
}

// Settings on the Application reach every Surface through inheritance, and a Surface's own value
// wins: the window opens at the Desktop size with the inherited chrome, resizable on both axes.
#[test]
fn surface_opens_at_desktop_size() {
  let (mut tree, application, surfaces) = titled(&["main"]);
  Desktop::set_width(&mut tree, application, Some(300.0));
  Desktop::set_chrome(&mut tree, application, Chrome::None);
  Desktop::set_height(&mut tree, surfaces[0], Some(200.0));

  let (_runner, cx) = started(tree, application);

  assert_eq!(
    cx.opened,
    [WindowDesc {
      title: "main".into(),
      size: LogicalSize {
        width: Some(300.0),
        height: Some(200.0)
      },
      chrome: Chrome::None,
      resizable: Axes::BOTH,
    }]
  );
}

// A Desktop value the Application changes while its surfaces are open updates each of them.
#[test]
fn application_setting_updates_open_surfaces() {
  let (mut tree, application, _) = titled(&["first", "second"]);
  let chrome = tree.attached_property(application, Desktop::CHROME);
  let (mut runner, mut cx) = started(tree, application);

  chrome.set(Chrome::None);
  runner.woken(&mut cx);

  let updated: Vec<(SurfaceId, Chrome)> = cx.updated.iter().map(|(surface, desc)| (*surface, desc.chrome)).collect();
  assert_eq!(updated, [(FIRST, Chrome::None), (SurfaceId::new(2), Chrome::None)]);
}

// A new title updates the window once; settling again sends nothing.
#[test]
fn title_change_updates_once() {
  let mut fixture = Fixture::new(|_| {});

  fixture.title.set("renamed".to_owned());
  fixture.settle();
  fixture.settle();

  let titles: Vec<(SurfaceId, &str)> = fixture
    .cx
    .updated
    .iter()
    .map(|(surface, desc)| (*surface, desc.title.as_str()))
    .collect();
  assert_eq!(titles, [(FIRST, "renamed")]);
}

// A surface marked for reconciling whose window description didn't change isn't updated.
#[test]
fn unchanged_surface_is_not_updated() {
  let mut fixture = Fixture::new(|_| {});

  fixture.title.set("renamed".to_owned());
  fixture.title.set("main".to_owned());
  fixture.settle();

  assert!(fixture.cx.updated.is_empty());
}

// The user closing a window writes IsOpen = false and closes it; setting IsOpen again opens a new
// window for the same surface.
#[test]
fn os_close_writes_is_open_and_reopens() {
  let (tree, application, surfaces) = titled(&["first", "second"]);
  let is_open = surface(&tree, surfaces[0]).is_open;
  let (mut runner, mut cx) = started(tree, application);

  runner.event(&mut cx, HostEvent::CloseRequested { surface: FIRST });
  assert!(!is_open.get());
  assert_eq!(cx.closed, [FIRST]);
  assert!(!cx.exited);

  is_open.set(true);
  runner.woken(&mut cx);

  let titles: Vec<&str> = cx.opened.iter().map(|desc| desc.title.as_str()).collect();
  assert_eq!(titles, ["first", "second", "first"]);
}

// A window that couldn't be created isn't opened again on the next settle, only after IsOpen goes
// false and then true.
#[test]
fn failed_open_is_not_retried() {
  let (tree, application, surfaces) = titled(&["main"]);
  let is_open = surface(&tree, surfaces[0]).is_open;
  let (mut runner, mut cx) = started(tree, application);

  runner.surface_lost(&mut cx, FIRST, SurfaceLoss::Gone);
  runner.woken(&mut cx);
  assert_eq!(cx.opened.len(), 1);
  assert!(cx.exited);

  is_open.set(false);
  runner.woken(&mut cx);
  is_open.set(true);
  runner.woken(&mut cx);
  assert_eq!(cx.opened.len(), 2);
}

// A surface taken out of the Application closes its window; the other stays.
#[test]
fn removed_surface_closes() {
  let (tree, application, surfaces) = titled(&["first", "second"]);
  let second = surfaces[1];
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  runner.hooks().before_layout(move |tree, _, _, _| {
    if tree.parent(TreeKind::Logical, second).is_some() {
      tree.detach(TreeKind::Logical, second);
      tree.detach(TreeKind::Visual, second);
    }
  });
  let mut cx = Context::default();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
  let mut buffer = pixels();

  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  assert_eq!(cx.closed, [SurfaceId::new(2)]);
  assert!(!cx.exited);
}

// An Application with no Surface has nothing to show, so the app exits.
#[test]
fn empty_application_exits() {
  let (tree, application, _) = titled(&[]);

  let (_runner, cx) = started(tree, application);

  assert!(cx.exited);
}

// Only Surfaces may be an Application's children.
#[test]
#[should_panic(expected = "isn't a Surface")]
fn application_child_must_be_a_surface() {
  let (mut tree, application, _) = titled(&[]);
  let stray = leaf(&mut tree, 20.0);
  tree.add_child(TreeKind::Logical, application, stray.id);

  let _ = started(tree, application);
}

// A host without windows binds open surfaces to the surfaces it reports, in tree order.
#[test]
fn windowless_host_binds_in_tree_order() {
  let (tree, application, surfaces) = titled(&["first", "second"]);
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let drawn = Rc::new(Cell::new(None));
  let hook_drawn = Rc::clone(&drawn);
  runner.hooks().after_present(move |_, root, _| hook_drawn.set(Some(root)));
  let mut cx = Context {
    windowless: true,
    ..Context::default()
  };
  let mut buffer = pixels();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, SurfaceId::new(9), info(64, 64, 1.0));

  runner.frame(&mut cx, SurfaceId::new(9), target(&mut buffer), Duration::ZERO);

  assert!(cx.opened.is_empty());
  assert!(!cx.exited);
  assert_eq!(drawn.get(), Some(surfaces[0]));
}

// A host surface reported before any Surface takes it asks for a frame as soon as one does.
#[test]
fn lent_surface_reported_first_is_drawn_once_bound() {
  let (tree, application, _) = titled(&["first"]);
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let mut cx = Context {
    windowless: true,
    ..Context::default()
  };

  runner.surface_changed(&mut cx, SurfaceId::new(9), info(64, 64, 1.0));
  runner.started(&mut cx, Waker::new(|| {}));

  assert_eq!(cx.frames, [SurfaceId::new(9)]);
}

// On a host without windows, a closed surface gives its host surface to the next open one.
#[test]
fn closed_surface_frees_its_host_surface() {
  let (tree, application, surfaces) = titled(&["first", "second"]);
  let first_open = surface(&tree, surfaces[0]).is_open;
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let drawn = Rc::new(Cell::new(None));
  let hook_drawn = Rc::clone(&drawn);
  runner.hooks().after_present(move |_, root, _| hook_drawn.set(Some(root)));
  let mut cx = Context {
    windowless: true,
    ..Context::default()
  };
  let mut buffer = pixels();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, SurfaceId::new(9), info(64, 64, 1.0));

  first_open.set(false);
  runner.woken(&mut cx);
  runner.frame(&mut cx, SurfaceId::new(9), target(&mut buffer), Duration::ZERO);

  assert_eq!(drawn.get(), Some(surfaces[1]));
  assert!(cx.closed.is_empty());
}

// A window whose content sizes it opens at the content's measured size, and the user can't resize
// those axes.
#[test]
fn surface_opens_measured_to_content() {
  let mut tree = UiTree::new();
  let top = leaf(&mut tree, 20.0);
  let bottom = leaf(&mut tree, 30.0);
  let (content, _) = column(&mut tree, &[top.id, bottom.id]);
  let (application, surfaces) = application(&mut tree, &[("main", content)]);
  Desktop::set_size_to_content(&mut tree, surfaces[0], SizeToContent::WidthAndHeight);

  let (_runner, cx) = started(tree, application);

  assert_eq!(
    cx.opened[0].size,
    LogicalSize {
      width: Some(10.0),
      height: Some(50.0)
    }
  );
  assert_eq!(cx.opened[0].resizable, Axes::NONE);
}

// When the OS gives a content-sized window another size than requested, such as clamping it to
// the screen, the runner doesn't ask again.
#[test]
fn size_to_content_settles() {
  let (mut tree, application, surfaces) = titled(&["main"]);
  Desktop::set_size_to_content(&mut tree, surfaces[0], SizeToContent::Height);
  let (mut runner, mut cx) = started(tree, application);
  let mut buffer = pixels();

  runner.surface_changed(&mut cx, FIRST, info(64, 10, 1.0));
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  assert_eq!(
    cx.opened[0].size,
    LogicalSize {
      width: None,
      height: Some(20.0)
    }
  );
  assert!(cx.updated.is_empty());
}

// Content that grows resizes a window sized to it, once.
#[test]
fn content_growth_resizes_window() {
  let mut tree = UiTree::new();
  let content = leaf(&mut tree, 20.0);
  let (application, surfaces) = application(&mut tree, &[("main", content.id)]);
  Desktop::set_size_to_content(&mut tree, surfaces[0], SizeToContent::Height);
  let (mut runner, mut cx) = started(tree, application);
  let mut buffer = pixels();
  runner.surface_changed(&mut cx, FIRST, info(64, 20, 1.0));
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  content.height.set(40.0);
  runner.woken(&mut cx);
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  let sizes: Vec<LogicalSize> = cx.updated.iter().map(|(_, desc)| desc.size).collect();
  assert_eq!(
    sizes,
    [LogicalSize {
      width: None,
      height: Some(40.0)
    }]
  );
}

// Content that shrinks resizes a window sized to it too, once.
#[test]
fn shrinking_content_resizes_window() {
  let mut tree = UiTree::new();
  let content = leaf(&mut tree, 20.0);
  let (application, surfaces) = application(&mut tree, &[("main", content.id)]);
  Desktop::set_size_to_content(&mut tree, surfaces[0], SizeToContent::Height);
  let (mut runner, mut cx) = started(tree, application);
  let mut buffer = pixels();
  runner.surface_changed(&mut cx, FIRST, info(64, 20, 1.0));
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  content.height.set(10.0);
  runner.woken(&mut cx);
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  let sizes: Vec<LogicalSize> = cx.updated.iter().map(|(_, desc)| desc.size).collect();
  assert_eq!(
    sizes,
    [LogicalSize {
      width: None,
      height: Some(10.0)
    }]
  );
}

// A surface on a host without windows fills its host surface: SizeToContent doesn't apply.
#[test]
fn windowless_surface_ignores_size_to_content() {
  let mut tree = UiTree::new();
  let content = leaf(&mut tree, 20.0);
  let (application, surfaces) = application(&mut tree, &[("main", content.id)]);
  Desktop::set_size_to_content(&mut tree, surfaces[0], SizeToContent::WidthAndHeight);
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let seen = Rc::new(Cell::new(None));
  let hook_seen = Rc::clone(&seen);
  let leaf_id = content.id;
  runner
    .hooks()
    .after_present(move |tree, _, _| hook_seen.set(tree.get_as::<Leaf>(leaf_id).and_then(|leaf| leaf.seen.get())));
  let mut cx = Context {
    windowless: true,
    ..Context::default()
  };
  let mut buffer = pixels();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, SurfaceId::new(9), info(64, 64, 1.0));

  runner.frame(&mut cx, SurfaceId::new(9), target(&mut buffer), Duration::ZERO);

  assert_eq!(seen.get(), Some(Size::new(64.0, 64.0)));
  assert!(cx.updated.is_empty());
}

// A new width for a window whose content sizes its height measures the content at that width
// before updating the window, so the height sent with it fits.
#[test]
fn width_change_measures_content_at_the_new_width() {
  let mut tree = UiTree::new();
  let content = tree.alloc(Box::new(Wrap));
  let (application, surfaces) = application(&mut tree, &[("main", content)]);
  Desktop::set_size_to_content(&mut tree, surfaces[0], SizeToContent::Height);
  let width = tree.attached_property(surfaces[0], Desktop::WIDTH);
  width.set(Some(50.0));
  let (mut runner, mut cx) = started(tree, application);

  width.set(Some(100.0));
  runner.woken(&mut cx);

  assert_eq!(
    cx.opened[0].size,
    LogicalSize {
      width: Some(50.0),
      height: Some(20.0)
    }
  );
  let sizes: Vec<LogicalSize> = cx.updated.iter().map(|(_, desc)| desc.size).collect();
  assert_eq!(
    sizes,
    [LogicalSize {
      width: Some(100.0),
      height: Some(10.0)
    }]
  );
}

// A surface must be a visual child of the Application too, so no surface draws inside another;
// a hook that nests another surface under the one being drawn is caught before anything renders.
#[test]
fn surface_nested_in_another_panics_before_rendering() {
  let (tree, application, surfaces) = titled(&["first", "second"]);
  let (first, second) = (surfaces[0], surfaces[1]);
  let renderer = Counter::default();
  let mut runner = Runner::new(tree, renderer.clone(), Box::new(engine()), application);
  let mut cx = Context::default();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
  runner.hooks().before_layout(move |tree, _, _, _| {
    if tree.parent(TreeKind::Visual, second) != Some(first) {
      tree.detach(TreeKind::Visual, second);
      tree.add_child(TreeKind::Visual, first, second);
    }
  });
  let mut buffer = pixels();

  let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
  }));

  let message = result.expect_err("nesting a surface panics");
  let message = message.downcast_ref::<String>().map_or("", String::as_str);
  assert!(message.contains("isn't a visual child of the Application"), "{message}");
  assert_eq!(renderer.rendered.get(), 0);
}

// A hook that takes the surface being drawn out of the Application closes its window instead of
// drawing it once more.
#[test]
fn surface_removed_by_a_hook_isnt_drawn() {
  let (tree, application, surfaces) = titled(&["first", "second"]);
  let first = surfaces[0];
  let renderer = Counter::default();
  let mut runner = Runner::new(tree, renderer.clone(), Box::new(engine()), application);
  let mut cx = Context::default();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
  runner.hooks().before_layout(move |tree, _, _, _| {
    if tree.parent(TreeKind::Logical, first).is_some() {
      tree.detach(TreeKind::Logical, first);
    }
  });
  let mut buffer = pixels();

  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  assert_eq!(renderer.rendered.get(), 0);
  assert_eq!(cx.closed, [FIRST]);
}

// A hook that gives the Application a child that isn't a Surface is caught before anything
// renders.
#[test]
fn application_child_added_by_a_hook_panics_before_rendering() {
  let (mut tree, application, _) = titled(&["main"]);
  let stray = leaf(&mut tree, 20.0).id;
  let renderer = Counter::default();
  let mut runner = Runner::new(tree, renderer.clone(), Box::new(engine()), application);
  let mut cx = Context::default();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
  runner.hooks().before_layout(move |tree, _, _, _| {
    if tree.parent(TreeKind::Logical, stray).is_none() {
      tree.add_child(TreeKind::Logical, application, stray);
    }
  });
  let mut buffer = pixels();

  let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
  }));

  let message = result.expect_err("a stray child panics");
  let message = message.downcast_ref::<String>().map_or("", String::as_str);
  assert!(message.contains("isn't a Surface"), "{message}");
  assert_eq!(renderer.rendered.get(), 0);
}

// A surface taken out of the Application but left inside another surface's visual subtree is
// caught before the outer surface draws it.
#[test]
fn surface_detached_into_another_panics_before_rendering() {
  let (tree, application, surfaces) = titled(&["first", "second"]);
  let (first, second) = (surfaces[0], surfaces[1]);
  let renderer = Counter::default();
  let mut runner = Runner::new(tree, renderer.clone(), Box::new(engine()), application);
  let mut cx = Context::default();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
  runner.hooks().before_layout(move |tree, _, _, _| {
    if tree.parent(TreeKind::Logical, second).is_some() {
      tree.detach(TreeKind::Logical, second);
      tree.detach(TreeKind::Visual, second);
      tree.add_child(TreeKind::Visual, first, second);
    }
  });
  let mut buffer = pixels();

  let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);
  }));

  let message = result.expect_err("a surface inside another panics");
  let message = message.downcast_ref::<String>().map_or("", String::as_str);
  assert!(message.contains("is inside the Surface"), "{message}");
  assert_eq!(renderer.rendered.get(), 0);
}

// A surface built inside another surface's content is caught at the first settle, before any
// window opens.
#[test]
fn surface_nested_from_the_start_panics_at_started() {
  let (mut tree, application, surfaces) = titled(&["first", "second"]);
  let (first, second) = (surfaces[0], surfaces[1]);
  tree.detach(TreeKind::Logical, second);
  tree.detach(TreeKind::Visual, second);
  tree.add_child(TreeKind::Visual, first, second);
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let mut cx = Context::default();

  let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    runner.started(&mut cx, Waker::new(|| {}));
  }));

  let message = result.expect_err("nesting a surface panics");
  let message = message.downcast_ref::<String>().map_or("", String::as_str);
  assert!(message.contains("is inside the Surface"), "{message}");
  assert!(cx.opened.is_empty());
}

// A surface nested inside a closed surface is caught too: the rule is about the tree, not about
// what is drawn.
#[test]
fn surface_nested_in_a_closed_surface_panics_at_started() {
  let (mut tree, application, surfaces) = titled(&["first", "second", "third"]);
  let (first, third) = (surfaces[0], surfaces[2]);
  surface(&tree, first).is_open.set(false);
  tree.detach(TreeKind::Logical, third);
  tree.detach(TreeKind::Visual, third);
  tree.add_child(TreeKind::Visual, first, third);
  let mut runner = Runner::new(tree, Counter::default(), Box::new(engine()), application);
  let mut cx = Context::default();

  let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    runner.started(&mut cx, Waker::new(|| {}));
  }));

  let message = result.expect_err("nesting a surface panics");
  let message = message.downcast_ref::<String>().map_or("", String::as_str);
  assert!(message.contains("is inside the Surface"), "{message}");
}

// A hook that frees a bound surface closes its window at the frame's settle, and the other surface
// is drawn.
#[test]
fn surface_freed_by_a_hook_closes_its_window() {
  let (tree, application, surfaces) = titled(&["first", "second"]);
  let second = surfaces[1];
  let renderer = Counter::default();
  let mut runner = Runner::new(tree, renderer.clone(), Box::new(engine()), application);
  let mut cx = Context::default();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
  runner.hooks().before_layout(move |tree, _, _, _| {
    if tree.contains(second) {
      tree.free(second);
    }
  });
  let mut buffer = pixels();

  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  assert_eq!(cx.closed, [SurfaceId::new(2)]);
  assert_eq!(renderer.rendered.get(), 1);
}

// A hook that closes the surface being drawn closes its window instead of drawing it once more.
#[test]
fn surface_closed_by_a_hook_isnt_drawn() {
  let (tree, application, surfaces) = titled(&["first", "second"]);
  let is_open = surface(&tree, surfaces[0]).is_open;
  let renderer = Counter::default();
  let mut runner = Runner::new(tree, renderer.clone(), Box::new(engine()), application);
  let mut cx = Context::default();
  runner.started(&mut cx, Waker::new(|| {}));
  runner.surface_changed(&mut cx, FIRST, info(64, 64, 1.0));
  runner.hooks().before_layout(move |_, _, _, _| is_open.set(false));
  let mut buffer = pixels();

  runner.frame(&mut cx, FIRST, target(&mut buffer), Duration::ZERO);

  assert_eq!(renderer.rendered.get(), 0);
  assert_eq!(cx.closed, [FIRST]);
}

// Every surface's layout shapes its text with the one engine the runner was given.
#[test]
fn one_engine_serves_every_surface() {
  let mut tree = UiTree::new();
  let (first, _) = label(&mut tree, "first");
  let (second, _) = label(&mut tree, "second");
  let (application, _) = application(&mut tree, &[("one", first), ("two", second)]);
  let text = Recorder::default();
  let mut runner = Runner::new(tree, Counter::default(), Box::new(text.clone()), application);
  let mut cx = Context::default();

  runner.started(&mut cx, Waker::new(|| {}));
  for id in [1, 2] {
    let surface = SurfaceId::new(id);
    runner.surface_changed(&mut cx, surface, info(64, 64, 1.0));
    runner.frame(&mut cx, surface, target(&mut pixels()), Duration::ZERO);
  }

  let mut asked = text.asked.borrow().clone();
  asked.sort();
  asked.dedup();
  assert_eq!(asked, ["first", "second"]);
}

// A window its content sizes is opened at the size of the text, measured through the engine.
#[test]
fn content_sized_window_measures_text() {
  let mut tree = UiTree::new();
  let (content, _) = label(&mut tree, "twelve chars");
  let (application, surfaces) = application(&mut tree, &[("main", content)]);
  Desktop::set_size_to_content(&mut tree, surfaces[0], SizeToContent::WidthAndHeight);
  let mut runner = Runner::new(tree, Counter::default(), Box::new(Recorder::default()), application);
  let mut cx = Context::default();

  runner.started(&mut cx, Waker::new(|| {}));

  assert_eq!(
    cx.opened[0].size,
    LogicalSize {
      width: Some(96.0),
      height: Some(16.0)
    }
  );
}
