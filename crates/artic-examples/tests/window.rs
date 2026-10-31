// Draws into real windows through the artic-winit host and artic-vello's renderer, and reads the
// presented pixels back from the X server. It covers what a pixel buffer can't: the swapchain each
// window gets, its reconfiguring on a resize, and its drop when the host reports the surface lost.
// winit's loop must own the main thread, so this test runs without the harness. It needs X11:
// without an X display it skips, or fails when `ARTIC_REQUIRE_DISPLAY` is `1`.

use artic_core::error::Chain;
use artic_layout::geometry::Rect;
use artic_platform::{
  AppHandler, Axes, Chrome, Host as _, HostContext, HostEvent, LogicalSize, SurfaceInfo, SurfaceLoss, Waker, WindowDesc,
};
use artic_render::{Color, PhysicalSize, RawHandles, Renderer as _, Scene, SurfaceId, SurfaceView, Target};
use artic_test::require;
use raw_window_handle::RawWindowHandle;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};
use x11rb::protocol::xproto::{ConnectionExt as _, ImageFormat};
use x11rb::rust_connection::RustConnection;

const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const GREEN: [u8; 3] = [0, 255, 0];

// A window shows what the renderer draws, at its first size and after a resize, and a surface the
// host reports lost is dropped, so its id can name another window's surface.
fn main() {
  if cfg!(any(target_os = "macos", windows)) || std::env::var_os("DISPLAY").is_none() {
    require::no_display("the window test reads pixels from an X server");
    return;
  }
  // SAFETY: no other thread has started, so nothing reads the environment meanwhile.
  unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
  let renderer = match artic_vello::Renderer::new() {
    Ok(renderer) => renderer,
    Err(error @ (artic_vello::Error::Adapter(_) | artic_vello::Error::UnsupportedAdapter(_))) => {
      require::no_gpu(Chain(&error));
      return;
    }
    Err(error) => panic!("the renderer failed to start: {}", Chain(&error)),
  };
  let done = Rc::new(Cell::new(false));
  let script = Script {
    renderer,
    sizes: HashMap::new(),
    step: Step::Draw,
    done: Rc::clone(&done),
  };
  artic_winit::Host::new()
    .expect("an X11 host")
    .run(script)
    .expect("the loop runs");
  assert!(done.get(), "the loop ended before the last step");
}

/// What the next frame checks.
#[derive(Clone, Copy)]
enum Step {
  /// Window A's first frame: sizes without pixels, then red at its size.
  Draw,
  /// A frame of A after it was resized from the size it holds: blue at the new size.
  Resized(PhysicalSize),
  /// Window B's first frame, after A was drawn: green to B under A's id, once A's surface is lost.
  Reused { lost: SurfaceId },
}

struct Script {
  renderer: artic_vello::Renderer,
  sizes: HashMap<SurfaceId, PhysicalSize>,
  step: Step,
  done: Rc<Cell<bool>>,
}

impl Script {
  /// Fills `size` of the window behind `handles`, drawing as surface `id`, with `color`.
  fn fill(&mut self, id: SurfaceId, size: PhysicalSize, handles: &dyn RawHandles, color: [u8; 3]) {
    let [r, g, b] = color;
    let scene = Scene::record(|builder| builder.fill(Rect::new(0.0, 0.0, 4096.0, 4096.0), Color::rgb(r, g, b)));
    let surface = SurfaceView {
      id,
      size,
      scale: 1.0,
      target: Target::Handles(handles),
    };
    self.renderer.render(&scene, surface).expect("draws into the window");
  }
}

impl AppHandler for Script {
  fn started(&mut self, cx: &mut dyn HostContext, _waker: Waker) {
    cx.windows()
      .expect("a desktop host has windows")
      .open(&desc("window A", 100.0, 80.0));
  }

  fn woken(&mut self, _cx: &mut dyn HostContext) {}

  fn surface_changed(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, info: SurfaceInfo) {
    self.sizes.insert(surface, info.size);
    cx.request_frame(surface);
  }

  fn surface_lost(&mut self, _cx: &mut dyn HostContext, _surface: SurfaceId, _loss: SurfaceLoss) {}

  fn event(&mut self, _cx: &mut dyn HostContext, _event: HostEvent) {}

  fn frame(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, target: Target<'_>, _time: Duration) {
    let Target::Handles(handles) = target else {
      panic!("a window's target is its handles");
    };
    let size = self.sizes[&surface];
    match self.step {
      Step::Draw => {
        for empty in [PhysicalSize::new(0, size.height), PhysicalSize::new(size.width, 0)] {
          let view = SurfaceView {
            id: surface,
            size: empty,
            scale: 1.0,
            target: Target::Handles(handles),
          };
          self
            .renderer
            .render(&Scene::default(), view)
            .expect("a size without pixels draws nothing");
        }
        self.fill(surface, size, handles, RED);
        assert_shows(handles, size.width / 2, size.height / 2, RED, "the first frame");
        let windows = cx.windows().expect("a desktop host has windows");
        windows.update(surface, &desc("window A", 160.0, 120.0));
        self.step = Step::Resized(size);
      }
      Step::Resized(old) if size == old => cx.request_frame(surface),
      Step::Resized(_) => {
        self.fill(surface, size, handles, BLUE);
        assert_shows(handles, size.width - 2, size.height - 2, BLUE, "the corner after the resize");
        cx.windows()
          .expect("a desktop host has windows")
          .open(&desc("window B", 100.0, 80.0));
        self.step = Step::Reused { lost: surface };
      }
      Step::Reused { lost } if surface != lost => {
        self.renderer.surface_lost(lost);
        self.fill(lost, size, handles, GREEN);
        assert_shows(handles, size.width / 2, size.height / 2, GREEN, "the second window");
        self.done.set(true);
        cx.exit();
      }
      Step::Reused { .. } => {}
    }
  }
}

fn desc(title: &str, width: f32, height: f32) -> WindowDesc {
  WindowDesc {
    title: title.into(),
    size: LogicalSize {
      width: Some(width),
      height: Some(height),
    },
    chrome: Chrome::System,
    resizable: Axes::BOTH,
  }
}

/// Checks that the window behind `handles` shows `expected` at `x`, `y`, waiting up to 2 s for the
/// presented frame to reach the X server, which refuses to read a window it hasn't shown yet.
fn assert_shows(handles: &dyn RawHandles, x: u32, y: u32, expected: [u8; 3], what: &str) {
  let window = match handles.window_handle().expect("the window's handle").as_raw() {
    RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).expect("an X11 window id"),
    RawWindowHandle::Xcb(handle) => handle.window.get(),
    other => panic!("not an X11 window: {other:?}"),
  };
  let (connection, _) = RustConnection::connect(None).expect("the X server");
  let x = i16::try_from(x).expect("a pixel inside the window");
  let y = i16::try_from(y).expect("a pixel inside the window");
  let start = Instant::now();
  loop {
    let read = connection
      .get_image(ImageFormat::Z_PIXMAP, window, x, y, 1, 1, !0)
      .expect("asks for the pixel")
      .reply()
      .map(|image| match image.data[..] {
        [b, g, r, ..] => [r, g, b],
        _ => panic!("a 32-bit pixel, got {:?}", image.data),
      });
    if read.as_ref().is_ok_and(|rgb| *rgb == expected) {
      return;
    }
    assert!(
      start.elapsed() < Duration::from_secs(2),
      "{what}: pixel {x},{y} is {read:?}, expected {expected:?}"
    );
    std::thread::sleep(Duration::from_millis(50));
  }
}
