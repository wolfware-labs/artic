# Design: shapes, strokes, clips and shadows

Issue: #36. Builds on: the retained scene (#33), the vello renderer (#34), [colors and brushes](brush.md) (#35), geometry (#22). Unblocks: #45 (`Border`), #38 (golden images), #71 (`Image`).

`SceneBuilder` learns to draw everything the [backend design](backends.md) requires of a renderer, apart from images and glyph runs: rectangles, rounded rectangles and paths, filled or stroked with any brush, box shadows, clips of any shape and opacity layers. After this, `Border` and every later control can draw.

```rust,ignore
builder.box_shadow(card, BoxShadow::new(Point::new(0.0, 2.0), 8.0, Color::rgba(0, 0, 0, 64)));
builder.fill(card, Color::WHITE);
builder.stroke(card, Color::LIGHT_GRAY, Stroke::new(1.0));
builder.with_clip(card, |builder| {
  builder.with_opacity(0.5, |builder| builder.fill(star, LinearGradientBrush::default()));
});
```

## 1. Geometry

Two types join `Rect` in `artic_layout::geometry`, because layout and hit-testing need them as much as drawing does:

```rust,ignore
pub struct RoundedRect { pub rect: Rect, pub radius: CornerRadius }

pub enum PathElement { MoveTo(Point), LineTo(Point), QuadTo(Point, Point), CubicTo(Point, Point, Point), Close }
pub enum FillRule { EvenOdd, NonZero }
pub struct Path { /* elements: Arc<[PathElement]>, fill_rule: FillRule */ }

let triangle = Path::builder(Point::new(50.0, 0.0))
  .line_to(Point::new(100.0, 100.0))
  .line_to(Point::new(0.0, 100.0))
  .close()
  .fill_rule(FillRule::NonZero)
  .build();
```

| Rule | Why |
| --- | --- |
| `Path::builder(start)` starts the first figure; `move_to` starts another | A path can't begin with a segment from nowhere, so no renderer needs a rule for one |
| After `close`, the next segment starts from the closed figure's start, as in SVG | The common convention, so path data translates one to one |
| A path carries its `FillRule`, default `EvenOdd`, which fills and clips both use | Where a path crosses itself, the rule decides what is inside, so it can't be left to each renderer. It belongs to the geometry, as `PathGeometry.FillRule` does in WPF and Avalonia, and the default is theirs |
| A fill or clip treats an open figure as closed by a straight line back to its start; a stroke leaves it open | What every renderer and SVG do, written down so none differs |
| A path's elements are an `Arc<[PathElement]>` | Commands are cloned into fragments; a shared list keeps that cheap, as with gradient stops (#35) |
| `Path::bounds()` is exact: it includes each curve's extremes, not only its control points | Gradients resolve against the bounds; control points would make a circle's box too big and shift its colors |
| `RoundedRect::fitted()` makes radii drawable: a negative, NaN or infinite radius is `0`, and when two radii on one side add up to more than the side, all four shrink by the same factor, as in CSS | Every renderer gets the same corners from one function, and `Border` can pass its properties through |
| `Rect` converts into `RoundedRect`, with zero radii | A box shadow takes either |

Parsing SVG path data (`"M0,0 L10,10 Z"`) can come later as a `FromStr` beside the other geometry parsers.

## 2. The drawing vocabulary

A shape says *what* is drawn; a command says *how*. Fills, strokes and clips work on every shape. A box shadow is cast only by a rounded rectangle, as [Box shadows](#box-shadows) explains:

```rust,ignore
pub enum Shape { Rect(Rect), RoundedRect(RoundedRect), Path(Path) }

pub enum Command {
  Push(Layer),
  Pop,
  Fill { shape: Shape, brush: Brush },
  Stroke { shape: Shape, brush: Brush, stroke: Stroke },
  BoxShadow { rect: RoundedRect, shadow: BoxShadow },
}

pub enum Layer { Transform(Transform), Clip(Shape), Opacity(f32) }
```

`Shape` converts from each of its three types, and `Shape::bounds()` is the box a brush's relative points resolve against. A stroke's brush resolves against the same bounds, without the stroke width, as in WPF.

The `Clip` enum is gone: a clip is a `Shape`, so rectangles, rounded rectangles and paths all clip. `ElementImpl::clip` returns `Option<Shape>`, and the compositor's `Layers.clip` holds one; it clips the node's descendants, not the node's own drawing ([border design](border.md)). `Layer` and `Layers` stop being `Copy`, since a shape can hold a path; cloning one is still cheap.

### Strokes

```rust,ignore
#[non_exhaustive]
pub struct Stroke { pub width: f32, pub join: LineJoin, pub cap: LineCap, pub miter_limit: f32 }
pub enum LineJoin { Miter, Round, Bevel }
pub enum LineCap { Flat, Round, Square }

Stroke::new(2.0).with_join(LineJoin::Round).with_cap(LineCap::Round)
```

The defaults are WPF's and Avalonia's: width `1`, `Miter`, `Flat`, miter limit `10`. The names are WPF's and Avalonia's (`Flat`, not SVG's `Butt`), since UI languages will set them once shapes become elements. A stroke is centered on the outline. `Border` draws its outline as a filled even-odd ring instead, so its sides can differ in width ([border design](border.md)).

`Stroke` is `#[non_exhaustive]` and built with `new` and `with_*` methods, so code outside `artic-render` can't write it as a literal, and adding fields (dashes, below) breaks no caller.

### Box shadows

```rust,ignore
#[non_exhaustive]
pub struct BoxShadow { pub offset: Point, pub blur: f32, pub spread: f32, pub color: Color }

BoxShadow::new(Point::new(0.0, 2.0), 8.0, Color::rgba(0, 0, 0, 64)).with_spread(2.0)
```

`new` takes the offset, blur and color, with no spread; like `Stroke`, it is `#[non_exhaustive]` and grows with `with_*` methods. A box shadow is CSS's outer `box-shadow`: the rounded rectangle, grown by `spread`, moved by `offset`, blurred by `blur`, in `color`. As in CSS, it shows only outside the rounded rectangle that casts it, so a translucent background doesn't darken. It is drawn behind what casts it, so `Border` records it before its background. It also parses from text in CSS's form, `offset-x offset-y [blur [spread]] color` ([border design](border.md)). The name leaves room for a `DropShadow` effect on whole elements later (section 5).

## 3. The builder

```rust,ignore
impl SceneBuilder {
  pub fn fill(&mut self, shape: impl Into<Shape>, brush: impl Into<Brush>);
  pub fn stroke(&mut self, shape: impl Into<Shape>, brush: impl Into<Brush>, stroke: Stroke);
  pub fn box_shadow(&mut self, rect: impl Into<RoundedRect>, shadow: BoxShadow);
  pub fn with_clip(&mut self, shape: impl Into<Shape>, draw: impl FnOnce(&mut SceneBuilder));
  pub fn with_opacity(&mut self, opacity: f32, draw: impl FnOnce(&mut SceneBuilder));
  pub fn with_transform(&mut self, transform: Transform, draw: impl FnOnce(&mut SceneBuilder));
}
```

There is one method per operation, so new shapes add no methods. `fill_rect` is removed; its callers write `fill(rect, color)`.

Layers stay closures. The layer is pushed before the closure runs and popped when it returns, so a recording is balanced by construction: a push without its pop can't be written, and there is nothing to check at runtime. Issue #36 asked for a debug panic on unbalanced pushes; that was designed for a push/pop API the builder doesn't have.

Each method has rustdoc with a usage example.

## 4. Drawing

Values out of range are made drawable when drawn, never rejected, as for brushes (#35). The rules live in `artic-layout` and `artic-render` as functions, so every renderer draws the same pixels:

| Value | Drawn as |
| --- | --- |
| Corner radii | `RoundedRect::fitted()` (section 1) |
| A stroke width of `0`, negative, NaN or infinite | nothing |
| A finite miter limit below `1` | `1` |
| A miter limit NaN or infinite | `10`, the default |
| A shadow blur below `0`, NaN or infinite | `0`: a sharp shadow |
| A negative spread | shrinks the rectangle, as in CSS, down to nothing |
| A spread NaN or infinite | `0` |
| A shadow offset with a NaN or infinite coordinate | nothing |
| A shadow's radii | grown by the spread; a corner with radius `0` stays sharp |
| A shape with a NaN or infinite coordinate | nothing |
| A rectangle or rounded rectangle with a negative width or height | nothing, as `Rect`'s parser already rejects one |
| A layer's opacity below `0`, above `1`, NaN or infinite | clamped to `0..=1`, NaN as `1`. One function in `artic-render` does this for `with_opacity` and for the element's `Opacity` when it builds `Layers` (the [element design](element.md) §7), so the two can't differ. A brush's opacity differs on purpose: NaN counts as `0` there (#35), since a broken brush should paint nothing, while a broken layer opacity shouldn't hide a whole subtree |

`artic-vello` translates, in `translate.rs`:

- A `Shape` becomes a kurbo `Rect`, `RoundedRect` (with per-corner radii) or `BezPath`.
- `Fill` and `Stroke` use the brush conversion from #35, resolved against `shape.bounds()`. A `Stroke` becomes a kurbo `Stroke` with the same width, join and miter limit. Its cap is both kurbo's start and end cap, so every open figure gets the same cap at both ends (WPF's separate start and end caps aren't needed yet); `LineCap::Flat` is kurbo's `Cap::Butt`, `Round` is `Cap::Round` and `Square` is `Cap::Square`. Closed figures have no ends, so they take no cap.
- `Layer::Clip` becomes `push_clip_layer` with the shape, and a path's `FillRule` becomes vello's `Fill::EvenOdd` or `Fill::NonZero` for fills and clips.
- A `BoxShadow` becomes `draw_blurred_rounded_rect`, with the standard deviation CSS defines, `blur / 2`. Vello's fast blur takes one radius, so a shadow with unequal radii is drawn in four quadrants, each clipped to its quarter of the shadow and blurred with its corner's radius. Near a corner, a blurred rounded rectangle depends almost only on that corner's radius, so the seams don't show. The shadow is drawn inside an even-odd clip of the surface and the caster's outline, which leaves out the caster.

## 5. What waits

| Feature | How it fits | Tracked in |
| --- | --- | --- |
| Dashed strokes | `Stroke::with_dashes(dashes, offset)` adds two fields. Every renderer in this repository draws them in the same change, and renderer crates elsewhere learn of it from the changelog, so none draws a dashed stroke solid without knowing | #274 |
| Inset box shadows | A clip to the shape around a blurred ring: a second vello path and its own pixel tests | #275 |
| `DropShadow` on any element | An effect on a layer, `Layer::Effect(DropShadow)` beside clip and opacity: each renderer blurs the layer's alpha. This is how icons, paths and text cast shadows, as with WPF's and Avalonia's `DropShadowEffect` | #275 |
| SVG path data | `FromStr for Path`, beside the other geometry parsers | when the `Path` element needs it |

Glyph runs (#37), images (#71), golden images (#38) and blend modes other than `Normal` are out of scope, as the issue says.

## 6. Testing

| Crate | Tests |
| --- | --- |
| `artic-layout` | `path_builder_records_figures`, `path_bounds_include_curve_extrema`, `path_fill_rule_defaults_to_even_odd`, `rounded_rect_fitted_scales_overlapping_radii` (and negative and NaN radii) |
| `artic-render` | `opacity_is_clamped`, `layers_balance_by_construction`, `opacity_layer_nests` (two nested opacity layers record in order), `fill_and_stroke_record_shapes`, `shape_bounds_cover_each_variant`, `stroke_has_the_usual_defaults` |
| `artic-vello` | `shapes_convert_to_kurbo` (per-corner radii, curves), `stroke_converts_join_cap_and_miter`, `invalid_shapes_and_strokes_draw_nothing` (each row of section 4's table, infinities included), `box_shadow_values_are_made_drawable`, `box_shadow_grows_moves_and_blurs` (spread, offset, standard deviation) |
| `artic-vello` pixels | `rounded_rect_leaves_corners_clear`, `stroke_paints_only_the_outline`, `path_clip_masks_drawing`, `fill_rule_decides_a_star_center`, `box_shadow_fades_outside_the_rect`, `unequal_radii_shadow_matches_each_corner` (each corner matches a shadow with that corner's radius everywhere), `box_shadow_stays_outside_its_caster`, `gradient_on_path_uses_its_bounds` |

The example `artic-vello/examples/primitives.rs` draws every primitive in one window: a rounded rectangle, a stroked star, linear and radial swatches, clips by rectangle, rounded rectangle and path, nested opacity, and a card with a shadow. It replaces `window.rs`, whose rectangles, clip and opacity it covers. Its PR records a manual check on each desktop available.

## 7. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | this document; the follow-up issues #274 and #275 | approved |
| B | `RoundedRect` and `Path` in `artic-layout`; `Shape`, `Stroke`, the new commands and shape clips in `artic-render`, with `fill_rect` and `Clip` removed; fills, strokes and clips in `artic-vello`; every caller in the workspace moved over: `ElementImpl::clip` and `Layers` in `artic-element`, and the tests and examples of `artic-element`, `artic-platform`, `artic-winit` and `artic-vello` | the tests in section 6, except box shadows |
| C | `BoxShadow`, recorded and drawn by vello with the quadrant path; `examples/primitives` | the box shadow tests, and the example checked by hand |
