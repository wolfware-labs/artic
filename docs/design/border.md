# Design: Border

Issue: #45. Builds on: [elements](element.md) (#42), [shapes, strokes, clips and shadows](shapes.md) (#36), [golden images](golden-images.md) (#38), [layout tests](layout-tests.md) (#56), [panels](panels.md) (#44). Unblocks: #54, #49, #167.

`Border` draws a box around one child: a shadow, a background, an outline whose sides can differ in width, rounded corners, and padding between the outline and the child, which it clips to the inside of the outline. It is a decorator, not a panel: it has exactly one child and no rule for arranging several. A box with a background in WPF and Avalonia is a `Border`, and control templates (#84) build their chrome from it.

```xml
<Border Background="White" BorderBrush="#D0D0D0" BorderThickness="1"
        CornerRadius="8" Padding="12" BoxShadow="0 2 8 #40000000">
  <StackPanel Spacing="4">…</StackPanel>
</Border>
```

## 1. Clipping descendants only

A node draws into its own fragment, and its children are separate nodes the compositor draws after it, so a control can't clip its children from inside its `render`. The hook for that is `ElementImpl::clip`, which today clips the element and its descendants. A clip to the inside of `Border`'s outline would then also cut off its shadow, background and outline.

`ElementImpl::clip` changes meaning to CSS's `overflow: hidden`: it clips the element's descendants, in the element's own coordinates, and leaves the element's own drawing alone. An element controls its own drawing and never needs a clip to stay inside itself; only its children need a fence. No control uses the hook yet; two tests rely on the old meaning and are rewritten (section 7).

The compositor (`artic-render`, `Walk::visit`) pushes the layers in a new order:

1. the node's transform and opacity, as now, so opacity still groups the node with its children;
2. the node's own fragment, not clipped;
3. the clip, only when at least one of the node's visual children is drawn (its layers are visible), so an element with no children, or only collapsed or hidden ones, costs no clip layer;
4. the children, then the pops.

`ElementImpl::clip` and `Layers.clip` keep their names; their docs, `element.md` and the spec's §7 hook comment say what the clip covers now. The hit-test walk doesn't exist yet; when #74 builds it, a point outside a node's clip shouldn't reach its children.

## 2. The inner shape and the ring

Both are geometry, beside `RoundedRect` and `Path` in `artic-layout`:

```rust,ignore
impl RoundedRect {
  /// The shape inside a border of `thickness`, as CSS defines its inner border edge.
  pub fn inner(self, thickness: Thickness) -> Path;
  /// The border between this shape and its `inner` shape, as one even-odd path.
  pub fn ring(self, thickness: Thickness) -> Path;
  /// The fitted shape with radii a border of `thickness` can follow inside.
  pub fn fitted_for_border(self, thickness: Thickness) -> RoundedRect;
}
```

The outer shape is `self.fitted_for_border(thickness)`: `self.fitted()`, which already scales radii that don't fit, as CSS does, then each corner's radius cut to the distance from its corner to the inner edge of each side opposite it. The top-left radius is at most `width − right` and `height − bottom`, the top-right at most `width − left` and `height − bottom`, the bottom-right at most `width − left` and `height − top`, and the bottom-left at most `width − right` and `height − top`. Without a border that changes nothing. The inner shape is the rectangle deflated by `thickness`, with each corner's radius reduced per axis by the widths of its two sides, stopping at 0: the top-left corner becomes `(radius − left, radius − top)`, the top-right `(radius − right, radius − top)`, the bottom-right `(radius − right, radius − bottom)` and the bottom-left `(radius − left, radius − bottom)`. Each inner corner then keeps its outer corner's center, so the inner edge never crosses the outline, and the cut above is exactly what keeps those inner radii inside the inner box. Scaling the inner radii down instead, as CSS's overlapping-corner rule would, moves a corner's center: on a box 20 wide with a top-left radius of 19 and a right side of 10, the inner corner would shrink to 10 and bulge past the outer curve, so the even-odd ring would paint outside the box. When the sides differ, the inner corners are elliptical, which keeps the ring an even width along each side; reusing the outer radius inside makes the corners look lumpy.

Each quarter of an ellipse is one cubic Bézier with control points at 0.5523 of the radius from the ends, within about 0.03% of the radius from the true curve. The outer corners come from the same function with equal radii, so the two outlines of the ring are built the same way; `Border` fills its background with that outer path too, so the background and the ring share an edge exactly. With zero thickness, `inner` is that same outline. When the deflated rectangle is empty on an axis, `inner` is a path of a single point (`Path::builder(origin).build()`, since a `Path` always starts with a point), which as a clip hides everything, and `ring` is the whole outer shape: the border fills the box. The arithmetic goes through `finite::add`, as the rest of layout's geometry does. A side that isn't a finite number of 0 or more counts as 0, here and in `Border`'s layout, so a negative side can't turn the ring inside out.

One even-odd fill of `ring` draws any border, whether its sides are equal or not, so there is one drawing path to test.

## 3. `BoxShadow` from text

`BoxShadow` exists in `artic-render` (#36), and vello already draws it outside its caster only, so it never shows through a semi-transparent background (#283). UI languages need it from text, so `impl FromStr for BoxShadow` goes in `shadow.rs`.

The grammar is CSS's and Avalonia's: `offset-x offset-y [blur [spread]] color`, separated by whitespace. Every token but the last is a finite number, two to four of them; the last is the color, parsed by `Color::from_str`. Blur and spread default to 0. Blur can't be negative; spread can, and shrinks the shadow. `"0 2 8 #40000000"`, `"4 4 Black"` and `"0 0 12 -2 #80000000"` are valid. One value is one shadow. `inset` (#275) is refused: a token equal to `inset`, ignoring case, anywhere in the value is checked for before the numbers are read, so it reports `Inset` rather than `NotANumber`.

The color is required: CSS defaults it to the text color, which artic doesn't have, and a required part can later get a default without breaking anything.

Errors carry the text and a problem enum, as `ParseGeometryError` does; the type name and forms are fixed for a shadow, so they're part of the message rather than fields:

```rust,ignore
pub struct ParseBoxShadowError {
  pub text: String,
  pub problem: ShadowProblem,
}

pub enum ShadowProblem {
  Empty,
  Inset,
  MissingColor,
  TooFewNumbers,
  TooManyNumbers,
  NotANumber(String),
  NotFinite(String),
  NegativeBlur(String),
  Color(ParseColorError),
}
```

`artic-render` re-exports `ParseBoxShadowError` and `ShadowProblem` from its root, beside `BoxShadow`, as it does `ParseColorError` beside `Color`. The message quotes the text and shows the forms, in the same shape as `ParseColorError`'s: `invalid BoxShadow '4 4': it needs a color last; expected forms like '0 2 8 #40000000' or '4 4 Black'`. A last token that parses as a number is `MissingColor`; any other last token is the color, so a mistyped color such as `#4000000` reports `Color`, worded as "its color '#4000000' …" with the color error's own reason, not a number problem.

## 4. `Border`

In `artic-controls`:

```rust,ignore
#[element]
#[derive(Debug)]
pub struct Border {
  #[ui(content)] pub child: Child,
  #[ui(affects = layout)] pub padding: Property<Thickness>,
  #[ui(affects = layout | render)] pub border_thickness: Property<Thickness>,
  #[ui(affects = render)] pub background: Property<Option<Brush>>,
  #[ui(affects = render)] pub border_brush: Property<Option<Brush>>,
  #[ui(affects = render)] pub corner_radius: Property<CornerRadius>,
  #[ui(affects = render)] pub box_shadow: Property<Option<BoxShadow>>,
}
```

`Border` declares its own `Background`: it isn't a panel, and its background fills a rounded shape, not the `Panel` group's rectangle. WPF's `Border.Background` and `Panel.Background` are separate properties too. A shared group of box chrome would have one user, so it waits for a second one.

| Step | Rule |
| --- | --- |
| Properties | `border_thickness` affects layout and render, since the outline and the clip use it even when the arranged size doesn't change; `padding` affects layout only |
| Measure | the child is offered the available size less `border_thickness` and `padding`, stopping at 0 and saturating; `Border` asks for the child's desired size plus both. Without a child, it asks for the thickness plus the padding. A side of either that isn't a finite number of 0 or more counts as 0 |
| Arrange | the child gets the box deflated by `border_thickness`, then by `padding`, with its origin kept inside the box when the two are larger than it, so the child's rectangle never lies outside its parent |
| Render | with `outer` the box with `corner_radius`, `fitted_for_border(border_thickness)`: the shadow, if set; the background filling `outer`'s path, if set; and `outer.ring(border_thickness)` filled with `border_brush`, if it's set and at least one side isn't zero. The background reaches under the outline, as CSS's default `background-clip: border-box` does |
| Clip | `outer.inner(border_thickness)`: the child and its subtree are clipped to the inside of the outline, padding included, always, so a child never draws over the outline whatever the corners. With no child, the compositor pushes no clip |
| Accessibility | nothing added: the default role, `GenericContainer`. The child's place in the accesskit tree comes from #54's walk |

Layout snaps the child's edges to the device-pixel grid, while the ring and the clip use the thickness as given, since `render` and `clip` don't get the scale. At a fractional scale with a fractional thickness, the child's edge and the clip's edge can differ by up to half a device pixel, so a child that fills its area may lose an anti-aliased sliver at the edge. Snapping the chrome needs the scale in `RenderCx`, which waits for a later issue (section 6).

## 5. Documents that change

| Document | Change |
| --- | --- |
| `docs/design/element.md` | `ElementImpl::clip` clips descendants only; the layers order in its `layers` row and in the composition paragraph |
| `docs/design/shapes.md` | the clip layer's description and order; "Border insets its rectangle by half the thickness" becomes the ring fill; the `BoxShadow` text format lives on `artic-render`'s `BoxShadow` itself, not a separate value type |
| `artic-render` docs | `Layers` and `Layers.clip`, and `Compositor::compose`'s order, which becomes: transform and opacity, the node's fragment, then the clip around its children |
| Spec §7 | the `clip` hook comment ("clips the element's descendants") and the `layers` comment's order; the `Border` row gains padding |
| Spec §3 | the `Border` example lists the properties of section 4 |

## 6. What waits

| Feature | How it fits | Tracked in |
| --- | --- | --- |
| Inset shadows | `inset` in the grammar, and drawing inside the shape | #275 |
| Border styles and theme resources | setters on these plain properties | #109, #116 |
| Hit testing through the clip | the walk skips a node's children outside its clip | #74 |
| Accesskit child order | the tree walk | #54 |
| Snapping the outline and clip to device pixels | the scale in `RenderCx`, so chrome can round its thickness as layout does | #295 |
| `Image` clipping to rounded corners | the same `clip` hook and `inner` shape | #71 |

## 7. Testing

| Crate | Tests |
| --- | --- |
| `artic-render` | `clip_applies_to_children_not_the_node`, `no_clip_without_children`, `no_clip_when_no_child_is_drawn` (a collapsed child), with the composition tests updated for the new order; `BoxShadow` parsing: `two_numbers_and_a_color`, `three_numbers_add_blur`, `four_numbers_add_spread`, `negative_spread_is_allowed`, `inset_is_reported_as_inset`, and one test per `ShadowProblem`, each checking the message quotes the text |
| `artic-layout` | `inner_radii_shrink_per_axis`, `zero_thickness_inner_is_the_outline`, `inner_radius_stops_at_zero`, `radii_stop_at_the_opposite_inner_edge`, `inner_corners_share_the_outer_centers`, `inner_stays_inside_the_outline`, `fitted_for_no_border_is_fitted`, `negative_sides_count_as_zero`, `thick_border_leaves_no_hole`, `ring_fills_even_odd`, `ring_bounds_match_the_outer_shape` |
| `artic-controls` | `border_measure_includes_padding_and_thickness`, `border_arranges_child_inside_padding`, `border_without_a_child_is_its_chrome`, `thick_border_and_padding_stay_non_negative`, `border_clips_to_the_inner_shape`, `border_child_is_its_content`, `border_accesskit_node`, and the property test `border_invariants_hold` over random thickness and padding, with containment on |
| `artic-vello` | `transform_and_clip_place_the_child` in `pixels.rs` rewritten so the clip is on a parent; goldens `border_background`, `border_uneven_thickness`, `border_corner_radius`, `border_box_shadow`, `border_clips_child` (a square child whose corners would show past rounded corners) |

## 8. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | this document | approved |
| B | section 1, the compositor's order and the clip's docs, with the two rewritten tests; section 2, `RoundedRect::inner` and `ring`; section 3, `BoxShadow: FromStr`; the `element.md`, `shapes.md` and spec §7 changes | the `artic-render` and `artic-layout` tests in section 7 pass, and `pixels.rs` |
| C | section 4, `Border`, its tests and the five goldens, and the spec §3 example; closes #45 | the `artic-controls` and `artic-vello` tests in section 7 pass |
