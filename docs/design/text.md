# Design: Text shaping and fonts

Issue: #37. Builds on: [backends](backends.md) (#33), [brushes](brush.md) (#35), [shapes, strokes, clips and shadows](shapes.md) (#36), [golden images](golden-images.md) (#38), [layout tests](layout-tests.md) (#56), [frames](frame.md). Unblocks: #46, #47, #175, #171.

Text goes from a string, a font family list and a width to positioned glyphs broken into lines, with a size layout can use. A control shapes its text while it's measured, lets layout keep the result, and draws that same result when it records its scene, so text is shaped once per change and never by a renderer.

```rust
let paragraph = cx.text().paragraph("Hello, world", &style, TextLimits::wrap(200.0));
// later, in render:
scene.text(&paragraph, Point::ZERO, foreground);
```

The issue predates the [backends design](backends.md), which removed `Renderer::measure_text` and moved shaping out of the renderer. This document replaces the issue's entry point, its "renderer owns the contexts" note and its glyph cache; section 8 lists the acceptance criteria that change.

## 1. Crates

`parley` shapes and breaks lines, and `fontique` lists the system's fonts and maps family names to them. Both live in a new crate, `artic-text`, so no other crate compiles them. A renderer draws glyph runs from font data and never sees `parley`.

| Crate | Gains | Depends on |
| --- | --- | --- |
| `artic-layout` | the `TextEngine` trait and the value types of section 2; `text()` on `MeasureCx` and `ArrangeCx`, `set_measured` and `set_arranged`; `PixelGrid`; an engine argument to `LayoutManager::run` and `measure` | unchanged |
| `artic-element` | `RenderCx::layout_result` | tests dev-depend on `artic-text` and `artic-test-fonts` |
| `artic-text` (new) | `Engine`, which implements `TextEngine` with `parley` and `fontique` | `artic-layout` |
| `artic-render` | `Command::Text` and `SceneBuilder::text` | unchanged |
| `artic-platform` | the runner owns one engine and lends it to each surface's `Pipeline::prepare` and `Pipeline::measure` | tests dev-depend on `artic-text` and `artic-test-fonts` |
| `artic-vello` | draws `Command::Text` | unchanged |
| `artic-test-fonts` (new) | the test font's files and licence, as `pub const` byte slices | none |
| `artic-test`, `artic` | create the engine: the test font's and the system's | add `artic-text`; `artic-test` also adds `artic-test-fonts` |
| `artic-winit` | the `frames` example creates `Engine::system()` | a dev-dependency on `artic-text` |

`artic-text` sits in layer 2, beside `artic-render`. The data it produces, `Paragraph`, is defined in `artic-layout`, so neither `artic-text` nor a renderer depends on the other: both speak `artic-layout`'s types. The runner takes the engine as a `Box<dyn TextEngine>`, so `artic-platform` doesn't depend on `artic-text` either; whatever builds the runner, the facade or an example, passes `artic_text::Engine::system()`. The engine isn't a backend an app picks: shaping is artic's and the same for every host and renderer, so `artic::run(host, renderer, app)` keeps its arguments and the facade creates the engine itself.

`artic-text`'s `complex-scripts` feature, on by default, turns on `parley`'s dictionaries for breaking lines in scripts written without spaces between words, such as Thai, Lao, Khmer and Myanmar. It adds about 3.6 MB to a release binary; without it, those scripts break only at spaces and hard line breaks.

## 2. Value types

In `artic-layout`, beside `Thickness` and `Path`:

```rust
pub struct FontFamily(Arc<[FamilyName]>);
pub enum FamilyName { Named(Arc<str>), Generic(GenericFamily) }
pub enum GenericFamily { Serif, SansSerif, Monospace, Cursive, Fantasy, SystemUi, Emoji, Math }
pub struct FontWeight(u16);
pub enum FontStyle { Normal, Italic, Oblique }

pub struct TextStyle {
  pub family: FontFamily,
  pub size: f32,
  pub weight: FontWeight,
  pub style: FontStyle,
  pub line_height: Option<f32>,
}

pub struct TextLimits {
  pub max_width: Option<f32>,
  pub max_lines: Option<NonZeroU32>,
}

pub struct Paragraph { size: Size, lines: Arc<[Line]>, runs: Arc<[GlyphRun]> }
pub struct Line { pub top: f32, pub height: f32, pub baseline: f32, pub width: f32 }
pub struct GlyphRun { font: FontData, size: f32, coords: Arc<[i16]>, synthesis: Synthesis, glyphs: Arc<[Glyph]> }
pub struct Synthesis { pub embolden: bool, pub skew: f32 }
pub struct Glyph { pub id: u32, pub x: f32, pub y: f32 }
pub struct FontData { bytes: Arc<dyn AsRef<[u8]> + Send + Sync>, index: u32, id: u64 }
```

- `FontFamily: FromStr` reads a CSS family list: names separated by commas, each quoted with `'` or `"` or not. An unquoted name equal to a generic keyword (`serif`, `sans-serif`, `monospace`, `cursive`, `fantasy`, `system-ui`, `emoji`, `math`), ignoring case, is that generic family; a quoted one is a font named so. The default, which `TextBlock` (#46) inherits, is `system-ui`.
- `FontWeight` is 1 to 1000, as in CSS. `FromStr` takes a number or one of WPF's `FontWeights` names, ignoring case: `Thin` 100, `ExtraLight` and `UltraLight` 200, `Light` 300, `Normal` and `Regular` 400, `Medium` 500, `SemiBold` and `DemiBold` 600, `Bold` 700, `ExtraBold` and `UltraBold` 800, `Black` and `Heavy` 900, `ExtraBlack` and `UltraBlack` 950. `FontStyle` is a `XamlEnum`, as `Orientation` is.
- `line_height: None` uses the font's own line spacing. `max_width: None` doesn't wrap, and `max_lines: None` keeps every line. `TextLimits::wrap(width)` and `TextLimits::NONE` are the two common values.
- `Paragraph`, `GlyphRun` and `FontData` keep their fields private and expose them through read-only accessors (`size()`, `lines()`, `runs()`, `font()`, `size()`, `coords()`, `synthesis()`, `glyphs()`, `bytes()`, `index()`, `id()`), and are built through `new` functions, so an engine outside artic can build them too and a renderer can't change a recorded paragraph.
- A `Paragraph` is cheap to clone: its lines and runs are behind `Arc`s. Glyph positions are in logical units, relative to the paragraph's top-left corner, with `y` on the glyph's baseline. Nothing is snapped to device pixels.
- `coords` are the run's variable-font axis positions, in `F2Dot14` form, so a weight drawn from a variable font comes out at that weight.
- `synthesis` is what the font lacks and the renderer fakes: `embolden` when a bold weight was asked of a family without a bold face, and `skew`, in degrees, when italic was asked of one without an italic face. `fontique` reports both when it picks the face.
- `FontData::new(bytes, index)` takes a new `id` from a process-wide counter. Two `FontData` are equal when their `id` and `index` are, never by comparing bytes, so `Paragraph: PartialEq` stays cheap and two recordings of the same text compare equal. `FontData`'s `Debug` is written by hand and shows the id, index and length, since the bytes have no `Debug` of their own. The bytes are the same type `peniko::Blob` holds, so a font `fontique` mapped from disk reaches vello without a copy.

Errors follow `ParseBoxShadowError`: the text and a problem enum.

```rust
pub struct ParseFontFamilyError { pub text: String, pub problem: FamilyProblem }
pub enum FamilyProblem { Empty, EmptyName, UnclosedQuote, PartlyQuoted(String) }

pub struct ParseFontWeightError { pub text: String, pub problem: WeightProblem }
pub enum WeightProblem { UnknownName, OutOfRange }
```

`"Inter,,Arial"` is `EmptyName`, `"'Segoe UI"` is `UnclosedQuote`, and `"'Segoe' UI"` is `PartlyQuoted`: a name is quoted whole or not at all. Messages quote the text and show the forms: `invalid FontFamily '': it's empty; expected forms like 'Inter, sans-serif' or 'system-ui'`.

## 3. The `TextEngine` trait

```rust
pub trait TextEngine {
  /// Shapes `text` in `style` and breaks it into lines within `limits`.
  fn paragraph(&mut self, text: &str, style: &TextStyle, limits: TextLimits) -> Paragraph;
}

impl MeasureCx<'_> {
  pub fn text(&mut self) -> &mut dyn TextEngine;
  /// Keeps what this measure produced, beside the size it returns.
  pub fn set_measured<T: Any>(&mut self, result: T);
}

impl ArrangeCx<'_> {
  pub fn text(&mut self) -> &mut dyn TextEngine;
  /// Keeps what this arrange produced for the final size, in place of the measured result.
  pub fn set_arranged<T: Any>(&mut self, result: T);
  /// The arranged result the node holds for its current size, if it set one.
  pub fn arranged<T: Any>(&self) -> Option<&T>;
}
```

The name says what it is, a text engine, and the method names what it returns. "Shaper" would cover only part of the job, "typesetter" reads as something about types in Rust, and "layout" already means measuring and arranging elements in this crate.

### What layout keeps

`Layoutable` already says that what layout works out is kept by the `LayoutManager`, not by the element. So layout keeps a node's results:

- **Measured:** what the node's last measure produced, beside its desired size. A `TextBlock` shapes its text in `measure`, calls `cx.set_measured(paragraph.clone())`, and returns the paragraph's size plus its padding. The next measure replaces it, and a measure that doesn't call `set_measured` drops it, so it's never stale while the size is current.
- **Arranged:** what the node's arrange produced for its final size, when that differs from what it measured. A parent can arrange a child in less room than it asked for, as a `Grid` column narrower than its content does. Then `TextBlock` shapes its text again at the width it got and calls `cx.set_arranged(paragraph)`, as WPF's and Avalonia's `TextBlock.ArrangeOverride` do.

Each result is kept until what it was computed from changes. The measured result lasts until the node's next measure runs; one answered from the cache keeps it. The arranged result lasts until anything but the node's position changes: a new measure, another offered size, a new `PixelGrid` or `invalidate_arrange` drops it, since `TextBlock`'s choice to shape again depends on the scale. An arrange that only moves the node keeps it, and `ArrangeCx::arranged::<T>()` reads it back, so a `TextBlock` that already holds a paragraph for its width doesn't shape again. A measure that runs without setting its result drops the old one, and so does any arrange but a move.

`RenderCx::layout_result::<Paragraph>()` hands back the arranged result if there is one, and the measured one otherwise. A control reads back only the type it set, so the `Any` downcast stays inside one control. Both results are dropped with the node's slot: in the `retain` that forgets nodes outside the root, in `forget_unmeasured`, and when a collapsed root clears its subtree. They live in their own maps beside `sizes`, since `Slot` is `Copy`. `LayoutManager::result(node) -> Option<&dyn Any>` reads them, and the pipeline passes it to `RenderCx::new`, which stays `Copy`.

Measure's cache is untouched: `Slot.available` and `desired` still say what the parent asked and got, so a change in a child still reaches its parent as before.

A result that is set or dropped changes what the node draws, so `LayoutManager` collects those nodes, in `run` and in `measure` alike, and the next `run` hands them over in `LayoutStats::results`. The pipeline adds them to the nodes the compositor records again, as it does for `resized`. A result set by a `measure` call outside `run`, such as the runner's measure of a content-sized window, is reported by the following `run`, so a new paragraph is never drawn with an old fragment. A node that only moved sets nothing and stays as recorded.

Arrange receives a snapped rectangle, so a width can lose up to a device pixel to snapping. `TextBlock` shapes again only when it got less than the paragraph's width by more than one device pixel (`1.0 / cx.scale()`), so width lost to snapping alone never makes text wrap; the overhang of up to one device pixel isn't clipped. Alignment is an input to shaping: the engine aligns each line against the paragraph's widest line, so a box wider than the paragraph needs no shaping and `TextBlock` places the whole paragraph in it. Lines centered among themselves and a paragraph centered in its box put every line in the center of the box; the same holds for the end, and right-to-left lines come out of the engine already placed. `RenderCx` doesn't have the engine: render draws what layout kept.

### Reaching the engine

```rust
pub struct PixelGrid { pub scale: f32, pub snap: bool }

pub fn run(&mut self, tree: &mut UiTree, nodes: &impl LayoutNodes, root: NodeId, available: Size,
           grid: PixelGrid, text: &mut dyn TextEngine) -> LayoutStats;
pub fn measure(&mut self, tree: &mut UiTree, nodes: &impl LayoutNodes, root: NodeId, available: Size,
               text: &mut dyn TextEngine) -> Size;
```

`PixelGrid` names what `LayoutManager` already keeps privately as `grid: (f32, bool)`, the device-pixel grid arranged rectangles land on. Only arranging uses the grid, so `run`, which arranges, takes it, and `measure`, which only measures, doesn't; both take the engine. `Pass` holds the `&mut dyn TextEngine` beside its other borrows, so no control can keep the engine past the pass. Layout runs on the UI thread, so the engine needn't be `Send`, and `parley`'s contexts stay plain `&mut` state.

The runner owns a single engine that every surface shares, so system fonts are listed once and a font keeps one identity in every window. It lends that same engine to each surface's `Pipeline::prepare`, which passes it to `run`, and to `Pipeline::measure`, which it calls before opening a window its content sizes, and which passes it to `LayoutManager::measure`. Both take it in `FrameServices`, beside the `ElementTypes` table: the values the runner lends every surface. `Runner::new` takes the engine after the renderer, as a `Box<dyn TextEngine>`: the runner is generic over its renderer, but layout takes the engine as `&mut dyn TextEngine` anyway, so a type parameter would buy nothing. `artic-test`'s headless layout helper and `render_tree` use the test engine of section 7.

Text doesn't change the rule that a desired size doesn't depend on the display: shaping runs at scale 1. Hinting, which would make glyph advances depend on the scale, stays out of measure (section 9).

## 4. The engine

In `artic-text`:

```rust
pub struct Engine { /* parley::FontContext, parley::LayoutContext, its font table, its default family */ }

impl Engine {
  /// An engine that finds the system's fonts. Its default family is `system-ui`.
  pub fn system() -> Engine;
  /// An engine that knows only `fonts`, with every generic family pointing at them, in order.
  /// Its default family is the first font's.
  pub fn with_fonts(fonts: impl IntoIterator<Item = Arc<dyn AsRef<[u8]> + Send + Sync>>) -> Engine;
}

impl TextEngine for Engine { /* … */ }
```

`paragraph`:

1. builds a `parley` layout at display scale 1, without quantizing, so its numbers are logical units. The family list, followed by the engine's default family, becomes a `parley` font stack; size, weight and style carry over; `line_height: None` is the font's metrics (`MetricsRelative(1.0)`);
2. breaks it with `break_all_lines(max_width)` and keeps the first `max_lines` lines. The size is the widest kept line by the sum of the kept lines' heights;
3. places the kept lines against that width itself. A line is the visual span of its clusters, except the whitespace that ends its text, which hangs: it takes no width and draws nothing. In a left-to-right paragraph every line's span starts at 0, and in a right-to-left one every line's span ends at the paragraph's width, whatever direction the line's own text runs. The paragraph's direction comes from its first strong character, as HTML's `dir="auto"` does. `parley`'s `align` isn't used: it aligns against `max_width` rather than the widest kept line, and it measures trailing whitespace wrongly in right-to-left runs. `parley` reorders mixed-direction runs. The alignment field that #46 adds to `TextStyle` moves each span by its free width, the paragraph's width minus its own, times 0, ½ or 1;
4. turns each `parley` glyph run into a `GlyphRun`, with its synthesis, looking its face up in the engine's font table by the blob's id and the face's index, so one face always has one `FontData` and one `id` for the engine's life. The table holds only the faces of the engine's own collection. Faces of a collection file (`.ttc`) share a blob and differ by index.

It never panics and never fails:

| Input | Result |
| --- | --- |
| A family that isn't installed | the next name in the list, then the engine's default family, then `fontique`'s fallback for the text's script |
| A weight or style the family has no face for | the nearest face, with `synthesis` set |
| Empty text | width 0, and the height of one line, as an empty WPF `TextBlock` |
| A size that isn't finite or isn't above 0, or no font at all | an empty paragraph of size 0 |
| A `max_width` that is NaN or infinite | no wrapping |
| A negative `max_width` | 0, so the text breaks at every opportunity |

The default family is the engine's policy, as the default font is a browser's setting rather than part of a page's `font-family`; `FontFamily` knows nothing about it. `system()`'s, `system-ui`, is mapped to a real font by `fontique`, per desktop, so artic keeps no font list of its own and a new OS release changes nothing here. `with_fonts` turns system fonts off, points every generic family at the given fonts, and falls back to the first, so an unknown name such as `"Nope"` lands on them too: in tests every family resolves and nothing depends on what the machine has installed. `Engine`'s `Debug` is written by hand, since `parley`'s contexts have none worth printing.

## 5. The scene

In `artic-render`:

```rust
pub enum Command {
  // …
  /// Draws `paragraph`'s glyphs with its top-left corner at `origin`, painted with `brush`.
  Text { paragraph: Paragraph, origin: Point, brush: Brush },
}

impl SceneBuilder {
  pub fn text(&mut self, paragraph: &Paragraph, origin: Point, brush: impl Into<Brush>);
}
```

One command draws a whole paragraph. Any `Brush` works, and its relative points and radii resolve against `Rect::new(origin, paragraph.size())`, as a `Fill` resolves against its shape's bounds, so a gradient runs across the whole text rather than restarting with each run. A paragraph without runs records nothing.

A brush per span of text (inline `Run`s, #47) can extend this later with a brush per run, without changing the command's shape.

## 6. Drawing glyphs

What a renderer may rely on is the same for fonts as for images in the [backends design](backends.md): shared data with a stable identity. A `FontData`'s `id` is unique and stays the same for as long as its bytes live, so a renderer may cache whatever it derives from a font (a typeface, outlines, an atlas) keyed by `id` and `index`, and decides itself when to drop it. artic's types carry nothing for any one renderer.

`artic-vello` draws each run with one `draw_glyphs` call: the run's size and `coords`, the brush, and the current transform, with the glyphs moved by `origin`, so the brush resolves against the paragraph's box as a fill's does against its shape's. `synthesis` becomes vello's `font_embolden` and a skewing `glyph_transform`. Hinting is off, since it depends on the device-pixel grid, which text doesn't snap to yet.

vello 0.11's renderer already caches glyph outlines between frames, keyed by the font blob's id, its index, the size and the variation coordinates, and drops entries left unused for 64 frames, checking every 64 frames or sooner once it holds more than 256 glyphs. So `artic-vello` keeps no cache and no map: it builds each run's blob with `Blob::from_raw_parts(bytes, id)` from the `FontData`, which clones an `Arc`, and the same id hits vello's cache in every frame. Every font vello draws carries an artic id, so ids can't collide in its cache.

A renderer such as skia keeps a map from `(id, index)` to its typefaces, with an eviction policy of its own; a CPU renderer caches `skrifa` outlines the same way.

Freeing memory on a memory warning (#146) decides then what to drop.

## 7. Testing

The test font is a font under the SIL Open Font License, picked in the plan by size, with a regular and a bold face as separate files and no italic face. Bold then selects a real face; italic is synthesized, and so is bold when the engine is given only the regular face. The files and their licence live in `artic-test-fonts`, a published crate in layer 0 with no dependencies and no code beyond `pub const REGULAR: &[u8]` and `pub const BOLD: &[u8]`, filled with `include_bytes!`. `artic-text` dev-depends on it for its own tests, and `artic-test` depends on it, so `artic_test::text::engine()` and every golden use the same bytes. A crate with no dependencies can't be part of a cycle, and no production crate gains a test-only API.

| Crate | Tests |
| --- | --- |
| `artic-layout` | `font_family_parses_list` (quoted and unquoted names, generic families), one test per `FamilyProblem` and `WeightProblem`, each checking the message quotes the text; `font_weight_names_and_numbers`; `font_data_compares_by_id`; `measure_reaches_the_text_engine` (a recording fake engine, private to the tests); `measure_only_reaches_the_text_engine` (through `LayoutManager::measure`); `arrange_reaches_the_text_engine`; `measured_result_is_kept_with_the_size`, `cached_measure_keeps_its_result`, `measure_without_a_result_drops_the_old_one`, `arranged_result_replaces_the_measured_one`, `moving_keeps_the_arranged_result`, `resizing_drops_the_arranged_result`, `measuring_drops_the_arranged_result`, `dropped_node_drops_its_results`, `set_and_dropped_results_are_reported`, `results_from_measure_reach_the_next_run`, `unchanged_layout_reports_no_results`; `parent_still_sees_the_measured_size` (an arranged result doesn't touch `desired`) |
| `artic-text` | `paragraph_size_with_test_font` (width and height within 0.5 px of the expected values), `line_breaking_respects_max_width` (the line count at two widths), `max_lines_caps_the_paragraph`, `missing_family_falls_back`, `empty_text_has_one_line_height`, `bad_sizes_give_an_empty_paragraph`, `unusable_max_width_does_not_wrap` (NaN and infinity), `negative_max_width_breaks_everywhere`, `line_height_sets_line_spacing`, `lines_stack_top_to_bottom` (each line's `top` is the previous line's `top` plus its `height`, and its `baseline` lies inside it), `bold_selects_a_heavier_face`, `missing_bold_is_synthesized` (only the regular face), `missing_italic_is_synthesized`, `right_to_left_text_stays_inside_its_size`, `same_font_keeps_its_id`, `collection_faces_get_their_own_ids`, `with_fonts_falls_back_to_its_first_font`, and `system_engine_resolves_system_ui`, which skips with a message on a machine with no fonts installed, as goldens skip without a GPU |
| `artic-element` | `render_reads_the_arranged_result_first`, `render_reads_the_measured_result_without_one` |
| `artic-render` | `text_records_one_command`, `empty_paragraph_records_nothing` |
| `artic-platform` | the pipeline and runner tests pass the test engine; `one_engine_serves_every_surface`; `content_sized_window_measures_text` (through `Pipeline::measure`); `replaced_result_is_recorded_again` |
| `artic-vello` | `font_id_becomes_the_blob_id`, `synthesized_bold_draws_heavier`; goldens `text_solid`, `text_gradient`, `text_wrapped`, each `per_os` as the [golden images design](golden-images.md) says for text; a text tile in `examples/primitives` |

## 8. Documents that change

| Document | Change |
| --- | --- |
| `docs/design/frame.md` | the facade's call becomes `Runner::new(tree, renderer, engine, application)`; step 5's `run` takes the engine and reports `results`, which join the nodes the compositor records again; `RenderCx` also carries the node's layout result |
| `docs/design/surface.md` | the facade's call, as in `frame.md`; the contexts table gains `text()` on both, `set_measured` and `set_arranged` |
| `docs/design/layout-tests.md` | `Headless` owns the test engine and passes it to `run`, so its `run` keeps its arguments |
| Spec §12 | the Fonts bullet: the default family is `fontique`'s `system-ui`, not a per-platform stack of artic's |
| `docs/architecture.md` | `artic-text` in layer 2, the graph and the edge table; the layer 1 role includes text values and the layer 2 role no longer says `artic-render` shapes text; the text-measurement bullet names `artic-text` as the trait's implementation |
| Spec §2 | the crate table: `artic-render` no longer shapes text, and `artic-text` gets a row |
| Spec §10 | the Text bullet: shaping is in `artic-text`, not `artic-render` |
| `docs/design/backends.md` | the crate table and the list of changes say text shaping is in `artic-text` |
| `docs/design/border.md` | §6: snapping chrome to device pixels is #295 |
| Issue #37 | the entry point is `TextEngine`, not `Renderer::measure_text`; `measure_text_with_test_font` becomes `paragraph_size_with_test_font`; the glyph cache criterion becomes `font_id_becomes_the_blob_id` |

## 9. What waits

| Feature | How it fits | Tracked in |
| --- | --- | --- |
| Wrapping and trimming modes, alignment, the inherited font and foreground properties | `TextBlock` declares them and fills `TextStyle` and `TextLimits`; alignment is a `TextStyle` field the engine applies | #46 |
| Inline runs with their own brush, weight and style | spans in `paragraph`, and a brush per run in `Command::Text` | #47 |
| Color emoji | vello's `COLRv1` glyphs | #175 |
| App fonts from resources | `Engine` registers them beside the system's | a later issue |
| Outlines for CPU renderers | a `skrifa` helper, as the [backends design](backends.md) says | the first CPU renderer |
| Snapping text and hinting | the device-pixel grid in `RenderCx`; hinting stays out of measure, so desired sizes keep not depending on the display | with #295 |
| Freeing memory on a warning | whatever #146 decides to drop | #146 |

## 10. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | this document and the documents in section 8 | approved |
| B | section 2 and the `TextEngine` trait: the value types and their parsers, in `artic-layout`; no caller changes | the value-type tests in section 7 pass |
| C | section 4: `artic-text`, with `parley` and `fontique` pinned exactly in the workspace beside `vello`; the `artic-test-fonts` crate with the test font and its licence | the `artic-text` tests pass |
| D | the rest of section 3: `text()`, `set_measured` and `set_arranged`, `RenderCx::layout_result`, `LayoutStats::results`, `PixelGrid`, and the engine through `LayoutManager::run` and `measure`, `Pipeline::prepare` and `measure`, the runner, `artic_test::text::engine()` in `Headless` and `render_tree`, and `Engine::system()` in the `frames` example | the `artic-layout`, `artic-element`, `artic-platform`, `artic-test` and `artic-controls` tests pass, including the property tests with snapping on, and the workspace builds with `--all-targets` |
| E | sections 5 and 6: `Command::Text`, `SceneBuilder::text`, vello drawing, the goldens and the example; closes #37 | the `artic-render` and `artic-vello` tests pass |
