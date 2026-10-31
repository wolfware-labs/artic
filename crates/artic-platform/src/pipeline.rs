//! One surface's frames: layout, composition and presenting, without a host.

use crate::{Axes, SurfaceInfo};
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::{ElementNodes, ElementTypes};
use artic_layout::geometry::Size;
use artic_layout::text::TextEngine;
use artic_layout::{LayoutManager, PixelGrid};
use artic_render::{
  Compositor, Layers, Nodes, PhysicalSize, RenderError, Renderer, Scene, SceneBuilder, SurfaceId, SurfaceView, Target,
};
use std::collections::HashSet;
use std::fmt::{self, Debug, Formatter};

/// What one frame did, for tests and the debug overlay.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameStats {
  /// The nodes measured, rather than answered from layout's cache.
  pub measured: u32,
  /// The nodes arranged, rather than skipped as unchanged.
  pub arranged: u32,
  /// The nodes whose drawing was recorded again.
  pub recorded: u32,
  /// Whether the frame reached the renderer.
  pub rendered: bool,
}

/// A frame laid out and composed, waiting to be presented.
///
/// It owns its scene and borrows nothing from the tree, so it's `Send`: a render thread can take it
/// over without the API changing.
#[derive(Debug)]
pub struct Frame {
  surface: SurfaceId,
  size: PhysicalSize,
  scale: f32,
  scene: Scene,
  stats: FrameStats,
}

impl Frame {
  /// The composed scene.
  #[must_use]
  pub fn scene(&self) -> &Scene {
    &self.scene
  }

  /// What laying out and composing it did. `rendered` is false until it's presented.
  #[must_use]
  pub fn stats(&self) -> FrameStats {
    self.stats
  }
}

/// What a frame's layout and composition borrow from the runner: the table it finds a node's
/// element in, and the text engine layout shapes with. Every surface's pipeline borrows the same
/// ones, so elements resolve the same way and a font keeps one identity in every window.
pub struct FrameServices<'a> {
  /// The element table.
  pub types: &'a ElementTypes,
  /// The text engine.
  pub text: &'a mut dyn TextEngine,
}

impl Debug for FrameServices<'_> {
  /// Shows the element table; the engine needn't implement `Debug`.
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("FrameServices")
      .field("types", &self.types)
      .finish_non_exhaustive()
  }
}

/// Draws one surface's frames.
///
/// It keeps the surface's layout results and recorded drawings between frames, so each frame
/// redoes only what changed, and the last scene it presented, for a redraw the OS asks for. Each
/// surface has its own, since both caches drop what isn't under the root they last worked on.
#[derive(Debug, Default)]
pub struct Pipeline {
  layout: LayoutManager,
  compositor: Compositor,
  stale: HashSet<NodeId>,
  last: Option<Scene>,
}

impl Pipeline {
  /// A pipeline that hasn't drawn yet.
  #[must_use]
  pub fn new() -> Pipeline {
    Pipeline::default()
  }

  /// Lays out `root`'s visual subtree in the surface's logical size and composes it.
  ///
  /// Layout measures only what changed; composition records only render-dirty nodes, the nodes
  /// layout resized since they were last recorded, including while hidden, and those whose layout
  /// result was set or dropped. On the `content`
  /// axes, which the surface's content sizes, layout offers `∞`, so the root takes its desired
  /// size there. Layout shapes text with `services`' engine.
  pub fn prepare(
    &mut self,
    tree: &mut UiTree,
    root: NodeId,
    surface: SurfaceId,
    info: &SurfaceInfo,
    services: &mut FrameServices<'_>,
    content: Axes,
  ) -> Frame {
    let fit = |sizes: bool, device: u32| if sizes { f32::INFINITY } else { device as f32 / info.scale };
    let available = Size::new(fit(content.width, info.size.width), fit(content.height, info.size.height));
    let grid = PixelGrid {
      scale: info.scale,
      snap: true,
    };
    let layout = self
      .layout
      .run(tree, services.types, root, available, grid, &mut *services.text);
    self.stale.extend(layout.resized);
    self.stale.extend(layout.results);
    self.stale.retain(|node| tree.is_within(TreeKind::Visual, *node, root));
    let mut nodes = PipelineNodes {
      elements: ElementNodes {
        types: services.types,
        layout: &self.layout,
      },
      stale: &mut self.stale,
    };
    let scene = self.compositor.compose(tree, root, &mut nodes);
    Frame {
      surface,
      size: info.size,
      scale: info.scale,
      scene,
      stats: FrameStats {
        measured: layout.measured,
        arranged: layout.arranged,
        recorded: self.compositor.recorded(),
        rendered: false,
      },
    }
  }

  /// Measures `root`'s visual subtree in `available` without arranging it, as before its window
  /// opens or when its window's description is rebuilt, and returns the size it wants. The next
  /// [`prepare`](Pipeline::prepare) in the same available size answers from the cache.
  pub fn measure(&mut self, tree: &mut UiTree, root: NodeId, available: Size, services: &mut FrameServices<'_>) -> Size {
    self
      .layout
      .measure(tree, services.types, root, available, &mut *services.text)
  }

  /// Renders `frame` on `target` and, once it's drawn, keeps its scene for a later
  /// [`redraw`](Pipeline::redraw).
  ///
  /// # Errors
  ///
  /// Returns what the renderer returned when it fails.
  pub fn present(&mut self, renderer: &mut impl Renderer, frame: Frame, target: Target<'_>) -> Result<FrameStats, RenderError> {
    let Frame {
      surface,
      size,
      scale,
      scene,
      mut stats,
    } = frame;
    renderer.render(
      &scene,
      SurfaceView {
        id: surface,
        size,
        scale,
        target,
      },
    )?;
    self.last = Some(scene);
    stats.rendered = true;
    Ok(stats)
  }

  /// Renders the last presented scene again, for a redraw the OS asked for with nothing changed,
  /// or returns `None` if nothing was presented yet.
  ///
  /// # Errors
  ///
  /// The inner result is what the renderer returned.
  pub fn redraw(
    &mut self,
    renderer: &mut impl Renderer,
    surface: SurfaceId,
    info: &SurfaceInfo,
    target: Target<'_>,
  ) -> Option<Result<FrameStats, RenderError>> {
    let scene = self.last.as_ref()?;
    let surface = SurfaceView {
      id: surface,
      size: info.size,
      scale: info.scale,
      target,
    };
    Some(renderer.render(scene, surface).map(|()| FrameStats {
      rendered: true,
      ..FrameStats::default()
    }))
  }
}

/// Composes the elements as [`ElementNodes`] does, and records again the nodes layout resized
/// since their last recording.
struct PipelineNodes<'a> {
  elements: ElementNodes<'a>,
  stale: &'a mut HashSet<NodeId>,
}

impl Nodes for PipelineNodes<'_> {
  fn layers(&self, tree: &UiTree, node: NodeId) -> Layers {
    self.elements.layers(tree, node)
  }

  fn record(&mut self, tree: &UiTree, node: NodeId, builder: &mut SceneBuilder) {
    self.stale.remove(&node);
    self.elements.record(tree, node, builder);
  }

  fn stale(&self, node: NodeId) -> bool {
    self.stale.contains(&node)
  }
}
