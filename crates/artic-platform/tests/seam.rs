// The host seam, driven from both sides: a fake host that a third-party adapter could have written
// against the traits alone, and a handler like artic's.

use artic_platform::{
  AppHandler, Axes, Chrome, DisplayId, Host, HostContext, HostEvent, LogicalSize, SurfaceInfo, SurfaceLoss, Waker, WindowDesc,
  Windows,
};
use artic_render::{PhysicalSize, PixelBuffer, PixelFormat, SurfaceId, Target, TargetKind};
use pretty_assertions::assert_eq;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// What the fake host's context saw, and the windows it opened.
#[derive(Debug, Default)]
struct Requests {
  frames: Vec<SurfaceId>,
  exited: bool,
  opened: Vec<String>,
}

/// The fake host's context: a window list that opens one window, and a log of requests.
struct FakeContext<'a> {
  requests: &'a mut Requests,
}

impl HostContext for FakeContext<'_> {
  fn request_frame(&mut self, surface: SurfaceId) {
    self.requests.frames.push(surface);
  }

  fn exit(&mut self) {
    self.requests.exited = true;
  }

  fn windows(&mut self) -> Option<&mut dyn Windows> {
    Some(self)
  }
}

impl Windows for FakeContext<'_> {
  fn open(&mut self, desc: &WindowDesc) -> SurfaceId {
    self.requests.opened.push(desc.title.clone());
    SurfaceId::new(7)
  }

  fn update(&mut self, _surface: SurfaceId, _desc: &WindowDesc) {}

  fn close(&mut self, _surface: SurfaceId) {}
}

/// A host whose surface is a pixel buffer, and whose loop runs one scripted frame.
struct FakeHost {
  requests: Rc<RefCell<Requests>>,
}

impl Host for FakeHost {
  type Error = std::convert::Infallible;

  fn targets(&self) -> TargetKind {
    TargetKind::Pixels
  }

  fn run(self, mut app: impl AppHandler + 'static) -> Result<(), std::convert::Infallible> {
    let mut requests = self.requests.borrow_mut();
    let mut cx = FakeContext { requests: &mut requests };
    let surface = SurfaceId::new(7);
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
    app.event(&mut cx, HostEvent::CloseRequested { surface });
    app.surface_lost(&mut cx, surface, SurfaceLoss::Gone);
    Ok(())
  }
}

/// A handler that opens a window, asks for frames while it lives and exits when it's closed,
/// recording each call.
struct Recorder {
  calls: Rc<RefCell<Vec<String>>>,
}

impl AppHandler for Recorder {
  fn started(&mut self, cx: &mut dyn HostContext, _waker: Waker) {
    self.calls.borrow_mut().push("started".into());
    let opened = cx.windows().expect("the fake host has windows").open(&WindowDesc {
      title: "main".into(),
      size: LogicalSize {
        width: Some(2.0),
        height: Some(1.0),
      },
      chrome: Chrome::System,
      resizable: Axes::BOTH,
    });
    self.calls.borrow_mut().push(format!("opened {}", opened.get()));
  }

  fn woken(&mut self, _cx: &mut dyn HostContext) {
    self.calls.borrow_mut().push("woken".into());
  }

  fn surface_changed(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, info: SurfaceInfo) {
    self
      .calls
      .borrow_mut()
      .push(format!("changed {} {}x{}", surface.get(), info.size.width, info.size.height));
    cx.request_frame(surface);
  }

  fn surface_lost(&mut self, _cx: &mut dyn HostContext, surface: SurfaceId, loss: SurfaceLoss) {
    self.calls.borrow_mut().push(format!("lost {} {loss:?}", surface.get()));
  }

  fn event(&mut self, cx: &mut dyn HostContext, event: HostEvent) {
    if let HostEvent::CloseRequested { surface } = event {
      self.calls.borrow_mut().push(format!("close {}", surface.get()));
      cx.exit();
    }
  }

  fn frame(&mut self, _cx: &mut dyn HostContext, surface: SurfaceId, target: Target<'_>, time: Duration) {
    let kind = match target {
      Target::Pixels(_) => "pixels",
      _ => "other",
    };
    self
      .calls
      .borrow_mut()
      .push(format!("frame {} {kind} {}ms", surface.get(), time.as_millis()));
  }
}

// A host written against the seam alone drives a handler through a window's life, and the
// handler's requests reach the host through the context.
#[test]
fn fake_host_drives_a_handler() {
  let requests = Rc::new(RefCell::new(Requests::default()));
  let calls = Rc::new(RefCell::new(Vec::new()));
  let host = FakeHost {
    requests: Rc::clone(&requests),
  };
  assert_eq!(host.targets(), TargetKind::Pixels);

  let Ok(()) = host.run(Recorder {
    calls: Rc::clone(&calls),
  });

  assert_eq!(
    *calls.borrow(),
    [
      "started",
      "opened 7",
      "changed 7 2x1",
      "frame 7 pixels 16ms",
      "close 7",
      "lost 7 Gone"
    ]
  );
  let requests = requests.borrow();
  assert_eq!(
    (requests.opened.as_slice(), requests.frames.as_slice(), requests.exited),
    (["main".to_string()].as_slice(), [SurfaceId::new(7)].as_slice(), true)
  );
}
