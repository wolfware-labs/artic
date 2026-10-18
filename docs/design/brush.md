# Design: colors and brushes

Issue: #35. Builds on: [value objects](value-objects.md) (#32), the [frame pipeline](frame.md) (#40), geometry parsing (#22). Unblocks: #36 (shapes, clips and shadows), #46 (`TextBlock`), #37, #116.

A brush is the paint behind `Background="#3B82F6"`, `BorderBrush="Red"` or a gradient. This design gives `Color` its XAML text forms, adds `Brush` with solid and gradient variants, and carries brushes through the scene to vello, so a control's `Background` reaches pixels.

```xml
<Border Background="#3B82F6" BorderBrush="SteelBlue"/>
```

```rust,ignore
builder.fill(bounds, Color::STEEL_BLUE);
builder.fill(bounds, LinearGradientBrush::new(
  RelativePoint::relative(0.0, 0.0),
  RelativePoint::relative(1.0, 0.0),
  [GradientStop::new(0.0, Color::WHITE), GradientStop::new(1.0, Color::STEEL_BLUE)],
));
```

## 1. Colors

`Color` stays in `artic-render`: sRGB, straight (not premultiplied) alpha, 8 bits per channel. It gains `FromStr`:

| Text | Meaning |
| --- | --- |
| `#RGB` | each digit doubled, opaque: `#3AF` is `#33AAFF` |
| `#ARGB` | each digit doubled, alpha first |
| `#RRGGBB` | opaque |
| `#AARRGGBB` | alpha first, as in WPF and Avalonia |
| a name | a CSS named color, ignoring ASCII case (below) |

The names are exactly CSS Color Module Level 4's `<named-color>` list: 148 names, including `rebeccapurple` and the `grey` spellings beside the `gray` ones, plus `transparent`, which is `#00000000`. That's WPF's and Avalonia's list with the CSS additions. One difference from WPF: its `Transparent` is `#00FFFFFF`. The two draw the same on their own, but a gradient from `transparent` blends through black, not white. Each name has a constant (`Color::REBECCA_PURPLE`, `Color::GREY` and `Color::GRAY` alike), and the lookup table is built from the same list, so the two can't diverge. The rustdoc lists the names.

CSS writes 8-digit hex with alpha *last* (`#RRGGBBAA`); the rustdoc says so, since text copied from CSS parses as a different color. The names live in `color/names.rs` as constants (`Color::STEEL_BLUE`) and a lookup table.

A failure is a `ParseColorError { type_name, text, problem, forms }`. Its message is worded like `ParseGeometryError`'s: it repeats the text, says what is wrong, and lists the accepted forms:

```text
invalid Color '#12': it has 2 hex digits; expected "#RGB", "#ARGB", "#RRGGBB", "#AARRGGBB" or a color name
invalid Color 'redd': 'redd' is not a color name; expected "#RGB", "#ARGB", "#RRGGBB", "#AARRGGBB" or a color name
```

## 2. Brushes

One type per markup element, and `Brush` as the family a property holds:

```rust,ignore
pub enum Brush {
  Solid(SolidColorBrush),
  Linear(LinearGradientBrush),
  Radial(RadialGradientBrush),
}

pub struct SolidColorBrush { pub color: Color, pub opacity: f32 }
pub struct LinearGradientBrush { pub start: RelativePoint, pub end: RelativePoint, pub stops: Arc<[GradientStop]>, pub opacity: f32 }
pub struct RadialGradientBrush {
  pub center: RelativePoint,
  pub radius_x: RelativeScalar,
  pub radius_y: RelativeScalar,
  pub stops: Arc<[GradientStop]>,
  pub opacity: f32,
}
pub struct GradientStop { pub offset: f32, pub color: Color }
```

| Rule | Why |
| --- | --- |
| Each struct converts into `Brush` with `From`, and so does `Color` (a solid brush) | `fill(rect, Color::RED)` and `Background = gradient.into()` read naturally |
| `FromStr for Brush` parses color text into a solid brush | `Background="Red"` is the common case |
| Defaults: linear from `0%,0%` to `100%,100%`; radial centered at `50%,50%` with radii of `50%`; opacity `1` | A gradient written with only its stops fills its box |
| `opacity` multiplies the alpha of the color, or of every stop | One brush-wide fade, as in WPF and Avalonia, without rewriting the stops |
| Stops are an `Arc<[GradientStop]>` | A brush is cloned into each draw command; a shared slice keeps that cheap |
| Two radii | A radial gradient fills a box that isn't square as an ellipse, as in Avalonia |
| Out-of-range values are clamped when drawn, not rejected | A brush built in code or animated can pass through odd values; drawing never fails |

The structs have the shape the [value objects](value-objects.md) design gives value elements, with one addition when that lands (#57): an `Arc<[GradientStop]>` can't grow, so the staged `LinearGradientBrush` or `RadialGradientBrush` collects its `GradientStop` children in a `Vec`. The `Arc` is made from that `Vec` when the value is moved into its property. The public fields and constructors stay as they are.

## 3. Relative units

Gradient points and radii are resolved against the box being filled, so one brush fits any size. The types are geometry, so they live in `artic_layout::geometry` beside `Point`, where the number parser and `ParseGeometryError` they reuse already are:

```rust,ignore
pub enum Scaling { Relative, Fixed }
pub struct RelativePoint { pub point: Point, pub scaling: Scaling }
pub struct RelativeScalar { pub value: f32, pub scaling: Scaling }
```

`"50%,25%"` is relative, the fractions `0.5, 0.25` of the box; `"10,20"` is fixed, in logical units from the box's top-left corner, whatever its size. Both are resolved against the box; `Scaling` says whether the value grows with it. A point mixing the two, such as `"50%,20"`, is an error. The scaling is visible in the text, so there is no separate mapping mode to remember. They have a parser of their own, beside the number parser: it splits the text the same way (commas, whitespace or both) and reads each value as a number with an optional `%`. The existing number parser stays unitless, so `Point`, `Thickness` and the other geometry values still reject `%`. Errors are `ParseGeometryError`s, with two new problems: `MixedUnits` (`"50%,20"`) and `NotAPercent` for a `%` that doesn't follow a number.

## 4. Drawing

Fill commands carry a brush instead of a color: `Command::Fill { shape, brush }`, recorded by `SceneBuilder::fill(shape, brush: impl Into<Brush>)`, which takes anything that converts, so passing a `Color` still works. Shapes come from the [shapes design](shapes.md) (#36); this design first had the same brush on a rectangle-only fill.

`artic-render` stays free of any backend. `artic-vello` converts, in `translate.rs`:

- Relative points and radii resolve against the bounds of the shape being filled.
- A linear gradient becomes a `peniko` linear gradient between the resolved points.
- A radial gradient becomes a circular `peniko` gradient of radius 1 at the origin, with a brush transform that scales it by the two radii and moves it to the center, so unequal radii draw an ellipse.
- `opacity` multiplies the alpha of the color or of each stop.
- Values outside their range are clamped, as WPF does:
  - offsets and opacity to `0..=1`;
  - radii to `0` or more;
  - a NaN offset, opacity or radius counts as `0`.
- Stops are sorted by offset, keeping the written order among equal offsets, so two stops at one offset make a hard edge.
- An empty stop list draws nothing. A single stop fills with its color.

## 5. What waits

`<SolidColorBrush Color="#3B82F6"/>`, `<LinearGradientBrush>` with its `GradientStop`s, and `<Border.Background>` property elements need the value-object machinery (`#[ui(value)]`, `TypeKind`, `FromUiObject`), which #57 implements with #43. Until then, a UI language sets brushes as color text, and Rust code builds gradients directly. A gradient spread mode (pad, reflect, repeat) and a gradient origin apart from the center are additive and wait for a need.

## 6. Testing

| Crate | Tests |
| --- | --- |
| `artic-layout` | `relative_point_parses_percent_and_fixed`, `relative_point_rejects_mixed_units`, `relative_scalar_parses_percent_and_fixed`, `relative_scalar_rejects_invalid` (empty, two values, a bare or doubled `%`) |
| `artic-render` | `color_parses_short_hex` (`#RGB`, `#ARGB`), `color_parses_rrggbb`, `color_parses_aarrggbb`, `color_parses_named_case_insensitive`, `color_rejects_invalid` (bad length, a non-hex digit, an unknown name, each message naming the input), `brush_from_color_string_is_solid` |
| `artic-vello` | `brush_to_peniko_roundtrip` (color, stops, resolved points, opacity), `gradient_values_are_clamped_and_sorted` (out-of-range and NaN offsets, opacity and radii; unordered stops), `linear_gradient_fills_rect` (pixels: the left edge is the first stop, the right edge the last), `radial_gradient_fills_an_ellipse` (pixels in a wide box: the center is the first stop, and the points at the ends of both radii are the last, so unequal radii scale the circle into an ellipse) |

## 7. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | this document | approved |
| B | relative units in `artic-layout`; `Color` parsing and names, `Brush` and its structs, fills with a brush in `artic-render`; the vello conversion | the tests in section 6 |
