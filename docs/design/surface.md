# Design: surfaces

Issue: #48. Builds on: the [host seam](host.md) (#39), the [frame pipeline](frame.md) (#40), attached properties (M0-30), `Element` and its property groups ([element design](element.md), #42). Unblocks: #52 (`UserControl` views), #55 (`artic` facade), #122 (`Application` resources and styles), #174 (multiple windows), the mobile hosts (#141, #142).

A **surface** is a root the app asks the host to present: an OS window on a desktop, the whole screen on a phone, a canvas on the web. The app declares its surfaces in the tree, under one `Application` node, and the runner makes the host match the tree: a surface added or reopened is presented, one closed is taken down, and one whose title or size changed is updated. App code never opens, updates or closes anything itself, and the same markup runs on every platform.

```xml
<Application x:Class="Notes.App"
             Desktop.Chrome="System">
  <Application.Resources>
    <Style x:Key="ToolSurface" TargetType="Surface">
      <Setter Property="Desktop.SizeToContent" Value="WidthAndHeight"/>
    </Style>
  </Application.Resources>

  <Surface Title="Notes" Desktop.Width="900" Desktop.Height="600">
    <local:NotesView/>
  </Surface>

  <Surface Title="Inspector" Style="{StaticResource ToolSurface}" IsOpen="{Binding ShowInspector}">
    <local:InspectorView/>
  </Surface>
</Application>
```

`Application.Resources` arrives with #122; the rest is this design.

## 1. Who sees what

| Who | Writes | Sees |
| --- | --- | --- |
| App code | an `Application` holding `Surface`s, each holding a view, usually a `UserControl`; `IsOpen` to open and close one | `Application`, `Surface`, `Desktop`, `UserControl` |
| The facade (#55) | builds the tree from the views, then `Runner::new(tree, renderer, engine, application)` (the engine is `artic_text::Engine::system()`, [text design](text.md)) and `artic_platform::run(host, runner)` | `Runner`, `run` |
| Control authors | `measure_override(&mut MeasureCx, …)` and `arrange_override(&mut ArrangeCx, …)` | `MeasureCx`, `ArrangeCx` |
| Host adapters | `Windows::open`, `update` and `close` | `WindowDesc`, `Windows` |

The vocabulary splits by layer. App code says `Surface`, with platform settings as attached properties of a family owner such as `Desktop`. `artic-platform` and the hosts say surface too, and "window" appears only in `Windows`, the capability of hosts whose surfaces are OS windows.

## 2. The types

| Type | Crate | Shape |
| --- | --- | --- |
| `Application` | `artic-element` | An `#[element]`, the root of the tree. Its content is the collection of its `Surface`s. #122 adds `Resources` and `Styles` to it |
| `Surface` | `artic-element` | An `#[element]` with `Title: String` (`affects = surface | render`, so a change also rebuilds the accesskit node) and `IsOpen: bool` (default `true`, `affects = surface`), and a single content. Allowed only as a child of `Application`. Accesskit role `Window`, named by `Title` |
| `Desktop` | `artic-element` | An attached-property owner with no instances. `Width: Option<f32>`, `Height: Option<f32>`, `SizeToContent`, `Chrome`, all `inherits`, `affects = surface`, `target = any` |
| `SizeToContent` | `artic-element` | `UiEnum`: `Manual` (default), `Width`, `Height`, `WidthAndHeight` |
| `Chrome` | `artic-element` | `UiEnum`: `System` (default: the OS draws the title bar and borders), `None` (the app draws everything) |
| `UserControl` | `artic-controls` | An `#[element]` with a single content. Accesskit role `GenericContainer`. `artic-controls` gains its `artic-element` dependency |
| `Affects::SURFACE` | `artic-core` | The third dirty stage, beside `LAYOUT` and `RENDER` |
| `MeasureCx`, `ArrangeCx` | `artic-layout` | Replace `LayoutCx`, one per pass (section 5) |
| `WindowDesc` | `artic-platform`, moved from `artic-core` | `title: String`, `size: LogicalSize` with `Option<f32>` axes (`None`: the host picks), `chrome: Chrome`, `resizable: Axes`. The runner builds it; only a host check such as the `smoke` example builds one by hand |
| `Windows` | `artic-platform` | `open(&WindowDesc) -> SurfaceId`, `update(SurfaceId, &WindowDesc)`, `close(SurfaceId)` |
| `SurfaceView<'a>` | `artic-render`, renamed from `Surface<'a>` | Unchanged: the id, size, scale and `Target` that `Renderer::render` gets for one frame. It's a view of the host surface `SurfaceId` names, lent for one render, and the rename frees `Surface` for the element |

`Surface` and `Application` are `#[element]`s like any other, so they carry the four property groups and styles, names and inheritance work on them unchanged. A surface's `layout` group acts inside the surface, as on any element: `Margin` insets its content, `MaxWidth` caps it. The window's own size comes only from `Desktop`.

`Desktop` lives in `artic-element`, not in a desktop crate, so markup that sets `Desktop.Width` compiles on every target. The owner is a set of property keys with no platform code behind it; a host that isn't a desktop never reads them.

## 3. Platform settings

A setting that means the same everywhere is a plain property of `Surface`: `Title`, `IsOpen`. A setting that means something to one platform family is an attached property of that family's owner: `Desktop.Width`, later `Mobile.Presentation`, and finer owners for one OS, such as `MacOS.TransparentTitleBar`, on the same mechanism. A surface's own type never learns that a platform exists, and a new platform adds an owner without touching `Surface` or another host.

Settings are shared through mechanisms artic already has:

| Need | Mechanism |
| --- | --- |
| One setting for the whole app | set it on `Application`: every family property `inherits`, so each surface gets it |
| A group of settings for some surfaces | a `Style` on `Surface` whose setters name the attached properties, grouped by role (`ToolSurface`), not by platform |
| One surface differs | a local value on that surface |

Precedence is the usual one, `Local > Binding > StyleSetter > Inherited > Default`, so there are no merge rules of its own. Settings stay typed: the XAML compiler checks `Desktop.Chrome` and the LSP completes it. Attached properties inherit through `inherits` on `attached`, which #48 added for the family owners.

## 4. Lifecycle

Settle, which the runner runs after every callback ([frame design](frame.md) §3), ends with **reconcile**: the runner walks `Application`'s `Surface` children and makes the host match them.

| A surface that is | Reconcile |
| --- | --- |
| open (`IsOpen`), unbound and not lost, on a host with `Windows` | measures it (section 6), builds its `WindowDesc`, calls `Windows::open` and binds the returned `SurfaceId` to it |
| open and unbound, on a host without `Windows` | binds it to the next host surface no `Surface` holds, in tree order; with none left it stays unbound (see windowless hosts below) |
| bound and `SURFACE`-dirty, on a host with `Windows` | rebuilds its `WindowDesc`; when it differs from the last one applied, calls `Windows::update` |
| bound and `SURFACE`-dirty, on a host without `Windows` | nothing reaches the host: such a host takes no title, size or chrome. `Title` still names the surface's accesskit node, and `Presenter` (mobile) defines what it takes when it arrives |
| bound, and closed (`IsOpen` false) or no longer in the tree | calls `Windows::close` where the host has it, and unbinds it |

`SURFACE` is cleared when reconcile reads the surface. A change to an inherited `Desktop` value on `Application` marks each surface whose effective value changed, through the same inheritance path as `LAYOUT` and `RENDER`.

**Closing.** The user closes a surface through `event(CloseRequested)`: the runner writes `IsOpen = false` at `Local`, and reconcile takes it down. The node stays in the tree, so its state survives and setting `IsOpen` again reopens it. A `surface_lost(Gone)` that no close request preceded, such as a window that couldn't be created, one the OS destroyed, or the loop exiting, isn't the user's decision and writes nothing: the runner unbinds the surface and marks it **lost**, and reconcile doesn't open a lost surface again until its `IsOpen` goes false and then true, so a window that can't be created isn't retried in a loop. When every surface is closed or lost, the runner exits. A `Suspended` loss is unchanged: the surface stays bound and is drawn again when the host reports it.

Writing `Local` shadows a binding: after the OS closes a surface whose `IsOpen` is bound, the binding no longer reaches it. Two-way binding replaces this write with one that goes back to the binding's source; until then a bound `IsOpen` opens a surface once. A cancellable `Closing` event arrives with routed events and sits in front of the write.

**Windowless hosts.** A host without `Windows`, such as a phone or a framebuffer, presents surfaces it creates itself. The runner binds open `Surface`s to them in tree order, as it binds roots today; a surface with no host surface left stays unbound.

**Mobile.** A mobile host has one host surface. The first open `Surface` fills it, with its content inset by the safe area (#147). Presenting more is a capability of its own, `Presenter`, which the mobile hosts (#141, #142) add beside `Windows`: through it, a second open `Surface` is presented over the first as a sheet, and closing it reveals the first; `Mobile.Presentation` chooses another presentation. A host with neither capability leaves the second surface unbound, as above. `Desktop` settings do nothing there, and `Title` names the app in the task switcher and to accessibility.

## 5. Measure and arrange

`LayoutCx` becomes two contexts, one per pass:

| | `MeasureCx` | `ArrangeCx` |
| --- | --- | --- |
| Reads the tree, `measure(child, available)` | yes | |
| `arrange(child, rect)`, `desired_size(child)` | | yes |
| `scale()`, `snap(rect)` | | yes |
| `text()`, the text engine ([text design](text.md)) | yes | yes |
| `set_measured(result)` / `set_arranged(result)` | yes | yes |

`Layoutable::measure` and `ElementImpl::measure_override` take `&mut MeasureCx`; `arrange` and `arrange_override` take `&mut ArrangeCx`. A desired size is in logical pixels and can't depend on the display, which spec §7 (layout protocol, rule 5) already intends and the types now enforce: a `measure_override` that reads the scale doesn't compile.

That's what makes a surface measurable before it has a window, and it makes a scale change cheap. `LayoutManager::run` remembers the scale it last arranged at. When the scale changes, as when a window moves to a monitor with a different scale, it arranges from the root again. The root is offered the surface's logical size, its device size over the scale: if that is unchanged, every measure answers from its cache; if not, as when a host reports the new scale before the size that matches it (Windows does), the root is measured again like on any resize, and only the children offered a different size follow. `surface_changed` stops marking the whole subtree `LAYOUT` on a scale change.

## 6. Sizing

This section is about surfaces the runner opens through `Windows`. A surface bound to a host surface it didn't open, on a phone or a framebuffer, is laid out at that host surface's logical size, its device size over its scale, on both axes; `Desktop` settings, `SizeToContent` included, don't apply to it.

Before opening a surface, the runner measures it with a `MeasureCx`. Each axis gets `∞` when `SizeToContent` covers it, `Desktop.Width` or `Height` when set, and otherwise `∞` with the host picking the size. `WindowDesc.size` carries the measured size on the `SizeToContent` axes, the set value on the others, and `None` where the host picks. The window opens at that logical size, and the OS converts it to device pixels for whichever monitor it lands on, so there is no hidden first frame and no visible jump.

While a surface is open, each frame lays it out with `∞` on its `SizeToContent` axes, so its root takes its desired size `D` there ([frame design](frame.md): a root's rectangle is its desired size on an infinite axis). After layout, when `D` differs from the last size the runner *requested*, it marks the surface `SURFACE` with `UiTree::mark_dirty`, which sets the pending flag, so the settle that ends the frame reconciles it and calls `Windows::update`. The OS resizes, `surface_changed` arrives, and the next layout measures at `∞` again and gets `D` again, so it settles in one round. The comparison is with the request, not with the window's actual size, so a window the OS clamps, such as one larger than the screen, settles too: `D` is requested once and never again.

The user can't resize a `SizeToContent` axis: `WindowDesc.resizable` leaves it out, and the host locks it. Content owns that axis, and nothing writes to app state behind the developer's back.

## 7. Testing

| Crate | Tests |
| --- | --- |
| `artic-layout` | `scale_change_arranges_without_measuring` (`LayoutStats.measured == 0`), `measure_cx_has_no_scale` (a compile-fail doctest) |
| `artic-element` | `surface_accesskit_node` (role `Window`, named by `Title`, renamed when `Title` changes), `desktop_chrome_inherits_from_application`; in `artic-core`, `attached_property_inherits` |
| `artic-controls` | `user_control_accesskit_node` (role `GenericContainer`), `user_control_lays_out_its_content` |
| `artic-platform`, through `Runner` and the fake host | `surface_opens_measured_to_content`, `surface_opens_at_desktop_size`, `title_change_updates_once`, `unchanged_surface_is_not_updated`, `size_to_content_settles` (the OS clamps, and no second request follows), `os_close_writes_is_open_and_reopens`, `failed_open_is_not_retried`, `content_growth_resizes_window`, `last_surface_closing_exits`, `removed_surface_closes`, `windowless_host_binds_in_tree_order`, `windowless_surface_ignores_size_to_content` |

The `frames` example builds an `Application` with one `Surface` and runs unchanged in CI. The `smoke` example stays a check of the host alone, without the runner: it opens its window through `Windows::open` with a `WindowDesc` it builds itself.

## 8. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | this document; spec §6, §7 and §10, `architecture.md`, and pointers in the host and frame designs; #267 for `Desktop.State` and `Desktop.Display` | approved |
| B | `MeasureCx` and `ArrangeCx`, and a scale change that arranges without measuring | the `artic-layout` tests in section 7 |
| C | `Affects::SURFACE`; attached properties that inherit; `Application`, `Surface`, `Desktop`, `SizeToContent`, `Chrome`, `UserControl`; reconcile, `WindowDesc` in `artic-platform`, `SurfaceView`, `Windows::update`, the `artic-winit` mapping; both examples ported; the overlapping-root checks and their tests removed, since surfaces under `Application` are disjoint | the `artic-element`, `artic-controls` and `artic-platform` tests in section 7, and `smoke` and `frames` passing in CI |

## 9. Later

- `Desktop.State` (`Normal`, `Maximized`, `Minimized`, `FullScreen`) and `Desktop.Display`: #267. `State` is written by the OS as well as the app, so it waits for two-way binding.
- `Chrome.Extended`, content drawn under the title bar with the OS buttons kept: #173.
- Owned surfaces: a `Surface` inside a `Surface`, closing with its owner. `Surface` is allowed only under `Application` until then, so allowing it is additive.
- `Application.ShutdownMode`, for tray apps that keep running with no surface open: #122.
- A `SizeToContent` value that sizes once at open and then leaves the axis to the user.
