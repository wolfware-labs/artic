# Design: accessibility

Issue: #54. Builds on: the [host seam](host.md) (#39), the [frame pipeline](frame.md) (#40), `Element::accessibility` and the property groups ([element design](element.md), #42), the M1 controls (#44, #45, #46, #48). Unblocks: #101 (`TestHost`), #102 (accesskit actions), #78 (focus), #159 and #160 (mobile adapters), #161 (`Accessibility.*` attached properties).

A screen reader reads an app through the OS accessibility API: UI Automation on Windows, NSAccessibility on macOS, AT-SPI on Linux. `accesskit` puts one Rust API in front of all three. An app hands it a tree of nodes (a role, a name, a value, bounds, children), and afterwards only the nodes that changed. This document fixes how artic builds that tree from the `UiTree`, how it reaches the host, and how `artic-winit` connects it to `accesskit_winit`.

## 1. Who sees what

| Who | Writes | Sees |
| --- | --- | --- |
| App authors | nothing: every app is readable by a screen reader on a host that supports it | |
| Control authors | `ElementImpl::accessibility(&self, cx, node)`, filling in the role, name and value of the node `Interaction` started ([element design](element.md)) | `AccessCx`, `accesskit::Node` |
| Host adapter authors | `HostContext::accessibility`, returning their per-window `Accessibility` | `Accessibility`, `accesskit::TreeUpdate` |
| artic (`artic-platform`) | the accessibility pass in `Pipeline` | the `UiTree`, the surface's layout, the host's `Accessibility` |

## 2. Node ids

`accesskit` names each node with one `u64`. The id must stay the same for a node's whole life and must differ for a new node that reuses a freed arena slot, or a screen reader would read the new node as the old one.

artic's `NodeId` is an arena slot index and the slot's generation, two `u32`s. `artic-core` adds:

```rust,ignore
impl NodeId {
  /// The id as one number: the generation in the high 32 bits, the slot index in the low 32. Stable
  /// for the node's life; a node that later reuses the slot has another generation, so other bits.
  pub fn to_bits(self) -> u64;
}
```

The accesskit id of a node is `accesskit::NodeId(node.to_bits())`. No side table maps one to the other. A slot's generation wraps after 2^32 reuses of that one slot, which no app reaches.

## 3. The tree

Each surface is its own accesskit tree, one per window, and the surface's node is its root. `Surface` already describes itself as `Role::Window` labeled with its `Title` (#48).

A node is in the tree when it is visible: when `ElementNodes::layers` says `visible`, that is, layout placed it and its `Visibility` is `Visible`. A `Hidden` or `Collapsed` node and its whole subtree are left out, as the [element design](element.md) section 7 requires. A node's accesskit children are its visible visual children, in visual order (`UiTree::children(TreeKind::Visual, node)`).

accesskit measures a node's `bounds` in the space of its nearest ancestor that has a `transform`; `bounds` itself moves nothing. So a node is placed by its transform, and its bounds start at its own origin:

- `Interaction::accessibility` turns `AccessCx::bounds`, the layout rect in the parent's coordinates, into a translation by the rect's origin (left out when it is zero, as accesskit asks for identity transforms) and bounds of the rect's size at `(0, 0)`. Today it puts the parent-relative rect in `bounds`, which places every nested node wrong; this design fixes that.
- The surface node is scaled instead: `Affine::scale(info.scale)`, since layout works in logical units and accesskit wants physical pixels from the window's origin.

A node that layout only moved changes only its own transform, so it alone goes into the update; its descendants' nodes stay the same.

Until focus lands (#78), every update names the root as focused.

## 4. The pass

The pass lives in `artic-platform`, in `Pipeline`, and runs in `prepare` between layout and composition. Two reasons put it there and not in a `Hooks` closure: the pass needs the surface's `LayoutManager` for bounds and visibility, which `Pipeline` owns, and it needs the render-dirty flags, which the compositor clears while it records.

Per surface, the pass keeps a cache: for each node it described last time, the accesskit node it sent, children included, and the rect it was built with. `accesskit::Node` is `Clone` and `PartialEq`, so the pass compares what it builds with what it sent.

```rust,ignore
struct Described {
  node: accesskit::Node,
  rect: Rect,
}

pub(crate) struct AccessTree {
  described: HashMap<NodeId, Described>,
}
```

Each frame the pass walks the surface's visible subtree from the root. For each visible node:

1. It collects the ids of the node's visible children.
2. If the cache has no entry for it, or it is render-dirty, or its layout rect differs from the cached one, it calls `Element::accessibility`, sets the children, and compares the new node with the cached one.
3. Otherwise, if the cached node lists the same children, the node is unchanged and costs nothing more: no call, no clone. If its children differ, the pass clones the cached node and sets the new children.
4. A node that came out different from the cached one replaces it and goes into the update.

After the walk, entries the walk did not reach are dropped. That handles every way a node leaves the tree without a signal of its own: a freed node, a node detached from the surface, a node that became hidden. Each of those also changes its parent's child list, so the parent goes into the update, and accesskit drops a node no parent lists. A node that becomes visible again has no cache entry, so its whole subtree is described again.

The pass reads the render-dirty flags and never clears them; the compositor still clears them while it records. It needs no new signal from `artic-core` or `artic-layout`: no render queue, no list of moved nodes, no list of freed nodes.

The pass runs only in `prepare`, and the runner calls `prepare` only for a surface that needs a frame. An idle app runs no pass at all. A frame where layout or a property changed something no screen reader sees, such as a background color, walks the tree and pushes nothing, so `clean_frame_pushes_no_update` holds.

A full update clears the cache first and walks the same way, so every visible node goes in, with `TreeUpdate::tree` set to the surface's root. An incremental update sets `tree` to `None`.

## 5. The host seam

`HostContext` gains the capability the [backend design](backends.md) lists, defaulting to `None` like the others:

```rust,ignore
pub trait HostContext {
  // …
  /// The host's connection to the OS accessibility API, if it has one.
  fn accessibility(&mut self) -> Option<&mut dyn Accessibility> { None }
}

pub trait Accessibility {
  /// Whether a screen reader is reading the surface. While it isn't, artic builds nothing.
  fn is_active(&self, surface: SurfaceId) -> bool;
  /// Whether the next update must be the whole tree: true once after a screen reader started
  /// reading the surface, and false after that until it starts again.
  fn take_full_request(&mut self, surface: SurfaceId) -> bool;
  /// Hands an update to the OS.
  fn update(&mut self, surface: SurfaceId, update: accesskit::TreeUpdate);
}
```

`Runner` passes the capability to `Pipeline::prepare` with the frame. The pass:

- does nothing when there is no capability or the surface is not active;
- builds a full update when `take_full_request` says so;
- otherwise builds an incremental one, and calls `update` only when it holds at least one node.

The cache is valid only while a screen reader reads the surface. Whenever one starts, the host asks for a full tree, so the frames in between need no bookkeeping.

`artic-platform` gains `accesskit` as a dependency. It is a description format, not a backend: it draws nothing and talks to no OS, so the crate still has no backend dependencies.

## 6. `artic-winit`

`artic-winit` depends on `accesskit_winit` and keeps one `accesskit_winit::Adapter` per window.

- **Creating the window.** `accesskit_winit` needs its adapter made before the window is first shown. `Windows::open` creates the window invisible (`with_visible(false)`), makes the adapter, then shows the window.
- **Handlers.** The adapter is built with direct handlers. The activation handler returns `None`, so accesskit waits for the tree, then sets the window's full-request flag and wakes the loop with the host's `Waker`, so the next frame sends the full tree. The deactivation handler marks the window inactive. The action handler ignores every action until #102.
- **Events.** `Driver::handle` passes each `WindowEvent` of a known window to its adapter's `process_event` before translating it.
- **Threads.** The handlers may run on another thread, so the flags they share with the loop are atomics. The tree itself is built only on the loop's thread, in the frame.

`deny.toml` lets `winit` in only through wrappers; `accesskit_winit` joins `artic-winit` in that list.

## 7. Tests

| Test | Where | What it shows |
| --- | --- | --- |
| `to_bits` round trips, reused slot gives other bits | `artic-core` | section 2 |
| walk: hidden subtree left out, visual order, cache reuse, re-described on render-dirty or new rect, freed and hidden nodes leave their parent's list, full update clears the cache | `artic-platform`, with its hand-written test elements | section 4 |
| `initial_tree_update_covers_all_nodes`, `text_change_updates_only_that_node`, `clean_frame_pushes_no_update`, `stack_panel_children_in_visual_order`, `freed_node_removed_from_tree` | `artic` (the facade crate's tests), with the real `Surface`, `Border`, `TextBlock` and `StackPanel`, a fake host whose `Accessibility` records updates, and headless frames | #54's acceptance criteria |
| adapter made before the window shows, events reach the adapter, activation requests a full tree | `artic-winit`, with its fake `Loop` and `Window` | section 6 |

The facade crate already depends on every layer, so these tests add no dependency edge and no exception to the [layering rules](../architecture.md#rules). `text_change_updates_only_that_node` keeps the `TextBlock` in a fixed-size parent, so the new text resizes nothing else.

The pull request that adds the adapter records a manual check with NVDA on Windows, VoiceOver on macOS and Orca on Linux reading the hello example.

## 8. Pull requests

1. This document, and the documents it changes: the [frame design](frame.md) (the pass is a `Pipeline` stage, not a `Hooks` closure), the [host design](host.md) (the `accessibility` capability), [technical spec](../spec/technical_spec.md) section 12 (child lists compared, not only render-dirty and moved nodes), the [element design](element.md) section 7 and the [architecture](../architecture.md) rule for the frame pipeline.
2. `NodeId::to_bits`, and the pass in `artic-platform` with its unit tests.
3. The `Accessibility` capability, the pass wired into `Runner` and `Pipeline`, and the facade's acceptance tests.
4. `accesskit_winit` in `artic-winit`, and the manual screen reader checks. Closes #54.
