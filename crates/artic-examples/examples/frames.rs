// Draws one surface of an `Application` through the frame pipeline, changes a color once after the
// first frame, and checks that the loop then goes idle: it exits with code 0 once 500 ms pass with no
// frame after at least two, and with code 1 if frames still come 10 s after the first. Waiting for
// the quiet rather than judging at a fixed time keeps a slow presenter, such as lavapipe under a
// headless compositor, from passing for a loop that never stops. The check and the exit run on the
// loop's thread, woken every 100 ms by a timer through the host's `Waker`. On WARP, Windows' software
// adapter (all GitHub's Windows runners have), vello can't run and the renderer refuses to start,
// so there it draws with a renderer that draws nothing: the check is about when frames run, not
// what they show.
//
// Run with `cargo run -p artic-examples --example frames`.

use artic_core::Object;
use artic_core::error::Chain;
use artic_core::property::{Affects, Inheritance, Property, PropertyKey, PropertyVisitor};
use artic_core::tree::{TreeKind, UiTree};
use artic_element::{AccessCx, Application, Desktop, Element, RenderCx, Surface};
use artic_examples::Report;
use artic_layout::geometry::{Rect, Size};
use artic_layout::{ArrangeCx, Layoutable, MeasureCx};
use artic_platform::{AppHandler, Host as _, HostContext, HostEvent, Runner, SurfaceInfo, SurfaceLoss, Waker};
use artic_render::{Color, RenderError, Renderer, Scene, SceneBuilder, SurfaceId, SurfaceView, Target, TargetKind};
use std::cell::Cell;
use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};

static RED: PropertyKey<u8> = PropertyKey::new("Red", Affects::RENDER, Inheritance::None);

/// Fills its whole box with a shade of red.
#[derive(Debug)]
struct Swatch {
  red: Property<u8>,
}

impl Object for Swatch {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.red);
  }
}

impl Layoutable for Swatch {
  fn measure(&self, _cx: &mut MeasureCx<'_>, available: Size) -> Size {
    available
  }

  fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
}

impl Element for Swatch {
  fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder) {
    let size = cx.size();
    scene.fill(Rect::from_size(size), Color::rgb(self.red.get(), 0, 0));
  }

  fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::GenericContainer)
  }
}

artic_element::element_type!(Swatch);

/// How long the loop must stay without a frame to count as idle.
const QUIET: Duration = Duration::from_millis(500);
/// How long after the first frame the loop may take to go idle.
const DEADLINE: Duration = Duration::from_secs(10);
/// How often the timer wakes the loop to check.
const TICK: Duration = Duration::from_millis(100);

/// Runs `runner` and judges whether the loop goes quiet after its frames.
struct Idle<R> {
  runner: Runner<R>,
  waker: Option<Waker>,
  frames: Vec<Instant>,
  quiet: Rc<Cell<Option<bool>>>,
}

impl<R: Renderer> AppHandler for Idle<R> {
  fn started(&mut self, cx: &mut dyn HostContext, waker: Waker) {
    self.waker = Some(waker.clone());
    self.runner.started(cx, waker);
  }

  fn woken(&mut self, cx: &mut dyn HostContext) {
    self.runner.woken(cx);
    let (Some(&first), Some(&last)) = (self.frames.first(), self.frames.last()) else {
      return;
    };
    if self.quiet.get().is_some() {
      return;
    }
    let quiet = self.frames.len() >= 2 && last.elapsed() >= QUIET;
    if quiet || first.elapsed() >= DEADLINE {
      println!(
        "{} frames over {} ms, the last {} ms ago",
        self.frames.len(),
        last.duration_since(first).as_millis(),
        last.elapsed().as_millis()
      );
      self.quiet.set(Some(quiet));
      cx.exit();
    }
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
    self.runner.frame(cx, surface, target, time);
    self.frames.push(Instant::now());
    if self.frames.len() == 1
      && let Some(waker) = self.waker.clone()
    {
      std::thread::spawn(move || {
        let start = Instant::now();
        while start.elapsed() <= DEADLINE + TICK {
          std::thread::sleep(TICK);
          waker.wake();
        }
      });
    }
  }
}

/// Accepts window handles and draws nothing.
struct Blank;

impl Renderer for Blank {
  fn accepts(&self, kind: TargetKind) -> bool {
    kind == TargetKind::Handles
  }

  fn render(&mut self, _scene: &Scene, _surface: SurfaceView<'_>) -> Result<(), RenderError> {
    Ok(())
  }
}

fn main() -> Result<(), Report> {
  match artic_vello::Renderer::new() {
    Ok(renderer) => {
      println!("{renderer:?}");
      check(renderer)
    }
    Err(error @ artic_vello::Error::UnsupportedAdapter(_)) => {
      println!("{}: drawing nothing", Chain(&error));
      check(Blank)
    }
    Err(error) => Err(error.into()),
  }
}

/// Runs the app with `renderer` and returns whether it went idle.
fn check(renderer: impl Renderer + 'static) -> Result<(), Report> {
  let mut tree = UiTree::new();
  let red = Property::new(&RED, 128);
  let swatch = tree.alloc(Box::new(Swatch { red }));
  let application = tree.alloc(Box::new(Application::default()));
  let surface = Surface::default();
  surface.title.set("artic frames".to_owned());
  let surface = tree.alloc(Box::new(surface));
  for kind in TreeKind::ALL {
    tree.add_child(kind, application, surface);
    tree.add_child(kind, surface, swatch);
  }
  Desktop::set_width(&mut tree, surface, Some(320.0));
  Desktop::set_height(&mut tree, surface, Some(240.0));
  let mut runner = Runner::new(tree, renderer, Box::new(artic_text::Engine::system()), application);
  let changed = Rc::new(Cell::new(false));
  runner.hooks().after_present(move |_, _, stats| {
    println!("frame: {stats:?}");
    if !changed.replace(true) {
      red.set(255);
    }
  });
  runner.hooks().on_error(|error| eprintln!("frame failed: {}", Chain(error)));

  let quiet = Rc::new(Cell::new(None));
  let idle = Idle {
    runner,
    waker: None,
    frames: Vec::new(),
    quiet: Rc::clone(&quiet),
  };
  artic_winit::Host::new()?.run(idle)?;
  match quiet.get() {
    Some(true) => Ok(()),
    Some(false) => Err(io::Error::other("frames ran after the app should have gone idle").into()),
    None => Err(io::Error::other("the loop ended before the check").into()),
  }
}
