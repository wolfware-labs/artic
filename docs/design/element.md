# Design: elements

Issue: #42. Implemented by pull requests under #42, in order: A (`artic-element`), B (the common properties), B2 (the property groups), C (`#[element]`), D (accessibility and composition). Related: #41 (layout), #33 (scene composition), #84 (`ContentState`, `TemplateState`), #241 (`linkme` on wasm).

Every control has the same body: a size, a margin, an alignment, a visibility, an opacity. WPF gives it to them through a base class, `FrameworkElement`. Rust has no class inheritance, so artic gives it through composition: a control is marked `#[element]`, which gives it four property groups and writes the code that connects them to the control. This document fixes the traits, the groups and what `#[element]` writes.

## 1. Pieces

| Piece | Crate | What it is |
| --- | --- | --- |
| `Element` | `artic-element` | The trait the frame pipeline drives: layout, render, hit testing, input, accessibility |
| `ElementImpl` | `artic-element` | The trait a control author implements: only what is special about the control |
| `HasState<S>` | `artic-element` | Gives a control's state structs to the traits built on them |
| `ElementType` | `artic-element` | How the pipeline finds a node's `&dyn Element` |
| `HitTestCx`, `EventCx`, `RenderCx`, `AccessCx`, `HitTestResult`, `Handled` | `artic-element` | Placeholders M3 fills in, enough for the defaults |
| `InputEvent` | `artic-core` | Placeholder input data, named to avoid `Event<A>`, the field type |
| `Layout`, `Appearance`, `Interaction`, `Identity`, `Visibility`, `Cursor`, `Classes` | `artic-element` | The properties every control has, in four groups |
| `#[element]`, `#[derive(UiProperties)]` | `artic-macros` | The element attribute and the group derive |

## 2. The traits

```rust,ignore
pub trait Element: Object + Layoutable {
  fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder);
  fn hit_test(&self, cx: &HitTestCx<'_>, point: Point) -> HitTestResult { cx.bounds_test(point) }
  fn on_event(&mut self, cx: &mut EventCx<'_>, event: &InputEvent) -> Handled { Handled::No }
  fn accessibility(&self, cx: &AccessCx<'_>) -> accesskit::Node;
  /// The node's layers in composition for its arranged `size`: transform, clip, opacity,
  /// visibility, z-index.
  fn layers(&self, size: Size) -> Layers { Layers::default() }
}

pub trait ElementImpl {
  /// The content's size within `available`, which already excludes the margin and is held within
  /// the element's limits.
  fn measure_override(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size { Size::ZERO }
  /// Places the children within `size`, in the element's own coordinates.
  fn arrange_override(&self, cx: &mut ArrangeCx<'_>, size: Size) {}
  fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder) {}
  fn on_event(&mut self, cx: &mut EventCx<'_>, event: &InputEvent) -> Handled { Handled::No }
  /// Adds the control's role, name, value and actions to the default node built from the groups.
  fn accessibility(&self, cx: &AccessCx<'_>, node: &mut accesskit::Node) {}
  /// Clips the element and its descendants, in its own coordinates, for its arranged `size`.
  fn clip(&self, size: Size) -> Option<Shape> { None }
  // #84 adds, with TemplateCx: fn on_apply_template(&mut self, cx: &mut TemplateCx<'_>) {}
}
```

`ElementImpl` is also where every per-control hook goes, each with a default: `on_apply_template` joins it rather than living on `Templated`, because state traits come from blanket impls (section 3), which a control can't override. #84 adds the hook together with `TemplateCx`, since nothing applies templates before it; PR A has no placeholder for either.

- The supertrait is `Object`, the trait in `artic-core`; spec §7's `XamlObject` is renamed to match. Not every node comes from a UI language: builder-made and test nodes implement `Object` too.
- `Layoutable` (#41) brings `measure` and `arrange`, so `Element` adds only what layout doesn't need.
- `Element` is what the pipeline calls; `ElementImpl` is what the author writes. `#[element]` connects them, so an author can't skip the shared work by overriding the wrong method. A control with no content writes `impl ElementImpl for Spacer {}`.

## 3. What `#[element]` writes

`#[element]` is an attribute on the control's struct, not a derive, because it adds fields: a derive can only add items next to a struct. It goes first, above the struct's `#[derive(...)]` and `#[ui(...)]` attributes, since an attribute above it is applied before the fields exist. It takes no settings of its own. It:

- adds four fields, `#[ui(flatten)] pub layout: Layout`, `appearance: Appearance`, `interaction: Interaction` and `identity: Identity`, before the struct's own fields, so `TypeInfo.groups` lists them first, in that order. The names are reserved: a field of the struct with one of them is an error that says `#[element]` adds it. A unit struct, such as `#[element] struct Spacer;`, gets the four fields; a tuple struct is an error;
- adds `#[derive(UiType)]`, named by its full path through the crate's `__private`, with `#[ui(crate = ...)]` naming the crate it found unless the struct names one, so a control crate needs only `artic-element` ([panels design](panels.md)). The struct's own `#[ui(...)]` attributes are the derive's. A struct that also derives `UiType` or `Default` is an error, since the added derive writes both;
- writes `Layoutable` and `Element` for the struct:

| Method | Shared work, from the groups | Then calls |
| --- | --- | --- |
| `measure` | `layout`: #41's `measure_with`, margin off, limits on. `Collapsed` never reaches it: the manager leaves the node out (`Layoutable::collapsed`) | `measure_override` |
| `arrange` | `layout`: #41's `arrange_with`, margin, limits, alignment. `Collapsed` never reaches it | `arrange_override` with the content's size |
| `render` | | `ElementImpl::render`; a hidden subtree is skipped by composition (section 7) |
| `hit_test` | `appearance.visibility` not `Visible`, or `interaction.is_hit_test_visible` false: a miss. Otherwise the trait default, `cx.bounds_test(point)` | |
| `on_event` | `interaction.is_enabled` false (own AND ancestors'): `Handled::No`, the control isn't called | `ElementImpl::on_event` |
| `accessibility` | builds the default node (section 7) | `ElementImpl::accessibility`, which adds to it |
| `layers` | `appearance`: `Opacity` (clamped), `RenderTransform`, `Visibility` and `ZIndex` as a `Layers` (section 7) | `ElementImpl::clip`, for the clip layer, which wraps only the descendants |

An element doesn't always fill the slot its parent offers: a margin or an alignment makes its box smaller. `Layout::arrange` reports that box, the slot less the margin, sized and aligned, with `ArrangeCx::set_bounds`, so `LayoutManager::rect` is the element's box rather than its slot, and its children are placed relative to it. Each node keeps the offered slot separately, so being offered the same slot still skips arranging.

A struct that declares a field named `layout`, `appearance`, `interaction` or `identity` fails to compile with an error that says `#[element]` adds it.

### State structs

For a field marked `#[element(state)]`, `#[element]` implements `HasState<S>` for the struct, where `S` is the field's type:

```rust,ignore
pub trait HasState<S> {
  fn state(&self) -> &S;
  fn state_mut(&mut self) -> &mut S;
}

#[element]
#[derive(Debug)]
pub struct Button {
  #[element(state)] #[ui(flatten)] pub content: ContentState, // HasState<ContentState>; its properties are Button's
}

impl ElementImpl for Button { /* measure_override, render, on_event, … */ }

// next to ContentState, in artic-controls (#84):
pub trait HasContent: Element + HasState<ContentState> {
  fn content(&self) -> &Property<Content> {
    &HasState::<ContentState>::state(self).content
  }
}
impl<T: Element + HasState<ContentState>> HasContent for T {}
```

The rule, as documented on `#[element]`: for each `#[element(state)]` field of type `S`, it implements `HasState<S>` for the struct, and the traits built on a state are implemented through `HasState<S>` by a blanket impl. A state trait therefore holds only methods its state fully determines, each with a default that reads the state; it names the state type (`HasState::<ContentState>::state(self)`) because a struct can have several. A blanket impl can't be overridden for one type, so anything a control customizes is an `ElementImpl` hook instead (section 2). A state struct that holds properties, such as `ContentState`'s `Property<Content>`, is also a property group: it derives `UiProperties`, and its field is marked `#[ui(flatten)]` too, so its properties are registered and visited like the groups'. The two attributes are read by different macros: `element(state)` by `#[element]`, which removes it, `ui(flatten)` by `UiType`. A struct has at most one field per state type, since two would mean two `impl HasState<S>` for the same `S`. `#[element]` reports two fields whose types are written the same way, naming both; two names for one type, such as an alias, reach rustc's conflicting-implementations error (E0119) instead, which the rule's rustdoc explains. There is no table of type names in the macro, so aliases work, and any crate, apps included, can add a state struct and its trait without changing `artic-macros`.

## 4. Finding a node's `Element`

`UiTree` stores `Box<dyn Object>`, and Rust can't downcast a `&dyn Object` to a trait `artic-core` doesn't know. `#[element]` therefore adds the type to a `linkme` slice in `artic-element`:

```rust,ignore
pub struct ElementType {
  pub type_id: fn() -> TypeId,
  pub cast: fn(&dyn Object) -> Option<&dyn Element>,
  pub cast_mut: fn(&mut dyn Object) -> Option<&mut dyn Element>,
}

// emitted for Border; downcasting is defined on dyn Any, so the object is upcast first, as
// UiTree::get_as does. Generated code names every item by its full path, since `Any` isn't in the
// prelude and the caller may not import `Element`. `__artic` is `<root>::__private`, where the root is
// whichever of `artic` and `artic-element` the calling crate depends on. `artic-element` depends
// on `artic-registry`, so its `__private` also re-exports what the UiType output names, and
// `#[element]` needs that one root. `derive(UiType)` alone still resolves `artic` or
// `artic-registry`, so PR C extends `crate_root` to take each macro's candidates:
cast: |object| (object as &dyn ::core::any::Any).downcast_ref::<Border>().map(|border| border as &dyn __artic::Element),
cast_mut: |object| (object as &mut dyn ::core::any::Any).downcast_mut::<Border>().map(|border| border as &mut dyn __artic::Element),
```

The pipeline builds a `TypeId → ElementType` map once. `cast` answers `LayoutNodes` and the render `Nodes`; `cast_mut` serves input dispatch, which calls `on_event(&mut self)`. A hand-written `Element` registers with `element_type!(TestElement)`, the line `#[element]` emits. App controls need no registration call, as spec §7 requires. `linkme` doesn't support wasm yet; the registry has the same limit, and #241 fixes both.

## 5. The property groups

The properties every control has come in four groups, each in the field `#[element]` adds: `layout: Layout` (where the element sits and how big it is), `appearance: Appearance` (how it's drawn), `interaction: Interaction` (how it takes input) and `identity: Identity` (what selectors and code find it by). Control code reads `self.layout.margin` or `self.interaction.is_enabled`, and what is special about a control stays in its own fields. Each group is a property group, not a UI type: `#[derive(UiProperties)]` gives it its keys, `PROPERTIES`, `visit_properties` and `Default`, and a UI language can't create one. Its keys are generated once, so every control shares them, and `IsEnabled` inherits across control types. `Visibility` is in `appearance`, where people look for showing and hiding, though `Collapsed` also changes layout: `#[element]` reports it to the layout manager through `Layoutable::collapsed`.

| Group | Property | Type | Default | Affects | Inherits |
| --- | --- | --- | --- | --- | --- |
| `layout` | `Width`, `Height` | `Option<f32>` | `None` (size to content) | layout | |
| `layout` | `MinWidth`, `MinHeight` | `f32` | `0` | layout | |
| `layout` | `MaxWidth`, `MaxHeight` | `f32` | infinity | layout | |
| `layout` | `Margin` | `Thickness` | zero | layout | |
| `layout` | `HorizontalAlignment`, `VerticalAlignment` | the #41 enums | `Stretch` | layout | |
| `appearance` | `Visibility` | `Visibility` | `Visible` | layout, render | |
| `appearance` | `Opacity` | `f32`; the property keeps any value, and composition clamps it (section 7) | `1` | render | |
| `appearance` | `RenderTransform` | `Option<Transform>` | `None` | render | |
| `appearance` | `ZIndex` | `i32`: draw and hit-test order among siblings | `0` | render | |
| `interaction` | `IsEnabled` | `bool` | `true` | render | yes, ANDed with its ancestors' (`inherits = and`) |
| `interaction` | `Cursor` | `Cursor` | `Arrow` | | |
| `interaction` | `Focusable` | `bool` | `false` | render (clears cached hit tests: touch inflates focusable targets, §12) | |
| `interaction` | `IsHitTestVisible` | `bool` | `true` | render (clears cached hit tests, §12) | |
| `interaction` | `ToolTip` | `Option<String>` | `None` | | |
| `identity` | `Classes` | `Classes` (space-separated names) | empty | style, through its signal (#107) | |
| `identity` | `Name` | `Option<String>` | `None` | style, through its signal (#107) | |

`DataContext` arrives with data binding (M5), whose `DataRef` is its type and which makes it inherit. An `Option<NodeId>` now would make it a child-element slot, since that's what a `NodeId` property means to the registry.

`RenderTransform` is written as CSS transform functions: `translate(x, y)` or `translate(x)`, `rotate(degrees)`, `scale(s)` or `scale(x, y)`, such as `RenderTransform="translate(10, 0) rotate(45)"`. As in CSS, whitespace between functions is optional, none may come between a name and its parenthesis, and the rightmost function applies to the element first: that example rotates, then moves.

The size properties and the alignments are what #41's `Sizing` holds; `Layout` builds a `Sizing` from them for `measure_with` and `arrange_with`.

A control changes a group's default with `#[ui(override_defaults)]` and an `OverrideDefaults` impl, which the type's `Default` calls once, after building the fields:

```rust,ignore
impl OverrideDefaults for Button {
  fn override_defaults(&self) {
    self.interaction.focusable.set_default(true);
    self.interaction.cursor.set_default(Cursor::Hand);
  }
}
```

`Property::set_default` sets the `Default` level, as Avalonia's `OverrideDefaultValue` does, so an inherited value, a style, a binding or a UI language all win over it. Overrides are plain code, one line each, for any value type, the control's own properties included.

`Visibility`, `Cursor` and `Classes` live in `artic-element`, with `#[derive(UiEnum)]` for the enums. The alignment enums already live in `artic-layout`, which is on the registry's layer (1) and so can't depend on it; they derive `UiEnum` too, whose output names only `artic-core` ([panels](panels.md)).

### Visibility

There is one visibility property, `Visibility`, with three states. Two settable properties for one fact would need a rule for when they disagree, so there is no `IsVisible` beside it.

The three states are what web and mobile developers already know: CSS `visibility: hidden` and `display: none`, Android's `VISIBLE` / `INVISIBLE` / `GONE`, Flutter's `Visibility`. A boolean `IsVisible`, as in Avalonia, was considered and rejected: "hidden but keeps its space" would then be `Opacity = 0`, which still takes clicks and is still read by screen readers unless `IsHitTestVisible` and an accessibility setting are changed too. `Hidden` does all three. This reverses the earlier naming review (#239); see `naming-review.md`.

| `Visibility` | Takes space | Drawn | Hit-testable | accesskit |
| --- | --- | --- | --- | --- |
| `Visible` | yes | yes | yes | normal |
| `Hidden` | yes | no | no | not emitted |
| `Collapsed` | no: not laid out and no rectangle; its parent doesn't see it | no | no | not emitted |

Whether an element and all its ancestors are visible is computed when needed, for example by a future `is_visible(tree, node)`, never set.

`LayoutManager` leaves collapsed nodes out. `#[element]` implements `Layoutable::collapsed` from `Visibility`, and the manager drops such a node from its parent's `children()`, ignores direct `measure` and `arrange` calls on it, measures the parent again when the answer changes, and lays out nothing for a collapsed root. Children a measure didn't measure are still forgotten with their subtrees, which keeps a collapsed node's descendants out of the dirty and waiting passes. `artic-layout` still never names `Visibility`.

## 6. Registry: flattened groups

`#[derive(UiType)]` skips fields without `#[ui]`, so a plain group field would be missing from the registry and from `visit_properties`: `Margin` couldn't be set from a UI language, changing it wouldn't mark the node dirty, and `IsEnabled` wouldn't inherit. The fields `#[element]` adds are marked `#[ui(flatten)]`:

```rust,ignore
#[element]
#[derive(Debug)]
pub struct Border {
  // #[element] adds: #[ui(flatten)] pub layout: Layout, and the other three groups
  #[ui(content)] pub child: Child,
}
```

For a flattened field, `derive(UiType)`:

- calls `self.layout.visit_properties(visit)` from the struct's own `visit_properties`;
- adds `PropertyGroupInfo { field: "layout", properties: <Layout as PropertyGroup>::PROPERTIES }` to the type's `TypeInfo.groups`.

A control has one entry per group, in the order the fields are declared.

A state struct that holds properties is flattened the same way.

The loader's per-owner thunks (M2, [loader values](loader-values.md)) take a tree and a node and downcast the node to the property's owner, so one shared `Layout` thunk couldn't reach a `Border`'s margin. They'll need descriptors generated per owner, through generics: `Flattened<G>`, which the derive implements to reach the group, and `GroupOf<G, O>`, whose associated `const` the compiler instantiates for each control. Both arrive with those thunks; until then, `PropertyInfo` holds no thunk, and one list per group serves every owner.

Lookups search the type's own properties, then each group, and the compiler writes `node.layout.margin.set(...)`. A name must be unique across a type and all its groups: a control property named `Margin` would otherwise hide `Layout`'s, so a UI language would set a value that layout never reads. The derive can't see a group's names, which come from another crate, so the registry checks when it's built and reports a clash as a new `Duplicate::Property`, as it does for a Rust path registered twice. A slice of groups, not one merged list, because a `const` can't join two slices whose lengths come from different crates. Spec §3's `Border` gains the attribute.

## 7. Accessibility and composition

The shared work, `Interaction::accessibility`, builds the default accesskit node: the arranged bounds, and disabled when `IsEnabled` is false. Only `Visible` elements get a node at all, which the accesskit walk (#54) decides from `Layers.visible`, not the node builder: like composition and hit testing, the walk skips a node that isn't `Visible` together with its whole subtree, so a `Visible` child of a hidden parent is never exposed. The tree is updated from render-dirty nodes, and a change of `Visibility` marks only the node that changed, so a node that becomes `Visible` again has its whole accessible subtree rebuilt, not just its own node; otherwise its unchanged descendants would stay missing. Layout moves nodes without marking them render-dirty, so the update also covers every node `LayoutManager` gave a new rectangle. Each node's bounds are in its parent's coordinates, placed with accesskit's per-node transform, so a moved node updates alone and its descendants don't change. Each control adds its role, name, value and actions in `ElementImpl::accessibility`, which `#[element]` calls with that node (#161 adds the attached properties).

`Opacity`, `RenderTransform`, `Visibility` and `ZIndex` feed #33's per-node layers, not each control's `render`. `#[element]` writes `Element::layers` from `appearance`, and the pipeline's render `Nodes` answers `layers(tree, node)` by casting the node with `ElementType::cast` and calling it with the node's arranged size from `LayoutManager`, so app-defined controls work the same way: opacity becomes the node's opacity layer, the transform its transform layer, and a control's `ElementImpl::clip`, such as a scroll viewer's viewport, its clip layer, which fences the node's descendants but not its own drawing ([border design](border.md)). Visibility needs a layer of its own: hiding only the node's own drawing would still draw its children. `Layers.visible`, default `true`, makes the compositor skip a node with `visible: false` together with its whole subtree, which keeps its fragments and render flags so it comes back without recording what didn't change. `Hidden` and `Collapsed` answer `visible: false`. Render flags stay set on a hidden subtree, so a later consumer of "render-dirty" (#54's accesskit update, a frame scheduler) has to look past hidden subtrees.

The transform `Element::layers` returns is the element's own: `RenderTransform` pivoting on the element's centre, as CSS's `transform-origin` defaults to, in the element's coordinates. The node's place in its parent comes from layout, so the frame pipeline's render `Nodes` applies the node's layout offset after this transform ([frame design](frame.md) §5).

`Opacity` is a plain `f32`, so a Rust write can set any value, while `Layers.opacity` must be from 0 to 1. The property itself keeps whatever was set, and reading it returns that value. Only the shared work's building of the node's `Layers` clamps it to 0–1, with NaN counting as 1, so composition never sees an invalid value.

`ZIndex` orders siblings. `Layers.z_index`, default `0`, makes the compositor draw a node's children sorted by it, stably, so equal values keep tree order. The hit-test walk (#74) uses the same order, reversed, so what is hit is what is drawn on top.

`Classes` and `Name` decide which style selectors match (§8) and have no direct effect on layout or rendering. Like every property, each is a signal, and the style index (#107) learns of changes by depending on them and re-evaluates the selectors, so they need no `Affects` flag. A style that starts or stops matching changes properties such as `Width` or `Opacity`, and those changes carry their own invalidation.

Hit testing follows the same rule: `#[element]`'s `hit_test` misses when the element isn't `Visible` or its `IsHitTestVisible` is false, and the hit-test walk (#74) doesn't descend into such a node. `IsHitTestVisible` is the drawn-but-click-through case that `Visibility` can't express, for overlays and decorations; spec §12 relies on it.

A disabled element is different: it gets no input, but it still blocks it. Hit testing finds it as usual, so a click on a disabled button lands on the button and doesn't fall through to what's behind it, as with HTML's `<button disabled>`. `#[element]`'s `on_event` then returns `Handled::No` without calling `ElementImpl::on_event` when `IsEnabled` is false, so no control has to check it. `IsEnabled` combines with its ancestors through `inherit::and`, so a disabled panel disables everything inside it, for pointer, keyboard and every other input. A child's own `true`, from a UI language or a binding, can't re-enable it: its effective value is its own AND its parent's, and `Property::own` still reads what it asks for. Focus navigation (#78) skips disabled elements. Tooltips come from the pipeline's hover tracking rather than `on_event`, so a disabled control can still show one.

## 8. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | `artic-element`: `Element`, `ElementImpl`, `HasState`, `ElementType` and `element_type!`, the placeholder contexts; `InputEvent` in `artic-core` | a hand-written element is laid out through `LayoutManager` by way of the cast table |
| B | `ElementBase` (now the property groups) with its 20 properties and `#[derive(UiProperties)]`, the enums, `FromStr` for the alignments and `Transform`, `#[ui(flatten)]` in `derive(UiType)`, `PropertyGroupInfo`, `ArrangeCx::set_bounds`, `Duplicate::Property`, the measure and arrange outline, `LayoutManager` forgetting unmeasured children | `element_base_properties_registered` (now `groups_registered`), `margin_and_alignment_layout`, `min_max_clamp_desired_size`, `collapsed_takes_no_space` (including a child made dirty after its parent collapsed), a registry test where a control property clashes with a group's |
| B2 | the four property groups in `artic-element`, replacing `CommonProperties`; `Property::set_default` | `groups_registered`, `set_default_loses_to_every_other_level` |
| C | `#[element]`: the four fields and their reserved names, the `UiType` output with `#[ui(override_defaults)]` calling `OverrideDefaults` from `Default`, `__private` in `artic-element` re-exporting what that output names, the outline from section 3, `#[element(state)]` and its duplicate-state error, `crate_root` taking each macro's candidate crates | the trybuild cases: a field with a reserved name, a struct that also derives `UiType`, a tuple struct, two fields of one state type, an unknown setting in a field's `#[element(...)]`; a control built by `Default` and by `TypeInfo.new` has its overridden defaults (`knob_overrides_group_defaults`); a test-only state giving a test trait through a blanket impl; a control whose `OverrideDefaults` makes it focusable, with a style setter winning over that default; a unit test that a disabled element's hook isn't called |
| D | `Layers.visible` and `Layers.z_index`, the compositor skipping hidden subtrees and drawing siblings by `ZIndex`; `Appearance::layers` filling opacity, the centred transform, visibility and z-index; the disabled flag on the default accesskit node | `hidden_node_skips_its_subtree`, `subtree_returns_when_visible_again`, `children_draw_by_z_index`, `layers_clamp_opacity`, `transform_pivots_on_the_centre`, `layers_carry_visibility_and_z_index`, `accessibility_node_is_disabled_with_the_element` |

The frame pipeline doesn't exist yet, so D stops at the element and the compositor. The accesskit tree and its updates (a hidden parent's child left out and restored, bounds following a layout-only change) move to #54, the render `Nodes` adapter that adds layout offsets to #40, and the hit-test order to #74. The opacity pixel test is `artic-vello`'s existing `opacity_blends_the_layer`, with `layers_clamp_opacity` covering how the property reaches the layer.

## 9. Spec changes

- §7: `Element: Object + Layoutable` instead of `Element: XamlObject`; `ElementImpl` and `HasState` added; `IsVisible` replaced by `Visibility` and its table.
- §3: `Border` gains `#[element]`; `TypeInfo` gains `groups`.
