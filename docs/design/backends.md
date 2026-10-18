# Design: backend seams

Issue: #240. Implemented by: #33 (renderer and scene), #39 (desktop host), #40 (frame pipeline), #34 (`vello` renderer). Related: #241 (the type registry on targets without `linkme`), D24 (web without app changes).

artic must run on targets nobody has listed yet. Anyone can write an **adapter** for a new backend without changing app code or artic code: a browser, a Raspberry Pi framebuffer, a VideoCore VI GPU with no window system, skia, SDL, GLFW, a handheld like the ROG Ally. This document fixes the seams adapters implement, keeping them as small as the targets allow.

The design is tested against the targets that differ most from the default desktop:

| Target | What it lacks or adds |
| --- | --- |
| Browser | a loop that can't block; async clipboard and dialogs |
| Raspberry Pi framebuffer (`/dev/fb0`) | a GPU, a window system; input comes from evdev |
| VideoCore VI (KMS/DRM) | compute shaders (so no `vello`), a window system |
| SDL or GLFW with skia | a renderer other than `vello` under a host other than `winit` |
| ROG Ally | a gamepad as the main input; directional focus |
| Kiosk with two outputs, desktop with mixed-DPI monitors | several displays, each with its own scale and refresh rate |

## 1. Roles

Two roles, combined freely:

- A **`Host`** drives the screen: surfaces, input and the event loop.
- A **`Renderer`** turns a `Scene` into pixels.

An adapter crate implements one role or both. A skia renderer written once works under SDL, GLFW or `winit`; a framebuffer adapter is both in one crate.

artic owns everything that decides something: layout, the scene build, text shaping, focus and navigation, and frame pacing. Adapters only **report facts** (input, sizes, scales, frame times) and **carry out requests** (draw this scene, present it, show the keyboard).

## 2. The host

### Core

Every target provides four things:

1. **Surfaces.** At least one drawable area. Each reports `SurfaceInfo { size, scale, display, refresh_rate }` when it appears and whenever any of these change (a window dragged to another monitor), and reports when it's lost (mobile suspend, a closed window).
2. **Events.** Input and lifecycle (section 5), delivered to artic.
3. **Frames, per surface.** artic asks for a frame on a surface; the host calls back when that surface should draw (vsync, a browser animation frame, a timer). A 144 Hz display and a 60 Hz display each get their own frames.
4. **Waking.** Another thread posts work to the `Dispatcher` (#16), and the host wakes its loop.

```rust,ignore
pub trait Host {
  /// What kind of target the host offers, checked against the renderer before the loop starts.
  fn targets(&self) -> TargetKind;

  /// Runs the loop, reporting surfaces, events and frame times to `app` until it quits. A host
  /// whose loop can't block (a browser) installs its callbacks and returns at once.
  fn run(self, app: impl AppHandler + 'static);
}
```

`AppHandler` is artic's side of the loop. The host calls its `started`, `woken`, `surface_changed`, `surface_lost`, `event` and `frame`, which carries the frame's time, each with a `HostContext` through which artic asks for frames, exits and reaches the capabilities; other threads wake the loop through a `Waker`. The [host design](host.md) (#39) fixes these traits; it moved the capabilities from `Host` to `HostContext`, since `run` consumes the host.

### Capabilities

Everything beyond the core is optional. Each capability is a `HostContext` method whose default returns `None`, so:

- a framebuffer adapter implements `run` and nothing else;
- adding a capability later is a new default method, and no existing adapter breaks;
- artic must decide, at each call site, what to do without the capability. Each capability documents its fallback.

| Capability | What it does | Without it |
| --- | --- | --- |
| `windows` | open, update and close (#39, #48); full screen and choosing a display are deferred to #267 | each `Surface` takes one whole host surface (below) |
| `displays` | list displays (id, bounds, scale, refresh rate, primary); connect and disconnect | everything goes on the host's surfaces in order |
| `clipboard` | read (async) and write text | Copy does nothing; Paste is disabled |
| `text_input` | IME composition, soft keyboard, input mode (`InputMode`) | text comes only from `Key` and `Text` events |
| `cursor` | cursor shape | the platform default |
| `accessibility` | hands the `accesskit` tree to the OS | no screen reader support |
| color scheme, safe area, keyboard inset | OS settings and insets | light scheme; no insets |
| file dialogs, menus, haptics, `open_url` | OS services | the call reports "not available" |

### Several displays

| Setup | What the host reports |
| --- | --- |
| Windowed, such as a laptop at 2× with a monitor at 1× | one surface per window; a surface moves between displays, and its scale and refresh rate change with it |
| Not windowed, such as a kiosk with two HDMI outputs | one full-screen surface per display, appearing and disappearing with the outputs |

The app declares its `Surface`s the same way for both ([surface design](surface.md)). artic assigns each one: on a host with `windows`, each open `Surface` is an OS window, on the primary display until `Desktop.Display` (#267) names another; on a host without it, each open `Surface` takes one display's surface, in tree order, and one with no surface left stays unbound. A two-screen kiosk app runs unchanged on a Pi with two outputs and on a desktop with two monitors.

## 3. The surface handoff

A host offers each surface as a `Target`, split by how a renderer gets pixels onto it:

```rust,ignore
pub enum Target<'a> {
  /// Native handles (`raw-window-handle`): OS windows, DRM/GBM screens, browser canvases. The
  /// renderer creates its swapchain from them and presents. They stay valid until the host reports
  /// the surface lost, so a swapchain may outlive a frame.
  Handles(&'a dyn RawHandles),
  /// A CPU pixel buffer (data, stride, format; the size is the surface's): framebuffers, offscreen
  /// tests, `putImageData`. The renderer fills it; the host presents it.
  Pixels(PixelBuffer<'a>),
  /// Anything else, for a host and renderer from one adapter crate that agree on the type.
  Custom(&'a mut dyn Any),
}
```

- **Window versus screen is not part of `Target`.** KMS/DRM is a screen drawn through handles, and a browser canvas sits in a window and can be drawn through pixels. Placement reads `SurfaceInfo` and the capabilities instead.
- **`raw-window-handle`** is the ecosystem's standard: `wgpu`, skia, `glutin` and `softbuffer` accept it, and new platforms reach artic through it without artic changing.
- **`Custom`** keeps the set open: a VideoCore adapter can pass its own display handle to its own renderer.
- **Exactly one side presents:** the renderer for `Handles`, the host for `Pixels`, and the adapter's own agreement for `Custom`.
- **A mismatch is a startup error, not a crash:** "renderer `skia-gl` can't draw on host `fbdev`, which offers `Pixels`; use a CPU renderer such as `tiny-skia`".

## 4. The renderer and the scene

```rust,ignore
pub trait Renderer {
  /// Whether it can draw on targets of `kind`; `artic::run` asks before the host starts.
  fn accepts(&self, kind: TargetKind) -> bool;

  /// Draws `scene` on `surface`.
  fn render(&mut self, scene: &Scene, surface: SurfaceView<'_>) -> Result<(), RenderError>;

  /// Releases what the renderer holds for a surface that's gone, such as its swapchain.
  fn surface_lost(&mut self, surface: SurfaceId) {}
}

/// One surface to draw on this frame.
pub struct SurfaceView<'a> {
  pub id: SurfaceId,        // which surface: a renderer keeps a swapchain per surface
  pub size: PhysicalSize,   // in device pixels: a swapchain needs it, and handles don't carry it
  pub scale: f32,           // device pixels per logical unit
  pub target: Target<'a>,
}
```

(First named `Surface`; the [surface design](surface.md) (#48) renamed it `SurfaceView`, freeing the name for the element a host presents.)

(Implementing #33 refined the first sketch, `render(&Scene, Target, scale)`: a renderer with several windows needs to know which surface it draws on and at what size, and `artic::run`'s pairing check needs `accepts`.)

- **Text is shaped by artic,** with `parley`, in its own crate. Layout measures text through it, and the scene carries **positioned glyph runs**: font data, glyph ids, positions and size. A renderer draws glyphs; it never shapes, breaks lines or measures. vello and skia draw glyph runs from font data directly, and a CPU renderer gets outlines from a `skrifa` helper artic provides. (The previous spec's `Renderer::measure_text` is gone, and layout no longer depends on which renderer runs.)
- **Images are shared data** (`Arc<ImageData>`) with a stable identity. A renderer uploads an image the first time it sees it and caches it. (`Renderer::create_image` is gone.)
- **The scene is retained:** one fragment per node, composed by the root with transforms, clips and opacity layers. Each fragment has an id and a generation that increases when it changes, so a renderer may cache its encoding of unchanged fragments. A simple renderer can redraw everything every frame and still be correct.
- **Every primitive is required:** paths (fill and stroke), rounded rectangles, images, glyph runs, linear and radial gradients, box shadows, clips (rectangle, rounded rectangle, path) and opacity layers. All of them can be drawn on a CPU, so a `tiny-skia` renderer on a framebuffer is complete, and the golden-image tests (#38) can run every renderer against the same scenes.

## 5. Input

Hosts report raw events in a vocabulary artic owns, taken from web standards because they're the most widely known neutral names:

| Event | Contents | Vocabulary |
| --- | --- | --- |
| `Pointer` | id, kind (`Mouse`, `Touch`, `Pen`), phase (`Down`, `Move`, `Up`, `Cancel`, `Enter`, `Leave`), position, buttons, pressure, modifiers | W3C Pointer Events |
| `Wheel` | delta in lines or pixels, position, modifiers | DOM `WheelEvent` |
| `Key` | `key` (logical, "a", "Enter"), `code` (physical, "KeyA"), pressed or released, repeat, modifiers | W3C UI Events |
| `Text`, `Ime` | committed text; preedit text and its cursor | |
| `Gamepad` | device; a button (`South`, `East`, `DPadUp`, …) or an axis and its value; connected and disconnected | W3C Standard Gamepad |
| Lifecycle | `CloseRequested`, `Suspended`, `Resumed`, `Back`, color scheme changed, memory warning | |

Positions arrive in the surface's **physical** pixels, as the platform gives them; artic divides by the surface's scale.

artic maps raw events to **actions**, so every adapter gets them:

| Action | Default keys | Default gamepad |
| --- | --- | --- |
| `Next`, `Previous` | Tab, Shift+Tab | shoulder buttons |
| `Up`, `Down`, `Left`, `Right` | arrow keys | D-pad, left stick past a threshold |
| `Activate` | Enter, Space | South (A) |
| `Cancel` | Escape | East (B), Android `Back` |
| `Menu` | the context-menu key | Start |

- **Directional focus lives in artic.** `Up`, `Down`, `Left` and `Right` move focus to the nearest focusable element in that direction, from layout geometry, as WinUI's XY focus and the web's spatial navigation do.
- **Input mode is tracked by artic,** not reported by the host: the most recently used kind of input decides hover states (mouse), enlarged hit targets (touch, `TouchInflate`) and the focus ring (keys and gamepad). The previous spec's `primary_input()` is gone.

## 6. Launchers and the event loop

The app is a library; each target has a **launcher**, a few lines that name a host, a renderer and the app:

```rust,ignore
// The app crate (a library), the same for every target.
pub fn app() -> artic::App { … }

// launchers/desktop/src/main.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
  artic::run(artic_winit::Host::new()?, artic_vello::Renderer::new()?, my_app::app());
  Ok(())
}

// launchers/pi/src/main.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
  artic::run(artic_fbdev::Host::new()?, artic_tiny_skia::Renderer::new()?, my_app::app());
  Ok(())
}
```

- **Entry points differ per platform anyway:** `main` on desktop, `android_main` on Android, a `wasm_bindgen` start function in a browser. The launcher is the one place that's platform-specific; the app crate and artic never change. `cargo artic new` generates the common launchers.
- **`artic::run` is generic** over `H: Host` and `R: Renderer`, so the common case uses no trait objects. It checks that the renderer accepts what the host offers (section 3) before starting the host.
- **The loop is the host's.** A desktop host blocks inside `run` until the app quits. A browser host installs its callbacks and returns, and the callbacks keep driving the `App`. App code can't tell the difference (D24).
- **No link-time registration of backends.** `linkme` doesn't support wasm, and it couldn't produce `android_main` either. The type registry has the same `linkme` gap on wasm; that is #241.

## Crates (new in this document)

The seams live in crates with no backend dependencies, and each backend is an adapter crate on top:

| Crate | Holds | Doesn't depend on |
| --- | --- | --- |
| `artic-render` | `Renderer`, `Scene`, `Target`, `PixelBuffer` | `vello`, `wgpu`, skia |
| `artic-text` | text shaping (`parley`) and fonts (`fontique`) | any renderer |
| `artic-platform` | `Host`, `AppHandler` and `HostContext` with its capabilities, `artic::run`'s pairing check, the frame pipeline | `winit`, `muda`, `rfd`, `arboard` |
| `artic-core` | raw input event types (controls see them without the platform, as today) | |
| `artic-element` | the action mapping and directional focus, next to focus navigation | |
| **`artic-winit`** (adapter) | the desktop `Host`: `winit`, `muda`, `rfd`, `arboard`, `accesskit_winit` | |
| **`artic-vello`** (adapter) | the default `Renderer`: `vello` on `wgpu` | |

Compared with today's §2 and `docs/architecture.md`:

- `vello` moves out of `artic-render` into `artic-vello`, and `winit` and the OS crates move out of `artic-platform` into `artic-winit`. #34 builds `artic-vello`, #39 builds `artic-winit`.
- The facade's `skia` feature goes away: a skia renderer is an adapter crate like any other.
- The rule "text measurement reaches layout through a small trait implemented by the renderer" becomes "implemented by `artic-text`", the same for every renderer ([text design](text.md)).
- `artic-test`'s `TestHost` implements `Host`, and a test renderer draws to `Pixels`, so artic's own tests exercise the seams the way third-party adapters do.

## What this changes

| Before | After |
| --- | --- |
| `PlatformHost` with 14 required methods | `Host`: `targets` and `run`; capabilities on `HostContext`, defaulting to `None` |
| `Renderer::begin_frame`, `render`, `resize`, `create_image`, `measure_text` | `Renderer::accepts`, `render(&Scene, SurfaceView)` and `surface_lost` |
| A "backend-neutral surface handle" | `Target::Handles`, `Pixels` or `Custom` |
| `primary_input()` from the host | input mode tracked by artic |
| `Focus` moves in `Tab` order | actions, with directional focus from geometry |
| `vello` in `artic-render`, `winit` in `artic-platform` | the adapter crates `artic-vello` and `artic-winit` |
| `App::run()` picks the backend | launchers call `artic::run(host, renderer, app)` |

## Left open

- **Damage regions.** Redrawing only changed areas of a surface is an optimization a `Pixels` renderer may want; it can be added to `render` later without changing adapters that ignore it.
- **The type registry without `linkme`** (#241).
