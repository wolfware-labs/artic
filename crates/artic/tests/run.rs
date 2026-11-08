// `artic::run` from a launcher's side: a scripted host and a pixel renderer, as adapter crates
// write them against `artic_platform` and `artic::render`, run an app built with `artic::App`.

use artic::element::{Application, Surface};
use artic::render::{
  PhysicalSize, PixelBuffer, PixelFormat, RenderError, Renderer, Scene, SurfaceId, SurfaceView, Target, TargetKind,
};
use artic::{App, Error};
use artic_platform::{AppHandler, DisplayId, Host, HostContext, SurfaceInfo, SurfaceLoss, Waker, WindowDesc, Windows};
use std::cell::{Cell, RefCell};
use std::io;
use std::rc::Rc;
use std::time::Duration;

/// What the host saw: whether its loop ran, and the titles of the windows it opened.
#[derive(Debug, Default)]
struct Log {
  ran: bool,
  opened: Vec<String>,
}

/// A host lending pixel buffers, whose loop opens what the app asks for, draws one frame on its
/// first surface and ends with `end`.
struct Scripted {
  log: Rc<RefCell<Log>>,
  end: Result<(), io::Error>,
}

/// The scripted host's context: it opens windows numbered from 1 and logs their titles.
struct Context<'a> {
  log: &'a RefCell<Log>,
}

impl HostContext for Context<'_> {
  fn request_frame(&mut self, _surface: SurfaceId) {}

  fn exit(&mut self) {}

  fn windows(&mut self) -> Option<&mut dyn Windows> {
    Some(self)
  }
}

impl Windows for Context<'_> {
  fn open(&mut self, desc: &WindowDesc) -> SurfaceId {
    let mut log = self.log.borrow_mut();
    log.opened.push(desc.title.clone());
    SurfaceId::new(log.opened.len() as u64)
  }

  fn update(&mut self, _surface: SurfaceId, _desc: &WindowDesc) {}

  fn close(&mut self, _surface: SurfaceId) {}
}

impl Host for Scripted {
  type Error = io::Error;

  fn targets(&self) -> TargetKind {
    TargetKind::Pixels
  }

  fn run(self, mut app: impl AppHandler + 'static) -> Result<(), io::Error> {
    self.log.borrow_mut().ran = true;
    let mut cx = Context { log: &self.log };
    let surface = SurfaceId::new(1);
    let info = SurfaceInfo {
      size: PhysicalSize { width: 2, height: 1 },
      scale: 1.0,
      display: DisplayId::new(1),
      refresh_rate: Some(60.0),
    };
    let mut pixels = vec![0; 8];

    app.started(&mut cx, Waker::new(|| {}));
    app.surface_changed(&mut cx, surface, info);
    let target = Target::Pixels(PixelBuffer {
      data: &mut pixels,
      stride: 8,
      format: PixelFormat::Rgba8,
    });
    app.frame(&mut cx, surface, target, Duration::from_millis(16));
    app.surface_lost(&mut cx, surface, SurfaceLoss::Gone);
    self.end
  }
}

/// A renderer that draws on `draws_on`, or on nothing when it's `None`, and counts the frames it
/// drew.
#[derive(Debug)]
struct Pixels {
  draws_on: Option<TargetKind>,
  rendered: Rc<Cell<u32>>,
}

impl Renderer for Pixels {
  fn accepts(&self, kind: TargetKind) -> bool {
    self.draws_on == Some(kind)
  }

  fn render(&mut self, _scene: &Scene, _surface: SurfaceView<'_>) -> Result<(), RenderError> {
    self.rendered.set(self.rendered.get() + 1);
    Ok(())
  }

  fn surface_lost(&mut self, _surface: SurfaceId) {}
}

/// An app with one surface, titled "Hello".
fn hello() -> App {
  App::new(|tree| {
    let surface = Surface::new().title("Hello").build(tree);
    Application::new().children([surface]).build(tree)
  })
}

/// Runs `hello` on a scripted host whose loop ends with `end`, with a renderer that draws on
/// `draws_on`; returns the result, the host's log and the frames drawn.
fn run_hello(end: Result<(), io::Error>, draws_on: Option<TargetKind>) -> (Result<(), Error>, Log, u32) {
  let log = Rc::new(RefCell::new(Log::default()));
  let rendered = Rc::new(Cell::new(0));
  let host = Scripted {
    log: Rc::clone(&log),
    end,
  };
  let renderer = Pixels {
    draws_on,
    rendered: Rc::clone(&rendered),
  };
  let result = artic::run(host, renderer, hello());
  (result, log.take(), rendered.get())
}

// The app's surface opens as a window and draws a frame, and the run ends without an error.
#[test]
fn run_returns_ok() {
  let (result, log, rendered) = run_hello(Ok(()), Some(TargetKind::Pixels));

  assert!(result.is_ok(), "{result:?}");
  assert_eq!(log.opened, ["Hello"]);
  assert_eq!(rendered, 1);
}

// A renderer that can't draw on the host's pixel buffers ends the run before the host starts.
#[test]
fn run_returns_unsupported() {
  let (result, log, _) = run_hello(Ok(()), None);

  assert!(matches!(result, Err(Error::Unsupported(TargetKind::Pixels))), "{result:?}");
  assert!(!log.ran);
}

// The host's own failure comes back as the run's error, with the host's message.
#[test]
fn run_returns_the_host_error() {
  let (result, log, _) = run_hello(Err(io::Error::other("the display went away")), Some(TargetKind::Pixels));

  let Err(Error::Host(error)) = result else {
    panic!("expected the host's error, got {result:?}");
  };
  assert_eq!(error.to_string(), "the display went away");
  assert!(log.ran);
}
