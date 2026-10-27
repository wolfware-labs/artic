//! winit's window events, translated into what the host reports.

use artic_render::PhysicalSize;
use winit::event::WindowEvent;
use winit::window::Window;

/// What a window event means to the host.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Translated {
  Changed { size: PhysicalSize, scale: Option<f32> },
  Close,
  Redraw,
  Moved,
  Destroyed,
  Ignored,
}

/// What `event` on `window` means: new surface facts, a close request, a frame, a move, the
/// window's destruction, or nothing. A scale change carries no size the host can read, so its
/// new size is read from the window.
pub(crate) fn translate(event: &WindowEvent, window: &Window) -> Translated {
  match event {
    WindowEvent::Resized(size) => Translated::Changed {
      size: PhysicalSize {
        width: size.width,
        height: size.height,
      },
      scale: None,
    },
    WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
      let size = window.inner_size();
      Translated::Changed {
        size: PhysicalSize {
          width: size.width,
          height: size.height,
        },
        scale: Some(*scale_factor as f32),
      }
    }
    WindowEvent::CloseRequested => Translated::Close,
    WindowEvent::RedrawRequested => Translated::Redraw,
    WindowEvent::Moved(_) => Translated::Moved,
    WindowEvent::Destroyed => Translated::Destroyed,
    _ => Translated::Ignored,
  }
}
