//! What a window description asks of a winit window: the attributes it opens with, and the
//! changes that follow a new description.

use artic_platform::{Axes, Chrome, WindowDesc};
use winit::dpi::LogicalSize;
use winit::window::{Window, WindowAttributes};

/// The width a window opens at when its description sets only its height.
const FALLBACK_WIDTH: f64 = 800.0;
/// The height a window opens at when its description sets only its width.
const FALLBACK_HEIGHT: f64 = 600.0;
/// The limit on an axis the user can resize while the other is locked. winit sets both axes of a
/// limit at once, and the platforms keep it in a 32-bit integer, so the free axis needs a finite
/// value: this one is beyond any display, more than six times the width of a 16K screen.
const UNLIMITED: f64 = 100_000.0;

/// The size a window with `desc` gets, with `current` filling the axes `desc` leaves to the host,
/// or `None` when it sets neither.
fn logical_size(desc: &WindowDesc, current: LogicalSize<f64>) -> Option<LogicalSize<f64>> {
  let (width, height) = (desc.size.width, desc.size.height);
  (width.is_some() || height.is_some()).then(|| {
    LogicalSize::new(
      width.map_or(current.width, f64::from),
      height.map_or(current.height, f64::from),
    )
  })
}

/// The smallest and largest size the user can resize a window to: both locked at `size` on an axis
/// its content sizes, and none when the user can resize every axis.
fn limits(resizable: Axes, size: LogicalSize<f64>) -> (Option<LogicalSize<f64>>, Option<LogicalSize<f64>>) {
  match (resizable.width, resizable.height) {
    (true, false) => (
      Some(LogicalSize::new(0.0, size.height)),
      Some(LogicalSize::new(UNLIMITED, size.height)),
    ),
    (false, true) => (
      Some(LogicalSize::new(size.width, 0.0)),
      Some(LogicalSize::new(size.width, UNLIMITED)),
    ),
    (false, false) => (Some(size), Some(size)),
    (true, true) => (None, None),
  }
}

/// One change a window makes to follow a new description.
#[derive(Debug, PartialEq)]
pub(crate) enum Step {
  Title(String),
  Decorations(bool),
  Size(LogicalSize<f64>),
  Resizable(bool),
  Limits(Option<LogicalSize<f64>>, Option<LogicalSize<f64>>),
}

/// The changes, in order, that take a window of `current` size from the description `before` to
/// `after`: only what differs, and everything when nothing was applied before. A new size or
/// resizable axes clear the old limits first, so they can't clamp the new size; a size that sets
/// neither axis requests none.
pub(crate) fn steps(before: Option<&WindowDesc>, after: &WindowDesc, current: LogicalSize<f64>) -> Vec<Step> {
  let differs = |part: fn(&WindowDesc, &WindowDesc) -> bool| before.is_none_or(|before| part(before, after));
  let mut steps = Vec::new();
  if differs(|before, after| before.title != after.title) {
    steps.push(Step::Title(after.title.clone()));
  }
  if differs(|before, after| before.chrome != after.chrome) {
    steps.push(Step::Decorations(after.chrome == Chrome::System));
  }
  let resized = differs(|before, after| before.size != after.size);
  if resized || differs(|before, after| before.resizable != after.resizable) {
    let requested = if resized { logical_size(after, current) } else { None };
    let size = requested.unwrap_or(current);
    steps.push(Step::Limits(None, None));
    if let Some(size) = requested {
      steps.push(Step::Size(size));
    }
    steps.push(Step::Resizable(after.resizable != Axes::NONE));
    let (min, max) = limits(after.resizable, size);
    steps.push(Step::Limits(min, max));
  }
  steps
}

/// The attributes a window opens with for `desc`.
pub(crate) fn attributes(desc: &WindowDesc) -> WindowAttributes {
  let mut attributes = Window::default_attributes()
    .with_title(desc.title.clone())
    .with_decorations(desc.chrome == Chrome::System)
    .with_resizable(desc.resizable != Axes::NONE);
  if let Some(size) = logical_size(desc, LogicalSize::new(FALLBACK_WIDTH, FALLBACK_HEIGHT)) {
    attributes = attributes.with_inner_size(size);
    if let (Some(min), Some(max)) = limits(desc.resizable, size) {
      attributes = attributes.with_min_inner_size(min).with_max_inner_size(max);
    }
  }
  attributes
}

#[cfg(test)]
mod tests {
  use super::*;
  use pretty_assertions::assert_eq;
  use winit::dpi::Size;

  // A description becomes winit attributes: the title, the decorations its chrome asks for, the
  // set width with the fallback height, and the height locked where content sizes it.
  #[test]
  fn attributes_follow_the_description() {
    let desc = WindowDesc {
      title: "notes".into(),
      size: artic_platform::LogicalSize {
        width: Some(300.0),
        height: None,
      },
      chrome: Chrome::None,
      resizable: Axes {
        width: true,
        height: false,
      },
    };

    let attributes = attributes(&desc);

    assert_eq!(attributes.title, "notes");
    assert!(!attributes.decorations);
    assert!(attributes.resizable);
    assert_eq!(
      attributes.inner_size,
      Some(Size::Logical(LogicalSize::new(300.0, FALLBACK_HEIGHT)))
    );
    assert_eq!(
      attributes.min_inner_size,
      Some(Size::Logical(LogicalSize::new(0.0, FALLBACK_HEIGHT)))
    );
    assert_eq!(
      attributes.max_inner_size,
      Some(Size::Logical(LogicalSize::new(UNLIMITED, FALLBACK_HEIGHT)))
    );
  }

  // A window whose content sizes both axes is locked at its size, so the user can't resize it.
  #[test]
  fn locked_window_keeps_its_size_limits() {
    let size = LogicalSize::new(300.0, 200.0);

    assert_eq!(limits(Axes::NONE, size), (Some(size), Some(size)));
    assert_eq!(limits(Axes::BOTH, size), (None, None));
  }

  /// A window locked at 300 by 200, as a window whose content sizes it is.
  fn locked() -> WindowDesc {
    WindowDesc {
      title: "notes".into(),
      size: artic_platform::LogicalSize {
        width: Some(300.0),
        height: Some(200.0),
      },
      chrome: Chrome::System,
      resizable: Axes::NONE,
    }
  }

  // An update applies only what changed, so a new title leaves a size the user chose alone.
  #[test]
  fn title_change_only_sets_the_title() {
    let renamed = WindowDesc {
      title: "notes*".into(),
      ..locked()
    };

    assert_eq!(
      steps(Some(&locked()), &renamed, LogicalSize::new(300.0, 200.0)),
      [Step::Title("notes*".into())]
    );
  }

  // A chrome change turns the system's decorations off for chrome the app draws, and back on.
  #[test]
  fn chrome_change_sets_the_decorations() {
    let drawn = WindowDesc {
      chrome: Chrome::None,
      ..locked()
    };
    let current = LogicalSize::new(300.0, 200.0);

    assert_eq!(steps(Some(&locked()), &drawn, current), [Step::Decorations(false)]);
    assert_eq!(steps(Some(&drawn), &locked(), current), [Step::Decorations(true)]);
  }

  // A window whose size goes back to the host's keeps its current size: no resize is requested.
  #[test]
  fn unset_size_requests_no_resize() {
    let unset = WindowDesc {
      size: artic_platform::LogicalSize::default(),
      resizable: Axes::BOTH,
      ..locked()
    };

    assert!(
      !steps(Some(&locked()), &unset, LogicalSize::new(300.0, 200.0))
        .iter()
        .any(|step| matches!(step, Step::Size(_)))
    );
  }

  // A locked window that grows clears its old limits before asking for the new size, so the old
  // ones can't clamp it, then locks at the new size.
  #[test]
  fn resize_clears_old_limits_first() {
    let grown = WindowDesc {
      size: artic_platform::LogicalSize {
        width: Some(300.0),
        height: Some(400.0),
      },
      ..locked()
    };
    let size = LogicalSize::new(300.0, 400.0);

    assert_eq!(
      steps(Some(&locked()), &grown, LogicalSize::new(300.0, 200.0)),
      [
        Step::Limits(None, None),
        Step::Size(size),
        Step::Resizable(false),
        Step::Limits(Some(size), Some(size)),
      ]
    );
  }

  // A description that sets no size leaves the size to the platform.
  #[test]
  fn unset_size_is_left_to_the_platform() {
    let desc = WindowDesc {
      title: String::new(),
      size: artic_platform::LogicalSize::default(),
      chrome: Chrome::System,
      resizable: Axes::BOTH,
    };

    let attributes = attributes(&desc);

    assert_eq!(attributes.inner_size, None);
    assert!(attributes.decorations);
  }
}
