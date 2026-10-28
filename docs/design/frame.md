# Design: the frame pipeline

Issue: #40. Builds on: the [host seam](host.md) (#39), dirty flags (#15), the `Dispatcher` (#16), the retained scene and `Renderer` (#33), `LayoutManager` (#41), `Element` and its property groups ([element design](element.md), #42). Unblocks: #54 (accesskit), #55 (`artic` facade), #110 (style pass), #115 (`Transitions`), #165 (debug overlay).

A **frame** turns changed state into pixels on one surface: posted work runs, the style pass applies, layout measures and arranges what changed, the scene is rebuilt from the nodes that need it, and the renderer draws it. A frame with nothing to do does no work at all, and an app with nothing to do sleeps until the OS or another thread wakes it.

## 1. Who sees what

| Who | Writes | Sees |
| --- | --- | --- |
| The facade (#55) | builds the tree from the views, then `Runner::new(tree, renderer, engine, application)` with the engine it creates, `artic_text::Engine::system()` ([surface design](surface.md), [text design](text.md)) and `artic_platform::run(host, runner)` | `Runner`, `run`, `Hooks`, `Error` |
| Features that run inside a frame (style pass, size classes, accesskit, transitions) | a closure registered on `Hooks` | `Hooks`, `UiTree`, `SurfaceInfo`, `FrameStats` |
| Tests and the debug overlay (#165) | | `FrameStats`, per frame and per surface |
| Host adapters | nothing new | `Runner` is an ordinary `AppHandler` |

## 2. Layers

All in `artic-platform`, which depends on `artic-element` to drive the tree as `dyn Element`. It still names no concrete control and no style or theme crate.

| Type | Role |
| --- | --- |
| `Pipeline` | One surface, no host: `prepare` runs layout and composition for the surface's root and returns a `Frame`; `present` hands it to the renderer. Owns the surface's `LayoutManager` and `Compositor`, and the last `Scene` it presented |
| `Frame` | The surface's `SurfaceId`, size and scale, the owned `Scene`, and the stats so far. `Send` |
| `FrameStats` | `{ measured: u32, arranged: u32, recorded: u32, rendered: bool }`, `Copy + Eq + Default` |
| `Hooks` | Per-stage lists of boxed closures: `before_layout`, `after_present`, `wants_frame`, and `on_error` for a frame that failed |
| `Runner<R: Renderer>` | `impl AppHandler`. Owns the `UiTree`, the renderer, the text engine, the `Hooks` and the `ElementTypes` cast table, built once and lent with the engine to every pipeline as `FrameServices`, and per surface its root, latest `SurfaceInfo`, a `needs_frame` mark, and its `Pipeline` |
| `run(host, runner) -> Result<(), Error>` | Checks `renderer.accepts(host.targets())`, then `host.run(runner)`; returns the error a frame failed with, if any, or else the host's own error, when the host's `run` returns |
| `Error` | `Unsupported(TargetKind)`: the renderer can't draw on what the host offers; `Render(RenderError)`: a frame failed; `Host(Box<dyn Error + Send + Sync>)`: the host's loop failed |

```rust,ignore
impl<R: Renderer> Runner<R> {
  pub fn new(tree: UiTree, renderer: R, text: Box<dyn TextEngine>, application: NodeId) -> Self;
  pub fn hooks(&mut self) -> &mut Hooks;
}

impl Hooks {
  pub fn before_layout(&mut self, hook: impl FnMut(&mut UiTree, NodeId, &SurfaceInfo, Duration) + 'static);
  pub fn after_present(&mut self, hook: impl FnMut(&UiTree, NodeId, &FrameStats) + 'static);
  pub fn wants_frame(&mut self, hook: impl FnMut() -> bool + 'static);
  pub fn on_error(&mut self, hook: impl FnMut(&Error) + 'static);
}
```

The runner owns the tree and lends it to each stage and hook in turn, so the tree, a surface's pipeline and the renderer are borrowed as separate fields, never as one struct, and no hook needs a `RefCell` to reach the tree.

Only stages that run inside a frame are hooks. Features that react to host events, such as the OS color scheme (#118), safe-area and keyboard insets (#147, #150) and platform defaults (#152, #156), arrive with their `HostEvent` variants and their own handlers.

Each surface has a pipeline of its own because each cache is cleaned per root: `LayoutManager::run` drops what it knew about nodes outside the root it lays out, and `Compositor::compose` drops the fragments it didn't visit, so two windows sharing one would discard each other's work on every frame.

Surface roots are disjoint: each `Surface` is a child of the `Application` and a `Surface` is accepted nowhere else ([surface design](surface.md) §2), and a node has one visual parent, so it belongs to at most one surface. Each pipeline therefore clears layout and render flags only on its own surface's nodes, and one surface's frame never consumes another's invalidation.

## 3. From a write to a request

A property write records that work exists and calls nothing: the `HostContext` that can ask for a frame exists only during an `AppHandler` call, and running code in the middle of a write invites re-entrancy.

`artic-core` keeps one **pending** flag per `UiTree`. Each node's properties receive a dirty handle holding the node's dirty cell and that flag, and every path that marks a node dirty sets both: a property write, `UiTree::mark_dirty`, and a child added to or removed from the visual tree. `UiTree::take_pending()` returns the flag and clears it.

The runner ends every `AppHandler` call by **settling**:

1. **Drain**: `Dispatcher::run_pending()`, then `flush_effects()`, repeated until the queue is empty. `run_pending` flushes effects only after each task it runs, so a write made directly in a callback or a hook, with no task queued, needs the explicit flush; an effect can post a task, hence the repeat.
2. If `tree.take_pending()` is true, every live surface gets `needs_frame` and a `cx.request_frame`.
3. **Reconcile**: when `UiTree::structure_generation()` moved since the last check, the runner first checks the `Application`'s shape (only `Surface` children, each a visual child of it, none inside another) and lists its surfaces again; then the host is made to match those surfaces, opening, updating and closing them ([surface design](surface.md) §4), so an `IsOpen` or `Desktop` write from any call reaches the host in the same settle.

Every change happens inside some call (an event, a wake-up, a task run from one), so settling catches it. The flag is tree-wide, so a change in one window also redraws the others: each one composes again and presents, but records nothing that didn't change. A flag per surface would avoid that and can replace this one behind `take_pending`.

Pending work never keeps the loop awake by itself: the queue is empty after each settle, and nothing reads the nodes' flags to decide whether to ask for a frame, only writes set the pending flag. A hidden subtree keeps its render flags (element design §7) without asking for frames.

## 4. One frame

`AppHandler::frame(cx, surface, target, time)`:

1. If the surface is unknown or lost, return.
2. Drain, then run the `before_layout` hooks with this surface's root, `SurfaceInfo` and the frame's `time`, the host's monotonic estimate of when the frame reaches the display, so animations tick from the display's timing rather than a clock of their own.
3. Drain again, so the effects of the hooks' writes land, then `take_pending()`; if true, mark every surface and request frames for the others. Writes from step 2, such as the style pass's, count for this frame. If the hooks changed the tree's structure, the `Application`'s shape is checked again here, so a hook that breaks it is caught before anything renders. If the surface is now closed or out of the `Application`, the runner settles and returns.
4. If this surface isn't marked, nothing changed for it, but the host still asked for pixels: an OS redraw, such as a window uncovered or restored, reaches `frame` like a requested one, and a host can't always tell them apart (winit can't). The pipeline presents the last `Scene` again, with nothing measured or recorded; the `after_present` hooks and `wants_frame` run as in step 6, then the runner settles and the loop goes back to sleep. A surface with no scene yet has nothing to present again, so the frame returns at once.
5. Clear the mark and `pipeline.prepare(tree, root, info, &mut services)`, lending the runner's `ElementTypes` and text engine as `FrameServices`, from which the pipeline builds the `LayoutNodes` and render `Nodes` for this frame:
   - `LayoutManager::run` at `size / scale` in logical units, with pixel snapping and the runner's text engine ([text design](text.md)). It measures only layout-dirty nodes and those whose desired size a change reached, and reports the nodes whose arranged size changed and the nodes whose layout result was set or dropped, including by a `measure` call since the last run.
   - `Compositor::compose` through the pipeline's render `Nodes` (section 5). It records only render-dirty nodes and the nodes layout resized or changed the layout result of, and reuses every other fragment.
6. `pipeline.present(renderer, frame, target)` builds the renderer's `SurfaceView` from the frame's id, size and scale and the lent target, and renders; then the `after_present` hooks run with the stats.
7. If any `wants_frame` hook returns true, mark the surface and request its next frame.
8. Settle (section 3). A write made in steps 5 to 7, such as by an `after_present` hook, sets the pending flag after step 3 read it; settling marks the surfaces and asks for exactly one more frame.

Each stage clears a node's flag before doing the node's work, never after: `LayoutManager::run` already clears `LAYOUT` before it measures, and the compositor clears `RENDER` before it calls `record`. A write made during that work, such as from an element's `render`, then sets the flag again and survives to the follow-up frame instead of being cleared with the work it came too late for.

Layout reports resized nodes rather than marking them render-dirty: a mark would set the pending flag in the middle of the frame and cost a second, empty frame after every layout change, such as each step of a window resize.

Pacing comes from the host and the renderer: a surface asks for its next frame only from step 7, and a renderer that presents in step with the display (as `artic-vello` does) holds that request to the display's refresh, so each surface runs at its own display's rate.

### The scene boundary

v1 renders on the UI thread, but a render thread must stay possible without changing the API (D6, spec §11). What makes it possible is data that moves by value: `prepare` reads the tree, `present` reads only the `Frame`, which owns its `Scene` and is `Send`. A test asserts `Frame: Send`, so the boundary can't silently break. The scene is half of what a render thread needs; the other half is a target it can keep. `present` takes the `Target` the host lends for one `frame` call, and its `Handles`, `Pixels` and `Custom` variants all borrow from the host, so a render thread also needs the host to hand the renderer a presentation target it owns, such as the window's raw handles kept for the window's lifetime. That is the render thread's own change, at the host seam, and it leaves `prepare`, `Frame` and the tree side of the pipeline as they are.

## 5. The render `Nodes`

The compositor's `Nodes` for any laid-out `dyn Element` is `artic_element::ElementNodes`, which finds each node's element through `ElementTypes`; the pipeline wraps it to add `stale`, and `artic_test`'s `Headless::record` uses it to draw one node the same way:

- `layers(tree, node)` casts the node and calls `Element::layers` with its arranged size from `LayoutManager::rect`. The element's own transform pivots on its centre in its own coordinates, so the node's layout offset is composed after it: `transform.then(translate(rect.origin))`, or the offset alone when the element has none. `LayoutManager::rect` is in the parent's coordinates, and the compositor nests children inside their parent's layers, so offsets add up down the tree.
- `record(tree, node, builder)` casts the node and calls `Element::render` with a `RenderCx` for it, which carries the node's arranged size (`RenderCx::size`) and its layout result (`RenderCx::layout_result`, such as a shaped paragraph), so an element draws to the box layout gave it, with what layout worked out.
- `stale(node)` is true for the nodes layout resized or changed the layout result of, and the compositor hasn't recorded since. The pipeline keeps them in a set that layout's `resized` and `results` add to and recording removes from, and drops entries for nodes that left the root's subtree, so a node resized while hidden, which the compositor skips, is still recorded again when it's shown. The compositor re-records a node that is render-dirty, stale, or has no fragment yet, so a node whose box changed without any property changing doesn't keep a drawing of the old size. A node that only moved stays as recorded: its offset is a layer.

A node that isn't an element gets default layers and records nothing. A node layout never arranged isn't drawn.

## 6. Surfaces and events

| Call | The runner |
| --- | --- |
| `started` | settles, so the first reconcile opens each open `Surface` through `cx.windows()` and binds its `SurfaceId` to it ([surface design](surface.md) §4). A host without `Windows`, such as a framebuffer, gets the open surfaces bound in tree order to the surfaces `surface_changed` reports |
| `surface_changed` | keeps the new `SurfaceInfo`, marks the surface and requests a frame for it, even when nothing differs from the last report: a surface reported again after `surface_lost`, such as on resume, has lost what the renderer kept for it and must be drawn again. It marks no node layout-dirty: a size change measures the root again because its logical size changed, and a scale change arranges the surface again from its root, snapped to the new device-pixel grid; measures answer from their caches unless the logical size (device size over scale) changed too, which measures the root again like any resize ([surface design](surface.md) §5) |
| `surface_lost` | calls `renderer.surface_lost`. A `Suspended` loss flags the surface lost and keeps its root, since a resume reports it again; a `Gone` loss forgets it, and when no window but closing ones is left, the runner exits |
| `event(CloseRequested)` | closes the window and flags the surface closing, so it gets no more frames; the `Gone` loss the host reports after a close releases the renderer's resources and forgets it. The last window closing exits |
| `event(Suspended)`, `event(Resumed)` | nothing beyond settling; the surfaces come and go through `surface_lost` and `surface_changed` |
| `woken` | settles; #79 connects the dispatcher's cross-thread `post` here |

`AppHandler` calls can't return errors. On `RenderError::SurfaceLost` the runner calls `renderer.surface_lost(id)`, which drops what the renderer kept for the surface (`artic-vello`'s swapchain), then marks the surface and asks for a frame, so the next one rebuilds it. Any other render error is passed to every `on_error` hook, and the runner calls `cx.exit()`. `Host::run` returns the host's own error, such as a lost connection to the display server, so `run` hands the host a small private handler around the runner that passes every call on and keeps the first frame error. When the host's `run` returns, `run` returns that error as `Error::Render`, or else the host's error as `Error::Host`, or `Ok(())`: the frame's error wins because it ended the loop. A host whose `run` returns at once, such as a browser's, returns `Ok(())`, so `on_error` is how an app on it learns of a failure. A launcher that wraps the runner in its own handler and calls `Host::run` itself, as the `frames` example does, gets the host's error from it and frame errors through `on_error`.

## 7. Stats

Each stage counts what it did: `LayoutManager::run` returns `LayoutStats { measured, arranged, resized, results }`, where `resized` lists the nodes whose arranged size changed and `results` the nodes whose layout result was set or dropped since the last run ([text design](text.md)), the compositor counts the fragments it recorded, and `present` sets `rendered`. A frame that stops at step 4 has `FrameStats { rendered: true, ..Default::default() }`, and nothing else ran. They're plain integers returned from the call that did the work, always on, so tests and the debug overlay read exactly the frame they ran, through `after_present`.

## 8. Testing

| Crate | Tests |
| --- | --- |
| `artic-core` | `property_write_sets_pending`, `mark_dirty_sets_pending`, `child_change_sets_pending` (taking the pending set resets it, checked in `property_write_sets_pending`) |
| `artic-layout` | `run_reports_measured_and_arranged`: a second run with nothing dirty reports zero; `run_reports_resized`: a node whose size changed is listed, one that only moved isn't; the `results` tests are in the [text design](text.md) |
| `artic-render` | `compose_reports_recorded`: only render-dirty nodes count; `stale_node_is_recorded_again`; `write_during_record_stays_dirty` |
| `artic-platform` | `layout_offset_applies_after_render_transform`, `render_cx_has_the_arranged_size`, and `Frame: Send` as a static assertion |
| `artic-platform`, through `Runner`, a fake `HostContext` and a counting renderer on a `Pixels` target | `clean_frame_is_skipped` (an unasked-for frame measures and records nothing, presents the last scene, and requests no frame, so the loop sleeps), `os_redraw_presents_the_last_scene`, `render_only_change_skips_layout`, `layout_change_remeasures_subtree`, `resize_invalidates_root`, `resized_node_is_recorded_again`, `replaced_result_is_recorded_again`, `node_resized_while_hidden_is_recorded_when_shown`, `windows_keep_their_own_caches`, `write_after_present_requests_one_frame`, `surface_lost_render_error_resets_the_renderer`, `style_write_lands_in_same_frame`, `hook_write_effects_land_in_same_frame`, `wants_frame_requests_another`, `last_surface_closing_exits`, `closed_window_is_released_from_the_renderer`, `render_error_is_returned_from_run`, `render_error_reaches_on_error`, `mismatched_renderer_is_rejected`, `first_render_error_is_returned`, `render_error_comes_before_the_host_error`, `host_error_is_returned_from_run`, `run_that_draws_returns_ok`, `run_passes_every_host_call_to_the_runner`, `host_that_returns_at_once_runs_ok` |

An `artic-winit` example, `frames`, opens a window with one element through `Runner` and `artic_vello::Renderer`, changes a property once, and exits with an error if any frame runs in the last 500 ms of a 1.5 s run. The Windows and macOS CI jobs run it next to `smoke`. A quiet window, not a frame count, is what it asserts, so a transient resize at startup can't fail it.

## 9. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | this document; pointers in the element and host designs | approved |
| B | `DirtyHandle`, the pending flag and `take_pending` in `artic-core`; `LayoutStats`; `Nodes::stale`, the compositor's recorded count, and the compositor clearing `RENDER` before `record` | the core, layout and render tests in section 8 |
| C | `Pipeline`, `Frame`, `FrameStats`, `Hooks`, `Runner`, `run`, `Error`, the render `Nodes`, `RenderCx::size`; the `frames` example and its CI step | the `artic-platform` tests in section 8, and `frames` passing on Windows and macOS |
