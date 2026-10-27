// Elements, a renderer and a host context shared by the pipeline and runner tests.
#![allow(dead_code)]

use artic_core::Object;
use artic_core::property::{Affects, Inheritance, Property, PropertyKey, PropertyVisitor};
use artic_core::tree::{NodeId, TreeKind, UiTree};
use artic_element::{AccessCx, Application, Element, RenderCx, Surface};
use artic_layout::geometry::{Rect, Size, Transform};
use artic_layout::text::{FontFamily, GlyphRun, Line, Paragraph, ParagraphStyle, TextEngine, TextLimits, TextStyle};
use artic_layout::{ArrangeCx, Layoutable, MeasureCx};
use artic_platform::{DisplayId, HostContext, SurfaceInfo, WindowDesc, Windows};
use artic_render::{
  Color, Layers, PhysicalSize, PixelBuffer, PixelFormat, RenderError, Renderer, Scene, SceneBuilder, SurfaceId, SurfaceView,
  Target, TargetKind,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

static VISIBLE: PropertyKey<bool> = PropertyKey::new("Visible", Affects::RENDER, Inheritance::None);
static HEIGHT: PropertyKey<f32> = PropertyKey::new("Height", Affects::LAYOUT, Inheritance::None);
static COLOR: PropertyKey<u32> = PropertyKey::new("Color", Affects::RENDER, Inheritance::None);
static TURN: PropertyKey<bool> = PropertyKey::new("Turn", Affects::RENDER, Inheritance::None);
static TEXT: PropertyKey<String> = PropertyKey::new("Text", Affects::LAYOUT, Inheritance::None);

/// Stacks its children from top to bottom, each as wide as itself; draws nothing.
#[derive(Debug)]
pub struct Column {
  visible: Property<bool>,
}

impl Object for Column {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.visible);
  }
}

impl Layoutable for Column {
  fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    let children: Vec<NodeId> = cx.children().collect();
    children.iter().fold(Size::ZERO, |total, child| {
      let size = cx.measure(*child, Size::new(available.width, f32::INFINITY));
      Size::new(total.width.max(size.width), total.height + size.height)
    })
  }

  fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect) {
    let children: Vec<NodeId> = cx.children().collect();
    let mut y = 0.0;
    for child in children {
      let height = cx.desired(child).height;
      cx.arrange(child, Rect::new(0.0, y, rect.width, height));
      y += height;
    }
  }
}

impl Element for Column {
  fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {}

  fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::GenericContainer)
  }

  fn layers(&self, _size: Size) -> Layers {
    Layers {
      visible: self.visible.get(),
      ..Layers::default()
    }
  }
}

artic_element::element_type!(Column);

/// A block `height` tall that fills its box with `color`, turned by a scale of 2 when `turn` is set.
#[derive(Debug)]
pub struct Leaf {
  height: Property<f32>,
  color: Property<u32>,
  turn: Property<bool>,
  pub seen: Cell<Option<Size>>,
  pub collapsed: Cell<bool>,
}

impl Object for Leaf {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.height);
    visitor.visit(&self.color);
    visitor.visit(&self.turn);
  }
}

impl Layoutable for Leaf {
  fn measure(&self, _cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    Size::new(10.0, self.height.get())
  }

  fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}

  fn collapsed(&self) -> bool {
    self.collapsed.get()
  }
}

impl Element for Leaf {
  fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder) {
    self.seen.set(Some(cx.size()));
    let color = self.color.get();
    let [_, r, g, b] = color.to_be_bytes();
    scene.fill(Rect::new(0.0, 0.0, cx.size().width, cx.size().height), Color::rgb(r, g, b));
  }

  fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::GenericContainer)
  }

  fn layers(&self, _size: Size) -> Layers {
    Layers {
      transform: self.turn.get().then(|| Transform::scale(2.0, 2.0)),
      ..Layers::default()
    }
  }
}

artic_element::element_type!(Leaf);

/// Text-like content of a fixed area: as wide as offered, up to 100, and as tall as its area needs
/// at that width.
#[derive(Debug)]
pub struct Wrap;

impl Object for Wrap {
  fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
}

impl Layoutable for Wrap {
  fn measure(&self, _cx: &mut MeasureCx<'_>, available: Size) -> Size {
    let width = available.width.min(100.0);
    Size::new(width, 1000.0 / width)
  }

  fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
}

impl Element for Wrap {
  fn render(&self, _cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {}

  fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::GenericContainer)
  }
}

artic_element::element_type!(Wrap);

/// A leaf's id and the properties a test changes.
#[derive(Debug, Clone, Copy)]
pub struct LeafHandles {
  pub id: NodeId,
  pub height: Property<f32>,
  pub color: Property<u32>,
  pub turn: Property<bool>,
}

/// Allocates a visible panel holding `children`, and returns it with its visibility.
pub fn column(tree: &mut UiTree, children: &[NodeId]) -> (NodeId, Property<bool>) {
  let visible = Property::new(&VISIBLE, true);
  let id = tree.alloc(Box::new(Column { visible }));
  for child in children {
    tree.add_child(TreeKind::Visual, id, *child);
  }
  (id, visible)
}

/// Allocates a black leaf `height` tall.
pub fn leaf(tree: &mut UiTree, height: f32) -> LeafHandles {
  let (height, color, turn) = (
    Property::new(&HEIGHT, height),
    Property::new(&COLOR, 0),
    Property::new(&TURN, false),
  );
  let id = tree.alloc(Box::new(Leaf {
    height,
    color,
    turn,
    seen: Cell::new(None),
    collapsed: Cell::new(false),
  }));
  LeafHandles { id, height, color, turn }
}

/// A renderer for pixel buffers that counts renders and lost surfaces, and fails once on demand.
#[derive(Debug, Clone, Default)]
pub struct Counter {
  pub rendered: Rc<Cell<u32>>,
  pub lost: Rc<RefCell<Vec<SurfaceId>>>,
  pub fail: Rc<RefCell<Option<RenderError>>>,
}

impl Renderer for Counter {
  fn accepts(&self, kind: TargetKind) -> bool {
    kind == TargetKind::Pixels
  }

  fn render(&mut self, _scene: &Scene, _surface: SurfaceView<'_>) -> Result<(), RenderError> {
    if let Some(error) = self.fail.borrow_mut().take() {
      return Err(error);
    }
    self.rendered.set(self.rendered.get() + 1);
    Ok(())
  }

  fn surface_lost(&mut self, surface: SurfaceId) {
    self.lost.borrow_mut().push(surface);
  }
}

/// A host context that logs what the runner asks for and opens windows numbered from 1.
#[derive(Debug, Default)]
pub struct Context {
  pub frames: Vec<SurfaceId>,
  pub exited: bool,
  pub opened: Vec<WindowDesc>,
  pub updated: Vec<(SurfaceId, WindowDesc)>,
  pub closed: Vec<SurfaceId>,
  pub windowless: bool,
}

impl Context {
  /// Forgets the frames asked for so far.
  pub fn clear(&mut self) {
    self.frames.clear();
  }
}

impl HostContext for Context {
  fn request_frame(&mut self, surface: SurfaceId) {
    self.frames.push(surface);
  }

  fn exit(&mut self) {
    self.exited = true;
  }

  fn windows(&mut self) -> Option<&mut dyn Windows> {
    if self.windowless { None } else { Some(self) }
  }
}

impl Windows for Context {
  fn open(&mut self, desc: &WindowDesc) -> SurfaceId {
    self.opened.push(desc.clone());
    SurfaceId::new(self.opened.len() as u64)
  }

  fn update(&mut self, surface: SurfaceId, desc: &WindowDesc) {
    self.updated.push((surface, desc.clone()));
  }

  fn close(&mut self, surface: SurfaceId) {
    self.closed.push(surface);
  }
}

/// Allocates an Application holding one Surface per `(title, content)`, each showing its content,
/// and returns the application and the surfaces in order.
pub fn application(tree: &mut UiTree, surfaces: &[(&str, NodeId)]) -> (NodeId, Vec<NodeId>) {
  let application = tree.alloc(Box::new(Application::default()));
  let nodes = surfaces
    .iter()
    .map(|(title, content)| {
      let surface = Surface::default();
      surface.title.set((*title).to_owned());
      let node = tree.alloc(Box::new(surface));
      for kind in [TreeKind::Logical, TreeKind::Visual] {
        tree.add_child(kind, application, node);
        tree.add_child(kind, node, *content);
      }
      node
    })
    .collect();
  (application, nodes)
}

/// The Surface element of `node`.
pub fn surface(tree: &UiTree, node: NodeId) -> &Surface {
  tree.get_as::<Surface>(node).expect("a Surface")
}

/// A surface `width` by `height` device pixels at `scale`.
pub fn info(width: u32, height: u32, scale: f32) -> SurfaceInfo {
  SurfaceInfo {
    size: PhysicalSize { width, height },
    scale,
    display: DisplayId::new(1),
    refresh_rate: Some(60.0),
  }
}

/// A 64 by 64 RGBA buffer.
pub fn pixels() -> Vec<u8> {
  vec![0; 64 * 64 * 4]
}

/// `pixels` as a frame target.
pub fn target(pixels: &mut [u8]) -> Target<'_> {
  Target::Pixels(PixelBuffer {
    data: pixels,
    stride: 64 * 4,
    format: PixelFormat::Rgba8,
  })
}

/// Sets `property`, for hooks that capture a property handle.
pub fn set_color(property: Property<u32>, color: u32) {
  property.set(color);
}

/// The text engine the pipeline and runner tests shape with: only the test font.
pub fn engine() -> artic_text::Engine {
  artic_text::Engine::with_fonts([Arc::new(artic_test_fonts::REGULAR) as Arc<dyn AsRef<[u8]> + Send + Sync>])
}

/// A text engine that shares, with every clone, the list of text it was asked to shape, and
/// answers with an empty paragraph 8 wide per character and 16 tall.
#[derive(Debug, Clone, Default)]
pub struct Recorder {
  pub asked: Rc<RefCell<Vec<String>>>,
}

impl TextEngine for Recorder {
  fn paragraph(&mut self, text: &str, _style: &TextStyle, _paragraph: &ParagraphStyle, _limits: TextLimits) -> Paragraph {
    self.asked.borrow_mut().push(text.to_owned());
    let size = Size::new(8.0 * text.chars().count() as f32, 16.0);
    Paragraph::new(size, Vec::<Line>::new(), Vec::<GlyphRun>::new())
  }
}

/// A line of text, as wide and tall as the paragraph the engine makes of it, which it keeps as its
/// layout result and notes the size of when it draws.
#[derive(Debug)]
pub struct Label {
  text: Property<String>,
  pub drawn: Cell<Option<Size>>,
}

impl Object for Label {
  fn visit_properties(&self, visitor: &mut PropertyVisitor<'_>) {
    visitor.visit(&self.text);
  }
}

impl Layoutable for Label {
  fn measure(&self, cx: &mut MeasureCx<'_>, _available: Size) -> Size {
    let style = TextStyle::new(FontFamily::default(), 16.0);
    let paragraph = cx
      .text()
      .paragraph(&self.text.get(), &style, &ParagraphStyle::default(), TextLimits::NONE);
    let size = paragraph.size();
    cx.set_measured(paragraph);
    size
  }

  fn arrange(&self, _cx: &mut ArrangeCx<'_>, _rect: Rect) {}
}

impl Element for Label {
  fn render(&self, cx: &mut RenderCx<'_>, _scene: &mut SceneBuilder) {
    self.drawn.set(cx.layout_result::<Paragraph>().map(Paragraph::size));
  }

  fn accessibility(&self, _cx: &AccessCx<'_>) -> accesskit::Node {
    accesskit::Node::new(accesskit::Role::Label)
  }
}

artic_element::element_type!(Label);

/// Allocates a label showing `text`, and returns it with its text.
pub fn label(tree: &mut UiTree, text: &str) -> (NodeId, Property<String>) {
  let text = Property::new(&TEXT, text.to_owned());
  let id = tree.alloc(Box::new(Label {
    text,
    drawn: Cell::new(None),
  }));
  (id, text)
}
