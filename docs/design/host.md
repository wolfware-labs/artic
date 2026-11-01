# Design: the host seam

Issue: #39. Builds on: the [backend seams](backends.md) (#240), `artic-render`'s `Target` and `Renderer` (#33, #34), the `Dispatcher` (#16). Unblocks: #40 (frame pipeline), #48 (surface roots), #73 (input), #79 (cross-thread posting), #101 (`TestHost`), #141 and #142 (mobile hosts).

A **host** drives the screen: it owns the event loop, reports surfaces and events to artic, draws frames when artic asks, and wakes up when work is posted from another thread. The [backend design](backends.md) fixed the roles; this document fixes the traits a host adapter implements and calls, and the first adapter, `artic-winit`, which opens real windows on Windows, macOS and Linux.

## 1. Who sees what

| Who | Writes | Sees |
| --- | --- | --- |
| App authors | `pub fn app() -> artic::App`, and a launcher: `artic::run(artic_winit::Host::new()?, artic_vello::Renderer::new()?, my_app::app())`, from a `main` that returns a `Result` | `artic::App`, `artic::run`, a host and a renderer |
| Host adapter authors (winit, SDL, a framebuffer, a browser) | `impl Host for MyHost`, calling an `AppHandler` from their loop | `Host`, `AppHandler`, `HostContext`, `SurfaceInfo`, `HostEvent`, `Waker` |
| artic (#40) | `Runner`, the frame pipeline's `impl AppHandler` ([frame design](frame.md)) | the same seam, from the other side |

`App` stays the app author's type. The seam is named `AppHandler`, after winit's `ApplicationHandler`, which plays the same role one level down, so adapter authors recognize it and the two never share a name.

## 2. The traits

All in `artic-platform`, which has no backend dependencies.

```rust,ignore
pub trait Host {
  /// Why the host's loop can fail, such as a lost connection to the display server.
  type Error: std::error::Error + Send + Sync + 'static;

  /// What kind of target the host offers, so `artic::run` can reject a renderer that can't draw
  /// on it before the loop starts.
  fn targets(&self) -> TargetKind;

  /// Runs the loop, calling `app` until it exits, and returns the host's own failure. A host whose
  /// loop can't block, such as a browser, installs its callbacks and returns `Ok(())` at once; the
  /// callbacks keep calling `app`.
  fn run(self, app: impl AppHandler + 'static) -> Result<(), Self::Error>;
}

pub trait AppHandler {
  fn started(&mut self, cx: &mut dyn HostContext, waker: Waker);
  fn woken(&mut self, cx: &mut dyn HostContext);
  fn surface_changed(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, info: SurfaceInfo);
  fn surface_lost(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, loss: SurfaceLoss);
  fn event(&mut self, cx: &mut dyn HostContext, event: HostEvent);
  fn frame(&mut self, cx: &mut dyn HostContext, surface: SurfaceId, target: Target<'_>, time: Duration);
}

pub trait HostContext {
  fn request_frame(&mut self, surface: SurfaceId);
  fn exit(&mut self);
  fn windows(&mut self) -> Option<&mut dyn Windows> { None }
  // clipboard, text input, cursor, accessibility, …: added by their issues, each defaulting to None
}

pub trait Windows {
  /// Opens a window; its surface arrives through `surface_changed`, or `surface_lost` if it
  /// couldn't be created.
  fn open(&mut self, desc: &WindowDesc) -> SurfaceId;
  fn update(&mut self, surface: SurfaceId, desc: &WindowDesc);
  fn close(&mut self, surface: SurfaceId);
}
```

- **A context on every call.** artic acts on the host only through the `cx` it's handed: asking for a frame, exiting, opening a window. This is winit's own rule (windows are created from `&ActiveEventLoop`, inside callbacks), and it needs no shared mutable state between artic and the host. `run` consumes the host, so the capabilities live on the context, not on `Host`.
- **Capabilities default to `None`.** A host implements what its target has; artic decides the fallback at each use ([backends §2](backends.md#capabilities)). A new capability is a new default method on `HostContext`, so existing adapters keep compiling. #39 implements `windows`; clipboard (#91), IME (#90), color scheme (#118) and OS services (#172) arrive with their issues.
- **The target is lent only in `frame`.** `surface_changed` reports facts; `frame` is the moment to draw, with the window's handles or a pixel buffer borrowed for that call. A renderer keys its swapchain on the `SurfaceId`, creates it the first time it sees the handles, and drops it on `surface_lost`, as `artic-vello` already does. A `Pixels` host hands a fresh buffer each frame and presents it after the call.
- **Each frame carries its time.** `time` is monotonic, from an epoch the host picks (such as the loop's start), and is the host's best estimate of when the frame will be presented, or now when the platform doesn't say. Animations read it, so they follow the display rather than when the code happens to run. It's a `Duration` rather than an `Instant` because `Instant` panics on `wasm32-unknown-unknown`, and a browser's `requestAnimationFrame` timestamp converts to it directly, as do `CADisplayLink`'s and `Choreographer`'s.
- **`open` doesn't fail synchronously.** It returns the new id at once; the surface follows through `surface_changed`, or `surface_lost` if the window couldn't be created. A browser host, whose canvas may appear later, works the same way.

## 3. The data

| Type | Crate | Holds |
| --- | --- | --- |
| `WindowDesc` | `artic-platform` | `title: String`, `size: LogicalSize` with `Option<f32>` axes (`None`: the host picks), `chrome: Chrome`, `resizable: Axes`. The runner builds it, from a `Surface` ([surface design](surface.md), #48, which moved it from `artic-core` and added `Windows::update`); only the `smoke` example, a check of the host without the runner, builds one by hand. |
| `SurfaceInfo` | `artic-platform` | `size: PhysicalSize`, `scale: f32` (device pixels per logical unit), `display: DisplayId`, `refresh_rate: Option<f32>` (some platforms don't report it) |
| `SurfaceLoss` | `artic-platform` | why a surface went away: `Suspended`, reported again after `Resumed`, or `Gone`, closed, destroyed, never created or the loop exiting |
| `DisplayId` | `artic-platform` | an opaque id the host assigns per display |
| `HostEvent` | `artic-platform` | `#[non_exhaustive]`: `CloseRequested { surface }`, `Suspended`, `Resumed` |
| `Waker` | `artic-platform` | `Clone + Send + Sync`; `wake()` makes the host call `AppHandler::woken` on its loop. An adapter builds one with `Waker::new(hook)`, where `hook` is any `Fn() + Send + Sync + 'static` that wakes its loop. |

`SurfaceId`, `PhysicalSize`, `Target` and `TargetKind` are `artic-render`'s, shared with the renderer seam.

`HostEvent` holds only what a desktop host reports today. Raw input takes the same path: #73 adds its variants to `HostEvent`, so `event` carries pointer, key and gamepad input too, with payload types in `artic-core` so controls see them without the platform; until then a host reports none. `ColorSchemeChanged` arrives with #118, and `Back` and `MemoryWarning` with the mobile hosts. `#[non_exhaustive]` lets each land without breaking an adapter or a handler. Surface facts (size, scale, display, refresh rate) aren't events: they arrive through `surface_changed`.

`CloseRequested` asks; it doesn't close. artic decides, by closing the window through `Windows::close` (then `surface_lost` follows) or by exiting.

## 4. Waking and the dispatcher

`Waker` is the only part of the seam other threads touch. A host builds it in its loop (`artic-winit` from winit's `EventLoopProxy`), and `wake()` from any thread makes the loop call `AppHandler::woken`. artic, not the host, runs posted work: it drains the `Dispatcher` (#16) in `woken` and after each event, so adapters never depend on the dispatcher; they only implement "wake me". #79 connects the dispatcher's cross-thread `post` to the `Waker` artic receives in `started`.

## 5. `artic-winit`

The desktop adapter, at layer 6, depending on `artic-platform`, `artic-render` and `winit`. It exports `Host` and `Error`: `Host::new() -> Result<Host, Error>` fails if the event loop can't be created, such as with no display, and `Error` boxes winit's error as its `source()`, so no `winit` type appears in a public item. Inside, a driver, generic over the event loop and window types, does what the table says, and keeps a map from winit's `WindowId` to artic's `SurfaceId`; `host.rs` forwards winit's `ApplicationHandler` callbacks to it and implements its traits for winit's types.

| winit | `artic-winit` | `AppHandler` |
| --- | --- | --- |
| first `resumed` | hands over the `Waker` `run` built from the `EventLoopProxy` | `started(cx, waker)`; artic opens its first window through `cx.windows()`, and `surface_changed` follows |
| `suspended` | (mobile: the GPU surfaces are destroyed) until resumed, ignores frame requests, redraws and surface changes | `surface_lost(…, Suspended)` for each surface, in open order, then `event(Suspended)` |
| later `resumed` | (mobile: the surfaces are recreated) | `event(Resumed)`, then `surface_changed` for each surface, stopping if the handler asked to exit |
| `WindowEvent::Resized`, `ScaleFactorChanged`, `Moved` | rebuilds the surface's `SurfaceInfo`, with the current monitor as its `DisplayId` and its refresh rate | `surface_changed`, if anything differs from what was last reported |
| `WindowEvent::CloseRequested` | | `event(CloseRequested { surface })` |
| `WindowEvent::Destroyed` (the OS destroyed the window without a close request) | forgets the window | `surface_lost(…, Gone)` |
| `WindowEvent::RedrawRequested` | reads the current monitor again, then lends the window as `Target::Handles`, with the time since `run` started (winit doesn't expose present times) | `surface_changed` if any fact changed, then `frame(cx, surface, target, time)`, unless the handler closed the window in that `surface_changed` |
| `user_event` (a `Waker` fired) | | `woken` |
| `about_to_wait` | `ControlFlow::Wait`: sleep until the next event or wake-up | |

- `cx.request_frame(id)` calls the window's `request_redraw()`, which schedules one `RedrawRequested` for that window; winit merges repeated requests. Scheduling isn't pacing: a handler that asks for the next frame from inside `frame` runs as fast as rendering allows unless something waits for the display. Pacing each surface to its display's refresh (§10) comes from the renderer's vsync'd presentation and the platform's frame callbacks ([frame design](frame.md) §4).
- `cx.exit()` exits the loop; windows still open are reported through `surface_lost(…, Gone)` before `run` returns, and `run` returns `Ok(())`.
- `targets()` is `TargetKind::Handles`.
- The monitor is read on every resize, scale change, move and frame, because Wayland sends no `Moved` and a move between monitors of equal scale sends no scale change; a display change on its own is still reported. A monitor's `DisplayId` follows the monitor's identity as winit gives it, its `MonitorHandle`: the display's UUID on macOS, the `HMONITOR` on Windows, the RandR CRTC on X11 and the `wl_output` global on Wayland. Rearranging displays doesn't change an id, and monitors connected together always have different ones, two of one model included; a monitor plugged in again may get a new id, and on X11 one plugged in later may take the id of one unplugged from its CRTC. A window on no monitor has an id of its own.
- A closed window is kept until its `surface_lost` has been delivered, and so is every window when the loop exits, so the handles a renderer holds stay valid until it's told they're gone. While the loop exits, `open` creates no window and its id is never reported, so a handler that opens a window from `surface_lost` can't keep the loop alive.
- A loss says whether the surface comes back: `Suspended` on suspend, `Gone` when its window is closed, destroyed or never created, or the loop exits. A window closed or destroyed while suspended is reported again, as `Gone`, so a handler never waits for a surface that won't return.
- On Windows, a scale change reports the new scale with the window's old size, and the `Resized` that follows corrects it.
- A window opened during a handler call reports its first `surface_changed` right after that call returns, since the context can't call the handler while it's in a call. A window that couldn't be created reports `surface_lost(…, Gone)` the same way.
- `run` returns `Error::Loop` if winit's loop ends with an error, such as a lost connection to the display server, after the surfaces have been reported lost.

`translate` reads a winit `WindowEvent` and says what the event means: new surface facts, a close request, a frame, a move or the window's destruction. A scale change reads its new size through a callback the host passes, `|| window.inner_size()`, since winit's `ScaleFactorChanged` carries a size writer rather than a size. Unit tests build the other events and call `translate` with no window; winit builds `ScaleFactorChanged` only inside its loop, so the smoke example covers that one.

## 6. Testing

- The `WindowId` → `SurfaceId` map gives distinct ids and maps each back; displays get stable ids, distinct monitors distinct ones, and a window on no monitor one of its own.
- The driver, with a fake loop and windows and a handler that records its calls: each row of the table in §5, the reports a call makes after it returns, and the windows opened while the loop exits.
- `artic-platform`: `Waker` is `Clone + Send + Sync` and `wake()` runs the host's hook; a minimal fake host drives a recording `AppHandler`, showing a host can be written against the seam alone, as `TestHost` (#101) will be.
- `cargo deny` bans `winit` everywhere but `artic-winit`, and no `winit` type appears in `artic-winit`'s public items.
- A smoke example, `artic-examples/examples/smoke.rs`: an `AppHandler` that opens one window in `started`, through `HostContext::windows()` with a `WindowDesc` it builds itself, and asks for a frame after each frame; with `ARTIC_SMOKE_FRAMES=N` it exits after N frames, otherwise it runs until the window closes. It calls only `Host::new()` and `run`, with no target-specific code, and draws nothing: it checks the host alone, without the runner, and the `frames` example draws through the runner. #53 runs it in CI; the PR records manual runs on Windows, macOS and Linux.

## 7. What this changes

| Before (§10, [backends](backends.md)) | After |
| --- | --- |
| `Host::run(self, app: App)` | `Host::run(self, app: impl AppHandler + 'static) -> Result<(), Self::Error>`; `App` stays the app author's type |
| Capabilities as methods on `Host` | Capabilities on `HostContext`, reached inside callbacks; `Host` keeps `targets()` and `run` |
| "artic asks for frames and wake-ups through a small handle" | `HostContext::request_frame` and `exit` on every call; `Waker` for other threads |
| The host drains the dispatcher | artic drains it in `woken` and after events |
| `PlatformHost` stubs that must not panic (#39's first text) | absent capabilities return `None` |
