# Design: accessibility

Issue: #54. Builds on: the [host seam](host.md) (#39), the [frame pipeline](frame.md) (#40), `Element::accessibility` and the property groups ([element design](element.md), #42), the M1 controls (#44, #45, #46, #48). Unblocks: #101 (`TestHost`), #102 (accesskit actions), #78 (focus), #159 and #160 (mobile adapters), #161 (`Accessibility.*` attached properties).

A screen reader reads an app through the OS accessibility API: UI Automation on Windows, NSAccessibility on macOS, AT-SPI on Linux. `accesskit` puts one Rust API in front of all three. An app hands it a tree of nodes (a role, a name, a value, bounds, children), and afterwards only the nodes that changed. This document fixes how artic builds that tree from the `UiTree`, how it reaches the host, and how `artic-winit` connects it to `accesskit_winit`.

## 1. Who sees what

| Who | Writes | Sees |
| --- | --- | --- |
| App authors | nothing: every app is readable by a screen reader on a host that supports it | |
| Control authors | `ElementImpl::accessibility(&self, cx, node)`, adding the role, name and value to the default node `Interaction::accessibility` builds ([element design](element.md)) | `AccessCx`, `accesskit::Node` |
| Host adapter authors | `HostContext::accessibility`, returning their per-window `Accessibility` | `Accessibility`, `accesskit::TreeUpdate` |
| artic (`artic-platform`) | the accessibility pass in `Pipeline` | the `UiTree`, the surface's layout, the host's `Accessibility` |

## 2. Node ids

`accesskit` names each node with one `u64`. The id must stay the same for a node's whole life and must differ for a new node that reuses a freed arena slot, or a screen reader would read the new node as the old one.

artic's `NodeId` is an arena slot index and the slot's generation, two `u32`s. `artic-core` adds:

```rust,ignore
impl NodeId {
  /// The id as one number: the generation in the high 32 bits, the slot index in the low 32. Stable
  /// for the node's life; a node that later reuses the slot has another generation, so its bits
  /// differ from the old node's.
  pub fn to_bits(self) -> u64;
}
```

The accesskit id of a node is `accesskit::NodeId(node.to_bits())`. No side table maps one to the other. A slot's generation wraps after 2^32 − 1 reuses of that one slot, which no app reaches.

## 3. The tree

Each surface is its own accesskit tree, one per window, and the surface's node is its root. `Surface` already describes itself as `Role::Window` labeled with its `Title` (#48).

A node is in the tree when it is visible: when `ElementNodes::layers` says `visible`, that is, layout placed it and its `Visibility` is `Visible`. A `Hidden` or `Collapsed` node and its whole subtree are left out, as the [element design](element.md) section 7 requires, except the surface node: the tree's root stays, with no children (section 4). A node's accesskit children are its visible visual children, in visual order (`UiTree::children(TreeKind::Visual, node)`).

A visual node that is not an element gets default layers from the [frame design](frame.md) and records nothing; it gets a `Role::GenericContainer` node with its size as bounds and, like every node, the transform `ElementNodes::layers` gives it, here its layout offset alone, so its descendants stay where they are drawn.

A node describes only itself. `ElementImpl::accessibility` may read the node's own properties and nothing else through `AccessCx::tree()`, because the cache in section 4 notices changes of the node, not of its neighbors. A name made from a child's text is left to accesskit, which computes it from the children; `Accessibility.LabeledBy` (#161) names another node by id and follows the same rule.

accesskit measures a node's `bounds` in the space that the node's own `transform` and its ancestors' transforms combine to; `bounds` itself moves nothing, and with no transform anywhere every node's bounds are in the window's space. So a node is placed by its transform, and its bounds start at its own origin:

- The pass sets each node's `transform` from `ElementNodes::layers`, the transform the compositor draws the node with: its `RenderTransform`, then the offset layout placed it at. It sets none when that is the identity, as accesskit asks. A rotated or scaled control is reported where it is drawn, the geometry is computed in one place, and a change of `RenderTransform`, which affects render, describes the node again.
- `Interaction::accessibility` sets `bounds` to the node's size at `(0, 0)`. Today it puts the layout rect, in the parent's coordinates, in `bounds`, which places every nested node wrong; this design fixes that.
- The surface node is also scaled: its transform is its own, then `Affine::scale(info.scale)`, since layout works in logical units and accesskit wants physical pixels from the window's origin. A window moved to a display with another scale can keep its logical rect, so the pass also keeps the scale it built the surface node with and describes that node again when `info.scale` changes.

A node that layout only moved changes only its own transform, so it alone goes into the update; its descendants' nodes stay the same.

Until focus lands (#78), every update names the root as focused.

## 4. The pass

The pass lives in `artic-platform`, in `Pipeline`, and runs in `prepare` between layout and composition. Two reasons put it there and not in a `Hooks` closure: the pass needs the surface's `LayoutManager` for bounds and visibility, which `Pipeline` owns, and it needs the render-dirty flags and the pipeline's stale set, which composition clears while it records.

Per surface, the pass keeps a cache: for each node it described last time, the accesskit node it sent, children included, and the rect it was built with. `accesskit::Node` is `Clone` and `PartialEq`, so the pass compares what it builds with what it sent.

```rust,ignore
struct Described {
  node: accesskit::Node,
  rect: Option<Rect>,
}

pub struct AccessTree {
  described: HashMap<NodeId, Described>,
  scale: f32,
  full: bool,
}
```

The rect is `None` for a node that layout didn't place, a collapsed surface. `AccessTree` is public, as `artic_platform::AccessTree`, so a host outside the crate, such as the `TestHost` of #101, builds a surface's tree the way `Pipeline` does.

Each frame the pass walks the surface's visible subtree from the root. The surface node is always described, visible or not: an accesskit tree must contain its root, so a `Hidden` or `Collapsed` surface is a `Window` with no children, at zero size when it is `Collapsed`, since layout gives a collapsed root no rectangle. For the surface node and each visible node below it:

1. It collects the ids of the node's visible children, none for a surface that is not visible.
2. If the node may describe itself differently, it calls `Element::accessibility`, sets the children and the transform, and compares the new node with the cached one. That is when the cache has no entry for it, the compositor records it again this frame (it is render-dirty, or in the pipeline's stale set: the nodes layout resized or gave a new result), its layout rect differs from the cached one, or it is the surface node and the scale changed.
3. Otherwise, if the cached node lists the same children, the node is unchanged and costs nothing more: no call, no clone. If its children differ, the pass clones the cached node and sets the new children.
4. A node that came out different from the cached one replaces it and goes into the update.

After the walk, entries the walk did not reach are dropped. That handles every way a node leaves the tree without a signal of its own: a freed node, a node detached from the surface, a node that became hidden. Each of those also changes its parent's child list, so the parent goes into the update, and accesskit drops a node no parent lists. A node that becomes visible again has no cache entry, so its whole subtree is described again.

The pass reads the render-dirty flags and the stale set and changes neither; the compositor still clears them while it records. It needs no new signal from `artic-core` or `artic-layout`: no render queue, no list of moved nodes, no list of freed nodes.

So a node is described again whenever the compositor records it again, and also when it only moved. A property that `ElementImpl::accessibility` reads must therefore affect render, or affect layout on an element whose measure always sets a layout result. Every property it reads today does: `IsEnabled` and `Surface.Title` affect render, and `TextBlock.Text` affects layout on an element whose `measure_override` always calls `set_measured`, so a new text gives the node a new layout result. Layout alone is not enough on an element that keeps no layout result and keeps its size: layout then reports nothing about it. #161's attached properties affect render.

The pass runs only in `prepare`, and the runner calls `prepare` only for a surface that needs a frame. An idle app runs no pass at all. A frame where layout or a property changed something no screen reader sees, such as a background color, walks the tree and pushes nothing, so `clean_frame_pushes_no_update` holds.

A full update clears the cache first and walks the same way, so every visible node goes in, with `TreeUpdate::tree` set to the surface's root. An incremental update sets `tree` to `None`. Every update sets `tree_id` to `TreeId::ROOT` and `focus` to the root.

A new `AccessTree` starts with `full` set, and `full` is cleared only when an update is handed to the host, not when a frame runs. So the first update a pipeline sends is always full, also for a surface whose screen reader was already reading, such as a spare surface a new `Surface` node takes over.

## 5. The host seam

`HostContext` gains the capability the [backend design](backends.md) lists, defaulting to `None` like the others:

```rust,ignore
pub trait HostContext {
  // …
  /// The host's connection to the OS accessibility API, if it has one.
  fn accessibility(&mut self) -> Option<&mut dyn Accessibility> { None }
}

pub trait Accessibility {
  /// Whether a screen reader is reading the surface, counted from the `AccessibilityRequested`
  /// the host reported for it. While it isn't, artic builds nothing.
  fn is_active(&self, surface: SurfaceId) -> bool;
  /// Hands an update to the OS.
  fn update(&mut self, surface: SurfaceId, update: accesskit::TreeUpdate);
}

pub enum HostEvent {
  // …
  /// A screen reader started reading the surface. The next frame sends it the whole tree.
  AccessibilityRequested {
    /// The window's surface.
    surface: SurfaceId,
  },
}
```

A screen reader that starts reading is an event like any other the host reports, so it arrives as a `HostEvent`, the [frame design](frame.md)'s rule for features that react to the host. A wake would not do: it only settles posted work, and a surface with nothing to draw gets no `prepare`, so the pass would never run in an idle app. On `AccessibilityRequested`, the runner marks the surface as needing a frame, requests one, and tells its `Pipeline` that the next update is full.

A host counts a surface active only from the moment it reports `AccessibilityRequested`, not from the moment the screen reader starts. accesskit takes the first update after a start as the whole tree and panics on one without `tree`, and a frame can run between the start and the event; an incremental update in that frame would be the first.

`Runner` lends the capability to `Pipeline::prepare` in `FrameServices`, with the element types and the text engine. The pass:

- does nothing when there is no capability or the surface is not active;
- builds a full update after `AccessibilityRequested`;
- otherwise builds an incremental one, and calls `update` only when it holds at least one node.

The cache is valid only while a screen reader reads the surface. Whenever one starts, the host reports `AccessibilityRequested` before it counts the surface active, so the first update is full and the frames in between need no bookkeeping.

A surface lent to a `Surface` node and given back to the spare list keeps the last tree it was sent until a new `Surface` takes it. Only hosts that lend surfaces, the mobile ones (#159, #160), meet this, and they decide what a spare surface tells a screen reader.

`artic-platform` gains `accesskit` as a dependency. It is a description format, not a backend: it draws nothing and talks to no OS, so the crate still has no backend dependencies.

## 6. `artic-winit`

`artic-winit` depends on `accesskit_winit` and keeps one `accesskit_winit::Adapter` per window.

- **Creating the window.** `accesskit_winit` needs its adapter made before the window is first shown. `Windows::open` creates windows through the driver's `Loop::create_window`, so `ActiveEventLoop`'s implementation (`host.rs`) creates the window invisible (`attributes` with `with_visible(false)`), makes the adapter, then shows the window, and the fake `Loop` records the same order.
- **Handlers.** The adapter is built with direct handlers. Each window has one atomic state, `Inactive`, `Requested` or `Active`. The activation handler returns `None`, so accesskit waits for the tree, stores `Requested`, and wakes the loop with a clone of the host's `Waker`, which the driver keeps since it hands its own to `started`. Woken, the driver reports `AccessibilityRequested` for each `Requested` window, then moves it to `Active` with a compare-exchange from `Requested`, and calls `woken`. The deactivation handler stores `Inactive`. A deactivation that lands between the report and the move leaves the window `Inactive`, where one flag per state would have let the loop mark it active again. The action handler ignores every action until #102.
- **Events.** `Driver::handle` passes each `WindowEvent` of a known window to its adapter's `process_event` before translating it, through a method on the driver's `Window` trait, so the fake window records what reached it.
- **Threads.** The handlers may run on another thread, so the flags they share with the loop are atomics. The tree itself is built only on the loop's thread, in the frame.
- **Checking again on hand-over.** A screen reader can stop and start again between the pass's `is_active` and its `update`, which leaves the adapter waiting for a whole tree while the pass hands it an incremental one, and accesskit panics on that. `update` calls the adapter's `update_if_active`, whose closure runs under the same lock as the activation handler, and checks the window's state again inside it. When the window is no longer `Active` and the update has no `tree`, it hands over a tree of the root alone instead: a `Window` with the root id the adapter kept from the last update it handed over that had `tree` set; the full update after the new `AccessibilityRequested` replaces it.

`accesskit_winit` 0.34 is the line built on `accesskit` 0.25, the version the workspace uses; 0.33 would bring a second `accesskit`. `deny.toml` lets `winit` in only through wrappers; `accesskit_winit` joins `artic-winit` in that list.

## 7. Testing

| Test | Where | What it shows |
| --- | --- | --- |
| `to_bits` round trips, reused slot gives other bits | `artic-core` | section 2 |
| walk: hidden subtree left out, and shown again with its unchanged descendants; visual order and a reorder; a node moved between parents; cache reuse; described again on render-dirty, a layout result with the same rect, a new rect or a new scale; freed and hidden nodes leave their parent's list; a non-element node as a container; a hidden surface as a `Window` with no children, and a collapsed one at zero size; full update clears the cache; `RenderTransform` placed in the node's transform; a new pipeline's first update is full; `AccessibilityRequested` gives an idle surface a frame with a full update | `artic-platform`, with its hand-written test elements | sections 3 to 5 |
| `initial_tree_update_covers_all_nodes`, `text_change_updates_only_that_node`, `clean_frame_pushes_no_update`, `stack_panel_children_in_visual_order`, `freed_node_removed_from_tree` | `artic` (the facade crate's tests), with the real `Surface`, `Border`, `TextBlock` and `StackPanel`, a fake host whose `Accessibility` records updates, and headless frames | #54's acceptance criteria |
| adapter made before the window shows, events reach the adapter, activation reports `AccessibilityRequested`, a window is not active before that report, a deactivation between the report and the move leaves it inactive, a stop and start during a frame turns an incremental update into the root alone, and a later activation is reported again | `artic-winit`, with its fake `Loop` and `Window` | section 6 |

The facade crate already depends on every layer, so these tests add no dependency edge and no exception to the [layering rules](../architecture.md#rules).

- `text_change_updates_only_that_node` keeps the `TextBlock`'s own rect the same, in a fixed-size parent with the default `Stretch`, so the new text resizes nothing and the test shows the node is described again because layout gave it a new result, not because its rect changed.
- `clean_frame_pushes_no_update` changes something no screen reader sees, a `Border`'s background, so the frame runs `prepare` and the pass, and the pass pushes nothing. A frame with nothing to do would skip the pass and prove nothing.
- The platform and facade tests apply every update they record to an `accesskit_consumer::Tree`, a dev-dependency. It checks what the OS adapters check and panics on the same invalid updates: a first update without `tree`, a full update without its root, a new node no parent lists, a child listed twice.

The pull request that adds the adapter records a manual check with NVDA on Windows, VoiceOver on macOS and Orca on Linux reading the hello example.

## 8. Pull requests

1. This document, and the documents it changes: the [frame design](frame.md) (the pass is a `Pipeline` stage in section 4's `prepare`, not a `Hooks` closure, and section 6's runner handles `AccessibilityRequested`), the [host design](host.md) (the `accessibility` capability and `AccessibilityRequested`), [technical spec](../spec/technical_spec.md) section 2 (the accesskit update runs between layout and scene build) and section 12 (child lists compared, not only render-dirty and moved nodes), the [element design](element.md) section 7 and the [architecture](../architecture.md) rule for the frame pipeline.
2. `NodeId::to_bits`, and the pass in `artic-platform` with its unit tests.
3. The `Accessibility` capability, `HostEvent::AccessibilityRequested`, the pass wired into `Runner` and `Pipeline`, and the facade's acceptance tests.
4. `accesskit_winit` in `artic-winit`, and the manual screen reader checks. Closes #54.
