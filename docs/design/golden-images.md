# Design: golden images

Issue: #38. Builds on: the `Renderer` trait and pixel targets (#34, [backend design](backends.md)), the [frame pipeline](frame.md) (#40), [elements](element.md) (#42), [shapes, strokes, clips and shadows](shapes.md) (#36). Unblocks: #53 (golden jobs in CI), #45 and #46 (control goldens), #121 (theme goldens), #171 (a second renderer).

A golden test draws a scene into memory and compares the pixels with a PNG checked into the repository. After this, a drawing change is checked by a machine, and an intended change shows up in the pull request as a changed picture.

```rust,ignore
#[test]
fn box_shadow() {
  with_renderer(|renderer| {
    let image = golden::render(renderer, &scenes::box_shadow(), scenes::SIZE, 1.0).expect("renders");
    golden!("box_shadow").assert(&image);
  });
}
```

## 1. Rendering into an image

The harness lives in `artic_test::golden` and draws through the `Renderer` trait into `Target::Pixels`, in `PixelFormat::Rgba8`. The renderer copies the pixels back and strips the row padding itself, so the harness never touches wgpu, and the same calls work for any renderer that accepts pixel targets in `Rgba8`. The trait lets a renderer draw only on window handles, or take pixel targets in only some formats; such a renderer can't have goldens, and `render` returns its `RenderError`, `Unsupported` or `UnsupportedFormat`.

```rust,ignore
pub struct Image { /* width: u32, height: u32, pixels: Vec<[u8; 4]> */ }

impl Image {
  pub fn new(width: u32, height: u32, pixels: Vec<[u8; 4]>) -> Image;
  pub fn load(path: &Path) -> io::Result<Image>;
  pub fn save(&self, path: &Path) -> io::Result<()>;
  pub fn pixel(&self, x: u32, y: u32) -> [u8; 4];
}

pub fn render(renderer: &mut impl Renderer, scene: &Scene, size: PhysicalSize, scale: f32) -> Result<Image, RenderError>;

pub fn render_tree(
  renderer: &mut impl Renderer,
  tree: &mut UiTree,
  root: NodeId,
  types: &ElementTypes,
  size: PhysicalSize,
  scale: f32,
) -> Result<Image, RenderError>;
```

An `Image` holds straight RGBA, row by row from the top, as `Rgba8` returns it and as PNG stores it. `new` panics when `pixels` doesn't hold `width × height` pixels, and `pixel` when `x` or `y` is outside the image, as slice indexing does. It reads and writes PNG with the `png` crate, which `artic-test` adds as a direct dependency in the workspace table at the version already in the lockfile, 0.18; no type of that crate appears in the API.

`render` takes a `Scene`, which is how the primitive goldens are written. `render_tree` takes a root element, lays it out and composes it with a fresh `Pipeline` from `artic-platform`, then renders the frame. It lays out with `Axes::default()`, sizing to content on neither axis, so the root is arranged in the image's logical size, `size / scale`, as in a window of that size; a golden's size is its test's choice, never the content's. It's the entry point for control goldens (#45, #46, #121). For it, `artic-test` adds `artic-element` as a direct dependency, for `ElementTypes`, beside the `artic-platform` it already has.

## 2. Goldens

```rust,ignore
#[non_exhaustive]
pub struct Tolerance { pub channel: u8, pub pixels: f32 }

impl Default for Tolerance { /* channel 8, pixels 0.001 */ }

impl Tolerance {
  pub fn with_channel(self, channel: u8) -> Tolerance;
  pub fn with_pixels(self, pixels: f32) -> Tolerance;
}

pub struct Golden { /* path, tolerance, per_os */ }

macro_rules! golden { ($name:expr) => { /* Golden::at(<calling crate>/tests/goldens/<name>.png) */ } }

impl Golden {
  pub fn at(path: impl Into<PathBuf>) -> Golden;
  pub fn with_tolerance(self, tolerance: Tolerance) -> Golden;
  pub fn per_os(self) -> Golden;
  pub fn check(&self, actual: &Image) -> Result<(), GoldenError>;
  pub fn assert(&self, actual: &Image);
}

#[non_exhaustive]
pub enum GoldenError {
  Missing { golden: PathBuf },
  Size { expected: (u32, u32), found: (u32, u32), actual: PathBuf },
  Pixels { differing: usize, allowed: usize, actual: PathBuf, diff: PathBuf },
  Io { path: PathBuf, error: io::Error },
}
```

`golden!("box_shadow")` is `tests/goldens/box_shadow.png` under the directory of the crate whose tests call it. It's a macro so that `env!("CARGO_MANIFEST_DIR")` expands in the calling crate and names that crate's directory; in a function of `artic-test` it would name `artic-test`. `Golden::at` takes any path. `per_os` adds the operating system, as `std::env::consts::OS` names it, to the file name: `box_shadow.linux.png`, for tests that draw with the system's fonts, which differ between systems. Text in the bundled test font draws the same on every system, so its goldens are shared.

`check` compares `actual` with the golden:

- A pixel differs when any channel of its color premultiplied by its alpha differs by more than `channel`. GPUs leave an arbitrary color in a fully clear pixel, so only what shows is compared.
- The check fails when more than `pixels`, a fraction of the image, differ, rounded down: 4 pixels of a 64×64 image by default. A `pixels` below 0 or NaN counts as 0, and one above 1 as 1. It also fails when the sizes differ, or when the golden is missing, with a message naming `ARTIC_UPDATE_GOLDENS`.
- When pixels differ it writes `<name>.actual.png` and `<name>.diff.png` beside the golden and returns them in `GoldenError::Pixels`, whose message prints both paths. When the sizes differ there's no pixel to pair, so it writes only `<name>.actual.png`, returned in `GoldenError::Size`. The diff shows matching pixels faded to gray and differing pixels in red. Every check removes the files it doesn't write, so neither outlives the run it belongs to.

A golden that can't be read or decoded, or an output that can't be written, is `GoldenError::Io` with the path; a PNG that isn't valid reads as an `io::Error` of kind `InvalidData`. `assert` panics with the error's message and its cause. `.gitignore` ignores `*.actual.png` and `*.diff.png`.

The default tolerance is a channel difference of 8 and 0.1% of the pixels. GPUs round anti-aliased edges and blurs differently by a few levels, which the channel limit absorbs. A wrong color changes a channel by far more than 8, and an edge one pixel off changes a whole row of pixels: 64, which is 1.6% of a 64×64 image. A test that needs another tolerance passes its own, built from the default: `Tolerance::default().with_channel(16)`.

## 3. Updating and skipping

Two environment variables, each on only when set to `1`:

| Variable | Effect |
| --- | --- |
| `ARTIC_UPDATE_GOLDENS=1` | `check` writes the image as the golden, removes any `.actual.png` and `.diff.png` beside it, and passes. Rerunning without it passes and changes no file. The new PNGs are reviewed in the pull request, where GitHub shows the old and new picture side by side |
| `ARTIC_REQUIRE_GPU=1` | a golden test that finds no usable GPU fails instead of skipping |

```rust,ignore
pub fn no_gpu(reason: impl Display);
```

Each renderer's tests skip when the renderer reports no usable GPU, in the helper that hands the tests their renderer. For vello, `Renderer::new` returns `Error::Adapter` when there's no adapter, and `Error::UnsupportedAdapter` when the one it found is WARP, the software DX12 adapter of Windows, on which vello's compute shaders crash. It tells WARP by a CPU adapter on the DX12 backend, so Mesa's lavapipe, a CPU adapter on Vulkan, counts as usable on Windows too. On either error the helper calls `no_gpu` with the reason instead of running the test. Only those two cases skip: a renderer that finds a usable adapter and then fails, opening the device or building vello's pipelines, is a broken renderer, and the helper panics with its error:

```rust,ignore
fn with_renderer(test: impl FnOnce(&mut Renderer)) {
  match Renderer::new() {
    Ok(mut renderer) => test(&mut renderer),
    Err(error @ (Error::Adapter(_) | Error::UnsupportedAdapter(_))) => golden::no_gpu(error),
    Err(error) => panic!("the renderer failed to start: {error}"),
  }
}
```

The tests share one renderer, as `pixels.rs` does; the sample leaves that out.

`no_gpu` prints `skipped: <reason>` and returns, or panics when `ARTIC_REQUIRE_GPU=1` is set. A test can't mark itself skipped at run time, so a skipped golden test passes with that line in its output.

The variable is ours rather than the `CI` most CI services set, because a CI machine without a GPU is normal today: GitHub's Windows runners have only WARP, and its Ubuntu runners no adapter. This design changes #53's plan, which ran Windows goldens on WARP:

- Ubuntu installs lavapipe and sets `ARTIC_REQUIRE_GPU=1`.
- Windows installs Mesa's lavapipe for Windows (mesa-dist-win) and sets `WGPU_BACKEND=vulkan`, which `artic-vello` already reads through `Backends::from_env` in place of its DX12 default, and `ARTIC_REQUIRE_GPU=1`. If vello can't run on lavapipe there, #53 drops Windows from its golden jobs and says so in its acceptance, rather than keeping a job that skips every golden and passes.
- macOS sets `ARTIC_REQUIRE_GPU=1`; GitHub's macOS runners have a Metal device.

## 4. The primitive goldens

`artic_test::scenes` builds one scene per primitive of #36, each drawn at `scenes::SIZE`, a `PhysicalSize` of 64×64, at scale 1:

| Scene | Draws |
| --- | --- |
| `rounded_rect_fill` | a rounded rectangle with four different radii |
| `stroke` | a stroked star with round joins |
| `path` | a filled star, even-odd |
| `linear_gradient` | a rectangle with a diagonal linear gradient of three stops |
| `radial_gradient` | an ellipse's radial gradient |
| `box_shadow` | a translucent card over a blurred shadow with spread |
| `rect_clip`, `rounded_rect_clip`, `path_clip` | a gradient clipped by each kind of shape |
| `nested_opacity` | two overlapping rectangles in nested opacity layers |

The scenes belong to `artic-test` and the goldens to each renderer: `artic-vello/tests/goldens.rs` has one test per scene, sharing one renderer as `pixels.rs` does, with its PNGs in `artic-vello/tests/goldens/`. A second renderer (#171) adds its own test file over the same scenes and its own PNGs, because two rasterizers don't agree to the pixel.

The exact pixel tests in `artic-vello/tests/pixels.rs` stay. They check rules a picture with a tolerance would let through, such as where a blur's edge falls to the half pixel, and when one fails it names the pixel and the rule.

## 5. What waits

| Feature | How it fits | Tracked in |
| --- | --- | --- |
| A CI job that runs the goldens on lavapipe, and uploads `.actual.png` and `.diff.png` | it sets `ARTIC_REQUIRE_GPU=1` and, from the workspace root, uploads `crates/*/tests/goldens/*.actual.png` and `crates/*/tests/goldens/*.diff.png`, since each crate keeps its own goldens. Wayland is covered by #53's separate smoke job, since goldens draw into memory, not on a window | #53 |
| Control goldens | `render_tree` with the control's tree | #45, #46, #47 |
| Theme state goldens | the same, once per state | #121 |
| Text goldens | shared, since the bundled test font draws the same on every system; `per_os` waits for a golden drawn with system fonts | #37, #46 |

## 6. Testing

| Crate | Tests |
| --- | --- |
| `artic-test` | `identical_images_match`, `small_differences_within_tolerance_match`, `too_many_differing_pixels_fail`, `different_sizes_fail`, `png_round_trips`, `golden_mismatch_writes_diff`, `passing_check_removes_old_outputs`, `missing_golden_says_how_to_create_it`, `unreadable_golden_is_an_io_error`, `updating_twice_changes_no_bytes`, `render_reads_the_renderers_pixels` and `render_tree_draws_the_root` (both with a renderer that fills the buffer with one color) |
| `artic-vello` goldens | one test per scene in section 4 |

The file tests write into a temporary directory through `Golden::at`, and take the update mode as a value instead of setting the variable: `set_var` is `unsafe` in edition 2024 because tests run on parallel threads. The first run of the vello goldens creates the PNGs with `ARTIC_UPDATE_GOLDENS=1`, and each is checked by eye before it's committed.

## 7. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | this document | approved |
| B | `artic_test::golden`, the `golden!` macro and `artic_test::scenes`, with rustdoc and an example; the vello goldens and their PNGs; the WARP check in the vello tests moved to the DX12 backend, and their helper skipping only on `Error::Adapter` and WARP; `png` and `artic-element` as dependencies of `artic-test`; the `.gitignore` entries | the tests in section 6 pass, and updating then rerunning leaves `git status` clean |
