//! Events a host reports.

use artic_render::SurfaceId;

/// Something that happened, reported to [`AppHandler::event`](crate::AppHandler::event).
///
/// Surface facts (size, scale, display, refresh rate) aren't events: they arrive through
/// [`AppHandler::surface_changed`](crate::AppHandler::surface_changed). Raw input, the color
/// scheme and the mobile events are added by the issues that deliver them, which is why the enum
/// is non-exhaustive.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum HostEvent {
  /// The user asked to close the surface's window, such as with its close button. The window
  /// stays open until artic closes it or exits.
  CloseRequested {
    /// The window's surface.
    surface: SurfaceId,
  },
  /// The app went to the background. Its surfaces were reported lost first, as
  /// [`SurfaceLoss::Suspended`](crate::SurfaceLoss::Suspended).
  Suspended,
  /// The app came back to the foreground. Its surfaces are reported again after this.
  Resumed,
  /// A screen reader started reading the surface. The next frame hands it the whole tree, through
  /// [`HostContext::accessibility`](crate::HostContext::accessibility).
  AccessibilityRequested {
    /// The window's surface.
    surface: SurfaceId,
  },
}
