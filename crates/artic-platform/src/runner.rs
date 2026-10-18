//! The frame pipeline's `AppHandler`: which host surface shows which `Surface`, when each one
//! draws, and what a frame runs.

use crate::{
  AppHandler, Axes, Error, FrameServices, FrameStats, Hooks, Host, HostContext, HostEvent, LogicalSize, Pipeline, SurfaceInfo,
  SurfaceLoss, Waker, WindowDesc,
};
use artic_core::dispatcher::Dispatcher;
use artic_core::property::Affects;
use artic_core::reactive::flush_effects;
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::{Application, Chrome, Desktop, ElementTypes, Surface};
use artic_layout::geometry::Size;
use artic_layout::text::TextEngine;
use artic_render::{RenderError, Renderer, SurfaceId, Target};
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::{self, Debug, Formatter};
use std::rc::Rc;
use std::time::Duration;

/// Drives a tree's frames from a host: it presents the open [`Surface`]s of the tree's
/// [`Application`], asks for a frame when something changed, and runs each frame's stages.
///
/// Build it with the tree, a renderer, a text engine and the `Application` node, register hooks, and hand it to
/// [`run`]. After every call from the host it reconciles the surfaces with the host: a surface
/// opened is presented, one closed or taken out of the tree is taken down, and one whose title,
/// size or chrome changed is updated.
pub struct Runner<R> {
  tree: UiTree,
  renderer: R,
  text: Box<dyn TextEngine>,
  types: ElementTypes,
  hooks: Hooks,
  application: NodeId,
  bound: Vec<Bound>,
  spare: Vec<(SurfaceId, Option<SurfaceInfo>)>,
  lost: HashSet<NodeId>,
  error: Rc<RefCell<Option<Error>>>,
}

/// A `Surface` the host presents, and what the runner knows about it.
struct Bound {
  id: SurfaceId,
  node: NodeId,
  applied: Option<WindowDesc>,
  info: Option<SurfaceInfo>,
  needs_frame: bool,
  suspended: bool,
  pipeline: Pipeline,
  stats: Option<FrameStats>,
}

/// A surface's `Desktop` settings, as one reconcile reads them.
struct Settings {
  width: Option<f32>,
  height: Option<f32>,
  content: Axes,
  chrome: Chrome,
}

impl Settings {
  /// Reads `node`'s settings, holding each property on it, so that a change to it, its own or
  /// inherited, marks the surface.
  fn of(tree: &mut UiTree, node: NodeId) -> Settings {
    Settings {
      width: tree.attached_property(node, Desktop::WIDTH).get(),
      height: tree.attached_property(node, Desktop::HEIGHT).get(),
      content: Axes::from(tree.attached_property(node, Desktop::SIZE_TO_CONTENT).get()),
      chrome: tree.attached_property(node, Desktop::CHROME).get(),
    }
  }

  /// What the surface's content is measured in to size its window, or `None` when it sizes no
  /// axis: `∞` on the axes it sizes, and on the others the set size, else `host`'s, the window's
  /// current logical size if it has one.
  fn available(&self, host: Option<Size>) -> Option<Size> {
    let axis = |content: bool, set: Option<f32>, host: Option<f32>| {
      if content {
        f32::INFINITY
      } else {
        set.or(host).unwrap_or(f32::INFINITY)
      }
    };
    (self.content != Axes::NONE).then(|| {
      Size::new(
        axis(self.content.width, self.width, host.map(|size| size.width)),
        axis(self.content.height, self.height, host.map(|size| size.height)),
      )
    })
  }

  /// The window titled `title`, sized to `desired` on the axes its content sizes.
  fn desc(&self, title: String, desired: Option<Size>) -> WindowDesc {
    let axis = |content: bool, set: Option<f32>, desired: Option<f32>| if content { desired } else { set };
    WindowDesc {
      title,
      size: LogicalSize {
        width: axis(self.content.width, self.width, desired.map(|size| size.width)),
        height: axis(self.content.height, self.height, desired.map(|size| size.height)),
      },
      chrome: self.chrome,
      resizable: Axes {
        width: !self.content.width,
        height: !self.content.height,
      },
    }
  }
}

impl<R: Renderer> Runner<R> {
  /// A runner for `tree`, drawing with `renderer` the surfaces of `application`, and shaping text
  /// with `text`, which every surface shares.
  ///
  /// # Panics
  ///
  /// Panics if `application` isn't an [`Application`] in `tree`.
  #[must_use]
  #[track_caller]
  pub fn new(tree: UiTree, renderer: R, text: Box<dyn TextEngine>, application: NodeId) -> Self {
    assert!(
      tree.get_as::<Application>(application).is_some(),
      "node {application:?} isn't an Application"
    );
    Runner {
      tree,
      renderer,
      text,
      types: ElementTypes::from_link_time(),
      hooks: Hooks::default(),
      application,
      bound: Vec::new(),
      spare: Vec::new(),
      lost: HashSet::new(),
      error: Rc::new(RefCell::new(None)),
    }
  }

  /// The hooks each frame runs.
  pub fn hooks(&mut self) -> &mut Hooks {
    &mut self.hooks
  }

  /// What `surface`'s last frame did, or `None` if it hasn't drawn.
  #[must_use]
  pub fn stats(&self, surface: SurfaceId) -> Option<FrameStats> {
    self.bound.iter().find(|bound| bound.id == surface)?.stats
  }

  fn drain() {
    loop {
      Dispatcher::run_pending();
      flush_effects();
      if Dispatcher::is_idle() {
        break;
      }
    }
  }

  fn mark_all(&mut self, cx: &mut dyn HostContext, except: Option<SurfaceId>) {
    for bound in self.bound.iter_mut().filter(|bound| !bound.suspended) {
      bound.needs_frame = true;
      if Some(bound.id) != except {
        cx.request_frame(bound.id);
      }
    }
  }

  fn settle(&mut self, cx: &mut dyn HostContext) {
    Self::drain();
    if self.tree.take_pending() {
      self.mark_all(cx, None);
    }
    self.reconcile(cx);
  }

  #[track_caller]
  fn reconcile(&mut self, cx: &mut dyn HostContext) {
    let surfaces = self.surfaces();
    let shown: Vec<NodeId> = surfaces
      .iter()
      .copied()
      .filter(|node| {
        self
          .tree
          .get_as::<Surface>(*node)
          .is_some_and(|surface| surface.is_open.get())
      })
      .collect();
    self.lost.retain(|node| shown.contains(node));
    let mut index = 0;
    while index < self.bound.len() {
      if shown.contains(&self.bound[index].node) {
        index += 1;
        continue;
      }
      let bound = self.bound.remove(index);
      match (bound.applied.is_some(), cx.windows()) {
        (true, Some(windows)) => windows.close(bound.id),
        _ => self.spare.push((bound.id, bound.info)),
      }
    }
    self.tree.clear_dirty(self.application, Affects::SURFACE);
    for node in surfaces {
      let settings = Settings::of(&mut self.tree, node);
      let dirty = self.tree.dirty(node).contains(Affects::SURFACE);
      self.tree.clear_dirty(node, Affects::SURFACE);
      if !shown.contains(&node) || self.lost.contains(&node) {
        continue;
      }
      match self.bound.iter().position(|bound| bound.node == node) {
        Some(index) if dirty => self.update(cx, index, &settings),
        Some(_) => {}
        None => self.present(cx, node, &settings),
      }
    }
    if shown.iter().all(|node| self.lost.contains(node)) {
      cx.exit();
    }
  }

  fn present(&mut self, cx: &mut dyn HostContext, node: NodeId, settings: &Settings) {
    if let Some(windows) = cx.windows() {
      let mut pipeline = Pipeline::new();
      let desired = settings.available(None).map(|available| {
        let mut services = FrameServices {
          types: &self.types,
          text: &mut *self.text,
        };
        pipeline.measure(&mut self.tree, node, available, &mut services)
      });
      let desc = settings.desc(self.title(node), desired);
      let id = windows.open(&desc);
      self.bound.push(Bound {
        id,
        node,
        applied: Some(desc),
        info: None,
        needs_frame: true,
        suspended: false,
        pipeline,
        stats: None,
      });
    } else if !self.spare.is_empty() {
      let (id, info) = self.spare.remove(0);
      self.bound.push(Bound {
        id,
        node,
        applied: None,
        info,
        needs_frame: true,
        suspended: false,
        pipeline: Pipeline::new(),
        stats: None,
      });
      if info.is_some() {
        cx.request_frame(id);
      }
    }
  }

  fn update(&mut self, cx: &mut dyn HostContext, index: usize, settings: &Settings) {
    let Some(windows) = cx.windows() else {
      return;
    };
    let node = self.bound[index].node;
    let title = self.title(node);
    let bound = &mut self.bound[index];
    if bound.applied.is_none() {
      return;
    }
    let host = bound
      .info
      .map(|info| Size::new(info.size.width as f32 / info.scale, info.size.height as f32 / info.scale));
    let desired = settings.available(host).map(|available| {
      let mut services = FrameServices {
        types: &self.types,
        text: &mut *self.text,
      };
      bound.pipeline.measure(&mut self.tree, node, available, &mut services)
    });
    let desc = settings.desc(title, desired);
    if bound.applied.as_ref() != Some(&desc) {
      windows.update(bound.id, &desc);
      bound.applied = Some(desc);
    }
  }

  #[track_caller]
  fn surfaces(&self) -> Vec<NodeId> {
    let surfaces: Vec<NodeId> = self.tree.children(TreeKind::Logical, self.application).collect();
    for node in &surfaces {
      assert!(
        self.tree.get_as::<Surface>(*node).is_some(),
        "the Application's child {node:?} isn't a Surface; an Application holds only Surfaces"
      );
      assert!(
        self.tree.parent(TreeKind::Visual, *node) == Some(self.application),
        "the Surface {node:?} isn't a visual child of the Application; each Surface is a logical and visual child of it, so no surface draws inside another"
      );
    }
    surfaces
  }

  fn title(&self, node: NodeId) -> String {
    self
      .tree
      .get_as::<Surface>(node)
      .map(|surface| surface.title.get())
      .unwrap_or_default()
  }

  fn failed(&mut self, cx: &mut dyn HostContext, index: usize, error: RenderError) {
    if matches!(error, RenderError::SurfaceLost) {
      let id = self.bound[index].id;
      self.renderer.surface_lost(id);
      self.bound[index].needs_frame = true;
      cx.request_frame(id);
      return;
    }
    let error = Error::Render(error);
    for hook in &mut self.hooks.on_error {
      hook(&error);
    }
    *self.error.borrow_mut() = Some(error);
    cx.exit();
  }
}

impl<R: Renderer> AppHandler for Runner<R> {
  fn started(&mut self, cx: &mut dyn HostContext, _waker: Waker) {
    self.settle(cx);
  }

  fn woken(&mut self, cx: &mut dyn HostContext) {
    self.settle(cx);
  }

  fn surface_changed(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, info: SurfaceInfo) {
    if let Some(bound) = self.bound.iter_mut().find(|bound| bound.id == surface) {
      bound.info = Some(info);
      bound.suspended = false;
      bound.needs_frame = true;
      cx.request_frame(surface);
    } else if let Some(spare) = self.spare.iter_mut().find(|(id, _)| *id == surface) {
      spare.1 = Some(info);
    } else {
      self.spare.push((surface, Some(info)));
    }
    self.settle(cx);
  }

  fn surface_lost(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, loss: SurfaceLoss) {
    self.renderer.surface_lost(surface);
    match (self.bound.iter().position(|bound| bound.id == surface), loss) {
      (Some(index), SurfaceLoss::Suspended) => self.bound[index].suspended = true,
      (Some(index), SurfaceLoss::Gone) => {
        let bound = self.bound.remove(index);
        self.lost.insert(bound.node);
      }
      (None, SurfaceLoss::Gone) => self.spare.retain(|(id, _)| *id != surface),
      (None, SurfaceLoss::Suspended) => {}
    }
    self.settle(cx);
  }

  fn event(&mut self, cx: &mut dyn HostContext, event: HostEvent) {
    if let HostEvent::CloseRequested { surface } = event
      && let Some(bound) = self.bound.iter().find(|bound| bound.id == surface)
      && let Some(node) = self.tree.get_as::<Surface>(bound.node)
    {
      node.is_open.set(false);
    }
    self.settle(cx);
  }

  fn frame(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, target: Target<'_>, time: Duration) {
    let Some(index) = self.bound.iter().position(|bound| bound.id == surface && !bound.suspended) else {
      return;
    };
    let Some(info) = self.bound[index].info else {
      return;
    };
    let root = self.bound[index].node;
    Self::drain();
    for hook in &mut self.hooks.before_layout {
      hook(&mut self.tree, root, &info, time);
    }
    Self::drain();
    if self.tree.take_pending() {
      self.mark_all(cx, Some(surface));
    }
    let open = self.tree.get_as::<Surface>(root).is_some_and(|surface| surface.is_open.get());
    if !self.surfaces().contains(&root) || !open {
      self.settle(cx);
      return;
    }
    let nested = self
      .tree
      .descendants(TreeKind::Visual, root)
      .find(|node| self.tree.get_as::<Surface>(*node).is_some());
    if let Some(nested) = nested {
      panic!("the Surface {nested:?} is inside the Surface {root:?}; no surface draws inside another");
    }
    let content = if self.bound[index].applied.is_some() {
      Axes::from(self.tree.attached(root, Desktop::SIZE_TO_CONTENT))
    } else {
      Axes::NONE
    };
    let window = &mut self.bound[index];
    let result = if window.needs_frame {
      window.needs_frame = false;
      let mut services = FrameServices {
        types: &self.types,
        text: &mut *self.text,
      };
      let frame = window
        .pipeline
        .prepare(&mut self.tree, root, surface, &info, &mut services, content);
      if let (Some(applied), Some(desired)) = (&window.applied, window.pipeline.desired(root)) {
        let requested = applied.size;
        let grew = (content.width && requested.width != Some(desired.width))
          || (content.height && requested.height != Some(desired.height));
        if grew {
          self.tree.mark_dirty(root, Affects::SURFACE);
        }
      }
      Some(window.pipeline.present(&mut self.renderer, frame, target))
    } else {
      window.pipeline.redraw(&mut self.renderer, surface, &info, target)
    };
    let Some(result) = result else {
      return;
    };
    match result {
      Ok(stats) => {
        self.bound[index].stats = Some(stats);
        for hook in &mut self.hooks.after_present {
          hook(&self.tree, root, &stats);
        }
      }
      Err(error) => self.failed(cx, index, error),
    }
    let wants = self.hooks.wants_frame.iter_mut().fold(false, |wants, hook| hook() | wants);
    if wants {
      self.bound[index].needs_frame = true;
      cx.request_frame(surface);
    }
    self.settle(cx);
  }
}

impl<R> Debug for Runner<R> {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("Runner")
      .field("surfaces", &self.bound.len())
      .field("hooks", &self.hooks)
      .finish_non_exhaustive()
  }
}

/// Runs `runner` on `host`, after checking that its renderer can draw on what the host offers.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] before starting if the renderer can't draw on the host's
/// targets, and [`Error::Render`] if a frame failed, once a host whose `run` blocks until the loop
/// ends returns.
pub fn run<H: Host, R: Renderer + 'static>(host: H, runner: Runner<R>) -> Result<(), Error> {
  let kind = host.targets();
  if !runner.renderer.accepts(kind) {
    return Err(Error::Unsupported(kind));
  }
  let error = Rc::clone(&runner.error);
  host.run(runner);
  error.borrow_mut().take().map_or(Ok(()), Err)
}
