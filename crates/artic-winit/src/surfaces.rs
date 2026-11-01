//! The host's windows, each with its artic surface id, last reported facts and last applied
//! description.

use artic_platform::{SurfaceInfo, WindowDesc};
use artic_render::SurfaceId;

/// One open window.
#[derive(Debug)]
struct Entry<K, W> {
  id: SurfaceId,
  key: K,
  window: W,
  info: Option<SurfaceInfo>,
  desc: Option<WindowDesc>,
}

/// The open windows, keyed by the windowing library's id `K`, in the order they opened.
///
/// Generic over the window type `W`, so tests use `()`.
#[derive(Debug)]
pub(crate) struct Surfaces<K, W> {
  entries: Vec<Entry<K, W>>,
  next: u64,
  lost: Vec<(SurfaceId, Option<W>)>,
}

impl<K, W> Default for Surfaces<K, W> {
  fn default() -> Self {
    Self {
      entries: Vec::new(),
      next: 1,
      lost: Vec::new(),
    }
  }
}

impl<K: PartialEq + Copy, W> Surfaces<K, W> {
  /// A new surface id, not yet tied to a window.
  pub(crate) fn reserve(&mut self) -> SurfaceId {
    let id = SurfaceId::new(self.next);
    self.next += 1;
    id
  }

  /// Adds `window`, known to the windowing library as `key`, under a new surface id.
  pub(crate) fn insert(&mut self, key: K, window: W) -> SurfaceId {
    let id = self.reserve();
    self.entries.push(Entry {
      id,
      key,
      window,
      info: None,
      desc: None,
    });
    id
  }

  /// Records that surface `id` never got a window, because it couldn't be created, so it's
  /// reported lost.
  pub(crate) fn lose(&mut self, id: SurfaceId) {
    self.lost.push((id, None));
  }

  /// Closes surface `id`, keeping its window until its loss is reported, so the handles a renderer
  /// holds stay valid until then.
  pub(crate) fn close(&mut self, id: SurfaceId) {
    if let Some(index) = self.entries.iter().position(|entry| entry.id == id) {
      let entry = self.entries.remove(index);
      self.lost.push((entry.id, Some(entry.window)));
    }
  }

  /// The surfaces gone since the last call, oldest first, each with its window to drop once its loss
  /// is reported, emptying the list.
  pub(crate) fn take_lost(&mut self) -> Vec<(SurfaceId, Option<W>)> {
    std::mem::take(&mut self.lost)
  }

  /// The surface of the window the library calls `key`.
  pub(crate) fn id(&self, key: K) -> Option<SurfaceId> {
    self.entries.iter().find(|entry| entry.key == key).map(|entry| entry.id)
  }

  /// The window of surface `id`.
  pub(crate) fn window(&self, id: SurfaceId) -> Option<&W> {
    self.entries.iter().find(|entry| entry.id == id).map(|entry| &entry.window)
  }

  /// The surfaces and their windows, in the order they opened.
  pub(crate) fn windows(&self) -> impl Iterator<Item = (SurfaceId, &W)> {
    self.entries.iter().map(|entry| (entry.id, &entry.window))
  }

  /// Records `info` for surface `id`, and whether it differs from what was last reported.
  pub(crate) fn update(&mut self, id: SurfaceId, info: SurfaceInfo) -> bool {
    match self.entries.iter_mut().find(|entry| entry.id == id) {
      Some(entry) if entry.info != Some(info) => {
        entry.info = Some(info);
        true
      }
      _ => false,
    }
  }

  /// Records `desc` as the description last applied to surface `id`'s window, and returns the one
  /// before it.
  pub(crate) fn apply(&mut self, id: SurfaceId, desc: WindowDesc) -> Option<WindowDesc> {
    let entry = self.entries.iter_mut().find(|entry| entry.id == id)?;
    entry.desc.replace(desc)
  }

  /// Forgets the facts reported for every surface, so each is reported again, such as after the
  /// surfaces are recreated on resume.
  pub(crate) fn forget_facts(&mut self) {
    for entry in &mut self.entries {
      entry.info = None;
    }
  }

  /// Removes every surface and returns it with its window, in the order they opened.
  pub(crate) fn drain(&mut self) -> Vec<(SurfaceId, W)> {
    self.entries.drain(..).map(|entry| (entry.id, entry.window)).collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use artic_platform::{Axes, Chrome, DisplayId, LogicalSize, SurfaceInfo, WindowDesc};
  use artic_render::PhysicalSize;
  use pretty_assertions::assert_eq;
  use std::rc::Rc;

  /// Facts for a 100 × 100 surface at scale 1 on `display`.
  fn info(display: u64) -> SurfaceInfo {
    SurfaceInfo {
      size: PhysicalSize { width: 100, height: 100 },
      scale: 1.0,
      display: DisplayId::new(display),
      refresh_rate: Some(60.0),
    }
  }

  // Each window gets its own surface id, and its window key maps back to it.
  #[test]
  fn surfaces_get_distinct_ids() {
    let mut surfaces = Surfaces::<u32, ()>::default();

    let first = surfaces.insert(10, ());
    let second = surfaces.insert(20, ());

    assert_ne!(first, second);
    assert_eq!(
      (surfaces.id(10), surfaces.id(20), surfaces.id(30)),
      (Some(first), Some(second), None)
    );
  }

  // A surface's window is found by its id, and every open window is listed in the order it opened.
  #[test]
  fn windows_are_found_by_surface() {
    let mut surfaces = Surfaces::<u32, &str>::default();
    let first = surfaces.insert(10, "first");
    let second = surfaces.insert(20, "second");

    assert_eq!(
      (surfaces.window(first), surfaces.window(second)),
      (Some(&"first"), Some(&"second"))
    );
    assert_eq!(
      surfaces.windows().collect::<Vec<_>>(),
      [(first, &"first"), (second, &"second")]
    );
  }

  // New facts are reported once: the same facts again are no change, and a move to another display
  // with nothing else different is one.
  #[test]
  fn display_change_is_reported_once() {
    let mut surfaces = Surfaces::<u32, ()>::default();
    let id = surfaces.insert(10, ());

    assert!(surfaces.update(id, info(1)));
    assert!(!surfaces.update(id, info(1)));
    assert!(surfaces.update(id, info(2)));
    assert!(!surfaces.update(id, info(2)));
  }

  // Draining reports what's still open, in the order it opened.
  #[test]
  fn surfaces_drain_in_open_order() {
    let mut surfaces = Surfaces::<u32, ()>::default();
    let [first, second, third] = [10, 20, 30].map(|key| surfaces.insert(key, ()));

    surfaces.close(second);

    assert_eq!(
      surfaces.drain().into_iter().map(|(id, ())| id).collect::<Vec<_>>(),
      [first, third]
    );
    assert_eq!(surfaces.id(10), None);
  }

  // A window that couldn't be created still has an id, which is reported lost.
  #[test]
  fn failed_open_reports_lost() {
    let mut surfaces = Surfaces::<u32, ()>::default();

    let id = surfaces.reserve();
    surfaces.lose(id);

    assert_eq!(surfaces.take_lost().into_iter().map(|(id, _)| id).collect::<Vec<_>>(), [id]);
    assert!(surfaces.take_lost().is_empty());
  }

  // A closed window is kept until its loss is reported, so its handles stay valid until then.
  #[test]
  fn closed_window_lives_until_reported() {
    let mut surfaces = Surfaces::<u32, Rc<()>>::default();
    let window = Rc::new(());
    let id = surfaces.insert(10, Rc::clone(&window));

    surfaces.close(id);

    assert_eq!(Rc::strong_count(&window), 2);
    let lost = surfaces.take_lost();
    assert_eq!(lost.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [id]);
    drop(lost);
    assert_eq!(Rc::strong_count(&window), 1);
  }

  // Each window keeps the description last applied to it, returned when the next one replaces it.
  #[test]
  fn applied_description_is_kept_per_window() {
    let mut surfaces: Surfaces<u32, ()> = Surfaces::default();
    let id = surfaces.insert(1, ());
    let desc = |title: &str| WindowDesc {
      title: title.into(),
      size: LogicalSize::default(),
      chrome: Chrome::System,
      resizable: Axes::BOTH,
    };

    assert_eq!(surfaces.apply(id, desc("first")), None);
    assert_eq!(surfaces.apply(id, desc("second")), Some(desc("first")));
    assert_eq!(surfaces.apply(SurfaceId::new(99), desc("none")), None);
  }

  // Forgetting the facts makes the same facts count as a change again.
  #[test]
  fn forgotten_facts_are_reported_again() {
    let mut surfaces = Surfaces::<u32, ()>::default();
    let id = surfaces.insert(10, ());
    surfaces.update(id, info(1));

    surfaces.forget_facts();

    assert!(surfaces.update(id, info(1)));
  }
}
