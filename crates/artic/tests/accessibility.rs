// What a screen reader gets from an app: the real controls in a window, run by a host whose screen
// reader reads it, over headless frames. Every update goes through accesskit_consumer, which panics
// on an update the OS adapters would reject.

use accesskit::{Node, Role, TreeUpdate};
use accesskit_consumer::{NodeRef, TreeChangeHandler};
use artic::controls::{Border, StackPanel, TextBlock};
use artic::element::{Application, Surface, Visibility};
use artic::prelude::*;
use artic::render::{
  Color, PhysicalSize, PixelBuffer, PixelFormat, RenderError, Renderer, Scene, SurfaceId, SurfaceView, Target, TargetKind,
};
use artic::{NodeId, Property};
use artic_platform::{
  Accessibility, AppHandler, DisplayId, FrameStats, HostContext, HostEvent, Runner, SurfaceInfo, Waker, WindowDesc, Windows,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// The one window's surface.
const WINDOW: SurfaceId = SurfaceId::new(1);

/// A host context with one window, which a screen reader reads once `reading` is set, keeping every
/// update it is handed.
#[derive(Default)]
struct Reader {
  reading: bool,
  updates: Vec<TreeUpdate>,
}

impl HostContext for Reader {
  fn request_frame(&mut self, _surface: SurfaceId) {}

  fn exit(&mut self) {}

  fn windows(&mut self) -> Option<&mut dyn Windows> {
    Some(self)
  }

  fn accessibility(&mut self) -> Option<&mut dyn Accessibility> {
    Some(self)
  }
}

impl Windows for Reader {
  fn open(&mut self, _desc: &WindowDesc) -> SurfaceId {
    WINDOW
  }

  fn update(&mut self, _surface: SurfaceId, _desc: &WindowDesc) {}

  fn close(&mut self, _surface: SurfaceId) {}
}

impl Accessibility for Reader {
  fn is_active(&self, surface: SurfaceId) -> bool {
    self.reading && surface == WINDOW
  }

  fn update(&mut self, _surface: SurfaceId, update: TreeUpdate) {
    self.updates.push(update);
  }
}

/// A renderer that draws nothing, on pixel buffers.
struct Blank;

impl Renderer for Blank {
  fn accepts(&self, kind: TargetKind) -> bool {
    kind == TargetKind::Pixels
  }

  fn render(&mut self, _scene: &Scene, _surface: SurfaceView<'_>) -> Result<(), RenderError> {
    Ok(())
  }

  fn surface_lost(&mut self, _surface: SurfaceId) {}
}

/// Takes the consumer's change notices and does nothing with them: the consumer's own checks are
/// what the tests want.
struct Ignore;

impl TreeChangeHandler for Ignore {
  fn node_added(&mut self, _node: &NodeRef) {}
  fn node_updated(&mut self, _old: &NodeRef, _new: &NodeRef) {}
  fn focus_moved(&mut self, _old: Option<&NodeRef>, _new: Option<&NodeRef>) {}
  fn node_removed(&mut self, _node: &NodeRef) {}
}

/// A change to the tree, made before the next frame's layout.
type Edit = Box<dyn FnOnce(&mut UiTree)>;

/// An app in a 200 by 200 window that a screen reader started reading, with the consumer that checks
/// what it is handed.
struct Screen {
  runner: Runner<Blank>,
  cx: Reader,
  edits: Rc<RefCell<Vec<Edit>>>,
  recorded: Rc<Cell<u32>>,
  oracle: Option<accesskit_consumer::Tree>,
}

impl Screen {
  /// Builds the app with `content` in a window titled "Hello", draws its first frame, then has a
  /// screen reader start reading it.
  fn new<T>(mut tree: UiTree, content: Handle<T>) -> Self {
    let surface = Surface::new().title("Hello").child(content).build(&mut tree);
    let application = Application::new().children([surface]).build(&mut tree);
    let mut runner = Runner::new(tree, Blank, Box::new(artic_text::Engine::without_fonts()), application.id());
    let edits = Rc::new(RefCell::new(Vec::<Edit>::new()));
    let pending = Rc::clone(&edits);
    runner.hooks().before_layout(move |tree, _, _, _| {
      for edit in pending.borrow_mut().drain(..) {
        edit(tree);
      }
    });
    let recorded = Rc::new(Cell::new(0));
    let count = Rc::clone(&recorded);
    runner
      .hooks()
      .after_present(move |_, _, stats: &FrameStats| count.set(stats.recorded));
    let mut cx = Reader::default();
    runner.started(&mut cx, Waker::new(|| {}));
    let info = SurfaceInfo {
      size: PhysicalSize { width: 200, height: 200 },
      scale: 1.0,
      display: DisplayId::new(1),
      refresh_rate: None,
    };
    runner.surface_changed(&mut cx, WINDOW, info);
    let mut screen = Screen {
      runner,
      cx,
      edits,
      recorded,
      oracle: None,
    };
    assert_eq!(screen.frame(), None, "no update before a screen reader reads");
    screen.cx.reading = true;
    screen
      .runner
      .event(&mut screen.cx, HostEvent::AccessibilityRequested { surface: WINDOW });
    screen
  }

  /// Changes the tree before the next frame's layout.
  fn edit(&mut self, edit: impl FnOnce(&mut UiTree) + 'static) {
    self.edits.borrow_mut().push(Box::new(edit));
  }

  /// Settles what changed, draws a frame, and returns the update it handed over, if any, after the
  /// consumer took it.
  fn frame(&mut self) -> Option<TreeUpdate> {
    self.runner.woken(&mut self.cx);
    let mut pixels = vec![0; 200 * 200 * 4];
    let target = Target::Pixels(PixelBuffer {
      data: &mut pixels,
      stride: 200 * 4,
      format: PixelFormat::Rgba8,
    });
    self.runner.frame(&mut self.cx, WINDOW, target, Duration::ZERO);
    let update = self.cx.updates.pop();
    assert!(self.cx.updates.is_empty(), "at most one update a frame");
    if let Some(update) = &update {
      match &mut self.oracle {
        None => self.oracle = Some(accesskit_consumer::Tree::new(update.clone(), true)),
        Some(oracle) => oracle.update_and_process_changes(update.clone(), &mut Ignore),
      }
    }
    update
  }
}

/// The accesskit id of `node`.
fn id(node: NodeId) -> accesskit::NodeId {
  accesskit::NodeId(node.to_bits())
}

/// The ids an update holds, in order.
fn sent(update: &TreeUpdate) -> Vec<accesskit::NodeId> {
  update.nodes.iter().map(|(id, _)| *id).collect()
}

/// What `update` holds for `node`.
fn node(update: &TreeUpdate, node: NodeId) -> &Node {
  let wanted = id(node);
  update
    .nodes
    .iter()
    .find_map(|(id, built)| (*id == wanted).then_some(built))
    .unwrap_or_else(|| panic!("no node {wanted:?} in the update"))
}

/// A `TextBlock` showing `text`, and its text property.
fn text_block(tree: &mut UiTree, text: &str) -> (Handle<TextBlock>, Property<String>) {
  let block = TextBlock::new().text(text).build(tree);
  (block, block.get(tree).text)
}

// The whole tree a screen reader gets first holds the window, the panel, the border and the text,
// each with the role and name its control gives it.
#[test]
fn initial_tree_update_covers_all_nodes() {
  let mut tree = UiTree::new();
  let (text, _) = text_block(&mut tree, "Hello world");
  let border = Border::new().child(text).build(&mut tree);
  let panel = StackPanel::new().children([border]).build(&mut tree);
  let mut screen = Screen::new(tree, panel);

  let update = screen.frame().expect("the whole tree");

  let root = update.tree.as_ref().expect("a full update").root;
  let window = update.nodes.iter().find(|(id, _)| *id == root).map(|(_, node)| node);
  assert_eq!(
    window.map(|window| (window.role(), window.label())),
    Some((Role::Window, Some("Hello")))
  );
  assert_eq!(node(&update, panel.id()).role(), Role::GenericContainer);
  assert_eq!(node(&update, border.id()).role(), Role::GenericContainer);
  let text = node(&update, text.id());
  assert_eq!((text.role(), text.value()), (Role::Label, Some("Hello world")));
  assert_eq!(update.nodes.len(), 4);
}

// A text change sends only that text, when it changes no other node's place or size: the text fills
// a border of fixed size.
#[test]
fn text_change_updates_only_that_node() {
  let mut tree = UiTree::new();
  let (text, content) = text_block(&mut tree, "Hello");
  let border = Border::new().width(120.0).height(40.0).child(text).build(&mut tree);
  let mut screen = Screen::new(tree, border);
  screen.frame();

  content.set("Goodbye".to_owned());

  let update = screen.frame().expect("the changed text");
  assert_eq!((update.tree.is_none(), sent(&update)), (true, vec![id(text.id())]));
  assert_eq!(node(&update, text.id()).value(), Some("Goodbye"));
}

// A frame that draws something no screen reader sees, a new background, hands over nothing.
#[test]
fn clean_frame_pushes_no_update() {
  let mut tree = UiTree::new();
  let border = Border::new().width(120.0).height(40.0).build(&mut tree);
  let background = border.get(&tree).background;
  let mut screen = Screen::new(tree, border);
  screen.frame();

  background.set(Some(Color::rgb(255, 0, 0).into()));

  assert_eq!(screen.frame(), None);
  assert!(screen.recorded.get() > 0, "the frame drew the new background");
}

// A StackPanel's children come in visual order, with a collapsed child left out.
#[test]
fn stack_panel_children_in_visual_order() {
  let mut tree = UiTree::new();
  let (first, _) = text_block(&mut tree, "first");
  let hidden = TextBlock::new()
    .text("hidden")
    .visibility(Visibility::Collapsed)
    .build(&mut tree);
  let (last, _) = text_block(&mut tree, "last");
  let panel = StackPanel::new().children([first, hidden, last]).build(&mut tree);
  let mut screen = Screen::new(tree, panel);

  let update = screen.frame().expect("the whole tree");

  assert_eq!(node(&update, panel.id()).children(), [id(first.id()), id(last.id())]);
  assert!(!sent(&update).contains(&id(hidden.id())));
}

// A freed node leaves its parent's child list, which is how a screen reader learns it is gone.
#[test]
fn freed_node_removed_from_tree() {
  let mut tree = UiTree::new();
  let (kept, _) = text_block(&mut tree, "kept");
  let (freed, _) = text_block(&mut tree, "freed");
  let panel = StackPanel::new().children([kept, freed]).build(&mut tree);
  let mut screen = Screen::new(tree, panel);
  screen.frame();

  screen.edit(move |tree| tree.free(freed.id()));

  let update = screen.frame().expect("the panel's new list");
  assert_eq!(node(&update, panel.id()).children(), [id(kept.id())]);
  assert!(!sent(&update).contains(&id(freed.id())));
}
