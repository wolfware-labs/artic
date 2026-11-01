//! winit's window events, translated into what the host reports.

use artic_render::PhysicalSize;
use winit::event::WindowEvent;

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

/// What `event` means: new surface facts, a close request, a frame, a move, the window's
/// destruction, or nothing. A scale change carries no size the host can read, so its new size is
/// read with `inner_size`.
pub(crate) fn translate(event: &WindowEvent, inner_size: impl FnOnce() -> winit::dpi::PhysicalSize<u32>) -> Translated {
  match event {
    WindowEvent::Resized(size) => Translated::Changed {
      size: physical(*size),
      scale: None,
    },
    WindowEvent::ScaleFactorChanged { scale_factor, .. } => Translated::Changed {
      size: physical(inner_size()),
      scale: Some(*scale_factor as f32),
    },
    WindowEvent::CloseRequested => Translated::Close,
    WindowEvent::RedrawRequested => Translated::Redraw,
    WindowEvent::Moved(_) => Translated::Moved,
    WindowEvent::Destroyed => Translated::Destroyed,
    _ => Translated::Ignored,
  }
}

/// winit's `size` as artic's.
fn physical(size: winit::dpi::PhysicalSize<u32>) -> PhysicalSize {
  PhysicalSize {
    width: size.width,
    height: size.height,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use winit::dpi::{PhysicalPosition, PhysicalSize as WinitSize};

  /// A window size that fails the test when read.
  fn unread() -> WinitSize<u32> {
    panic!("read the window's size")
  }

  // Each window event the host acts on translates to what it means; a resize carries its size
  // without reading the window's.
  #[test]
  fn window_events_translate() {
    let size = PhysicalSize { width: 30, height: 20 };

    assert_eq!(
      translate(&WindowEvent::Resized(WinitSize::new(30, 20)), unread),
      Translated::Changed { size, scale: None }
    );
    assert_eq!(translate(&WindowEvent::CloseRequested, unread), Translated::Close);
    assert_eq!(translate(&WindowEvent::RedrawRequested, unread), Translated::Redraw);
    assert_eq!(
      translate(&WindowEvent::Moved(PhysicalPosition::new(1, 2)), unread),
      Translated::Moved
    );
    assert_eq!(translate(&WindowEvent::Destroyed, unread), Translated::Destroyed);
    assert_eq!(translate(&WindowEvent::Focused(true), unread), Translated::Ignored);
  }
}
