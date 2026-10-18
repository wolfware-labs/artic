# Design: panels

Issue: #44. Builds on: layout (#41), [elements](element.md) (#42), [layout tests](layout-tests.md) (#56), `UiEnum` (#19), [golden images](golden-images.md) (#38). Unblocks: `Grid` (#67), `DockPanel` (#68), `WrapPanel` and `UniformGrid` (#69), `Canvas` (#70), the accesskit adapter (#54), #49.

`StackPanel` is the first layout container: it holds children and lines them up in a row or a column, with a background drawn under them. Building it the way the rest of the panels will be built needs a few changes underneath first: how `#[element]` and `#[derive(UiType)]` share work, where a type's XAML content is declared, what the property groups are called, how layout treats `Collapsed`, and where children live.

```rust,ignore
#[element]
#[derive(Debug)]
pub struct StackPanel {
  #[ui(flatten)] pub panel: Panel,
  #[ui(content)] pub children: Children,
  #[ui(affects = layout)] pub orientation: Property<Orientation>,
  #[ui(affects = layout)] pub spacing: Property<f32>,
}

impl ElementImpl for StackPanel {
  fn measure_override(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size { /* section 5 */ }
  fn arrange_override(&self, cx: &mut ArrangeCx<'_>, size: Size) { /* section 5 */ }
  fn render(&self, cx: &mut RenderCx<'_>, scene: &mut SceneBuilder) {
    self.panel.render(cx, scene);
  }
}
```

Shared behavior is composed explicitly. A property group holds data and methods, and a control calls the methods it uses from its `ElementImpl` hooks. Nothing runs because a field's type says so, so a control's hooks show everything it does. Groups that hook into the element by themselves were considered and rejected: they behave like base classes, their hooks need an order, and the four built-in groups depend on each other's fields (`hit_test` reads `visibility`), which would turn into coupling between hooks.

## 1. Building controls

### `#[element]` and `#[derive(UiType)]`

`#[element]` adds `#[derive(UiType)]` to the struct it rewrites, instead of calling the derive's functions itself. It keeps the work that makes a struct an element: it rejects generic and tuple structs, a written `derive(UiType)` or `derive(Default)` and the reserved field names; adds the four group fields; implements `HasState` for `#[element(state)]` fields; and writes `Layoutable`, `Element` and `element_type!`. It strips only its own `#[element(...)]` attributes and leaves `#[ui(...)]` to the derive. A control crate may depend on `artic-element` alone, which isn't one of `UiType`'s candidate crates, so `#[element]` passes on the crate it resolved, as it does today: it adds `#[ui(crate = …)]` naming that crate, unless the struct already sets `crate`, and the derive reaches the registry through that crate's `__private`. For the same reason the derive is named by its full path, `#[derive(<crate>::__private::UiType)]`: `artic-element` and the `artic` facade re-export the derive macro in `__private` beside the trait of the same name (a macro and a trait live in different namespaces, as serde's `Serialize` does), so the control crate needs no `artic-macros` dependency or import.

### Overriding defaults

A property's default is set where it's declared, with `#[ui(default = value)]`. A type that changes the default of a property it doesn't declare, one from a flattened group, says so with a flag on the struct and implements `OverrideDefaults`, a trait in `artic-registry`:

```rust,ignore
#[element]
#[derive(Debug)]
#[ui(override_defaults)]
pub struct Button { /* … */ }

impl OverrideDefaults for Button {
  fn override_defaults(&self) {
    self.interaction.focusable.set_default(true);
    self.interaction.cursor.set_default(Cursor::Hand);
  }
}
```

With the flag, the derive's `Default` builds every field and then calls `<Self as OverrideDefaults>::override_defaults(&value)`; without it, `Default` builds the fields and stops. The flag is needed because a derive sees only its struct, never an `impl` block, so the struct has to say that the impl exists. The flag is named after its trait so the one leads to the other, and isn't called `defaults`, which would sit one letter from the field-level `default = …` and read like serde's `#[serde(default)]`.

Elements use the same flag and trait as any other type, so `ElementImpl::defaults` is removed and `#[element]` has nothing to do with defaults. A struct-level list, `#[ui(default(interaction.focusable = true, …))]`, was considered and rejected: a control that changes several defaults would carry a long attribute.

### Content on the field

The member that receives a type's content is marked where it's declared, with `#[ui(content)]`, and the struct-level `#[ui(content = name)]` is removed, so renaming it can't leave a stale name behind. Content comes in two kinds, and `TypeInfo.content` says which, taking the place of the [loader design](loader-values.md)'s separate `TypeInfo.add_child`:

```rust,ignore
pub enum ContentInfo {
  /// A property that takes the content as its value, such as `ContentState`'s
  /// `Property<Content>`, which accepts text, an element or data (#84). The loader sets it
  /// through the property's `PropertyAccess`, as for any property.
  Property(PropertyRef),
  /// A collection field that takes child elements: `Children` or `Child`, which link them into
  /// the tree (section 4), or a value collection such as `Vec<GradientStop>`, which takes each
  /// staged value out of the tree into the field.
  /// `add_child` is generated by the derive: it calls the field's `UiCollection::push_child`
  /// with an accessor to the field built in.
  Collection {
    field: &'static str,
    add_child: fn(&mut UiTree, NodeId, NodeId) -> Result<(), ValueError>,
  },
}
```

A property may be marked in a property group, as `ContentState.content` is: the group's derive exposes `const CONTENT: Option<&'static PropertyInfo>`, and the outer derive builds the `PropertyRef` with the group's field, the way `PropertyRef` already names group properties. A `Children` or `Child` field is declared on the type itself, as every panel's `children` is (section 5); a group holds properties and the methods that use them. A type has at most one content member, counting its groups: the derive checks its own fields, and a `const` assertion over its groups' `CONTENT` checks the rest. A single child is a `Child` field, not a `Property<Option<NodeId>>`, so the tree link has no second copy; content that may be text or data stays a property, since it isn't always a node.

### Group names

The property groups are renamed after their fields: `LayoutProperties` becomes `Layout`, `AppearanceProperties` `Appearance`, `InteractionProperties` `Interaction` and `IdentityProperties` `Identity`. Each group holds methods as well as properties (`Layout::measure`, `Interaction::hit_test`), so the suffix described only half of them, and `field: Type` now reads the same everywhere: `layout: Layout`, `panel: Panel`. The derive keeps its name, `UiProperties`, since properties are what it generates. The test-local `Panel` structs in `artic-platform/tests/common/mod.rs` and the `artic-core` tree docs are renamed so that `Panel` names one thing.

## 2. Enums in `artic-layout`

`#[derive(UiEnum)]` generates a `FromStr` whose error is `ParseEnumError`, which lives in `artic-core`. The derive names it through the first of its candidate crates the calling crate depends on. `UiEnum` gets a candidate list of its own, `artic`, `artic-registry`, then, new here, `artic-core`, whose `__private` module re-exports `ParseEnumError`; `UiType` keeps its list without `artic-core`, since its output needs the registry's descriptors. Until now the candidates stopped at `artic-registry`, which is why `artic-layout`, depending only on `artic-core`, parsed its two alignments by hand.

`artic-layout` depends on `artic-macros` and derives the parser for `HorizontalAlignment`, `VerticalAlignment` and `Orientation`; the hand-written parsers are deleted:

```rust,ignore
/// The axis a panel stacks or wraps its children along.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, UiEnum)]
pub enum Orientation {
  Horizontal,
  #[default]
  Vertical,
}
```

`Orientation` sits in `sizing.rs` beside the alignments, where a contributor looks for layout vocabulary, and `artic-layout` re-exports it from its root with them (`pub use sizing::{HorizontalAlignment, Orientation, …}`). The new edge makes no cycle: `artic-macros` depends on no artic crate, and its output names crates the caller already depends on. A proc macro runs on the build machine, so the web target gains nothing.

## 3. Collapsed children

A `Collapsed` element takes no part in layout. Today it measures to zero, so a panel can't tell it from a visible element that is empty, and `StackPanel` would put spacing around it. `artic-layout` learns the idea without naming `Visibility`:

```rust,ignore
pub trait Layoutable {
  fn measure(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size;
  fn arrange(&self, cx: &mut ArrangeCx<'_>, rect: Rect);
  /// Whether the node takes no part in layout. By default, false.
  fn collapsed(&self) -> bool {
    false
  }
}
```

`#[element]` implements it from `appearance.visibility`. The manager leaves collapsed children out of `MeasureCx::children()` and `ArrangeCx::children()`, and so out of `measure_children` and `arrange_children`, so no panel writes code for them. A container that names a collapsed child directly gets the same answer: `MeasureCx::measure` returns zero without measuring it and `ArrangeCx::arrange` does nothing, so no path leaves it with a size or rectangle. Its subtree is covered by the rule `element.md` already gives the manager: children a measure didn't measure are forgotten with their subtrees, and the dirty and waiting passes don't measure forgotten nodes. The upward pass measures a dirty node only in the size it was last offered, and a forgotten node has none, so the change is passed to its parent instead, up to the nearest ancestor that is laid out, whose measure leaves the collapsed child out again. A descendant invalidated while its ancestor stays collapsed is therefore not measured. A collapsed node keeps no results: `desired` and `rect` return `None`, as for a child a measure skipped. A collapsed root lays out nothing: the measure pass that both `run` and `LayoutManager::measure` go through checks the root first and, when it's collapsed, drops the results of the root and its subtree and returns without measuring it, and `run` then arranges nothing.

Collapsing a child used to reach its parent as a desired size that changed to zero. Now the child isn't measured, so the manager watches membership instead: when a dirty node's `collapsed()` answer differs from the last pass, its parent's measure is marked dirty, the same path a removed child takes. `Layout::measure` and `Layout::arrange` lose their `Collapsed` branch, since the manager no longer calls them for a collapsed node; `Hidden` is measured and arranged as before.

The walks that follow layout skip a node it didn't lay out, with its subtree. Composition already skips a node and its subtree when `Nodes::layers` returns `visible: false` (the compositor's walk), so `ElementNodes::layers` returns that for a node with no layout rectangle, instead of drawing it at `Rect::ZERO`; hit testing follows the same rule, and #54's accesskit walk will too. `element.md`'s `Collapsed` row becomes: not laid out, no rectangle, skipped by drawing, hit testing and accessibility.

Layout arithmetic gets one rule for overflow, in place of fixes at each place a sum can grow: every addition, subtraction and scaling `artic-layout` does on positions and sizes saturates at the finite range when its operands are finite, returning `f32::MAX` or `f32::MIN` (that is, `-f32::MAX`) by sign instead of an infinity. An infinite operand is not overflow: an infinite available size, which `StackPanel` offers on its stacking axis, stays infinite through `available - margin` and the like, so a child still sees that it may take as much as it wants. That covers the `Rect`, `Size` and `Thickness` operations (`snap`, `deflate`, `inflate`, side totals, sums) and the position and alignment arithmetic in `measure_with` and `arrange_with`, which all go through one pair of private helpers, `add` and `scale`, so a new calculation can't miss the rule. Finite inputs then always give finite rectangles, which keeps the manager's rule, a finite desired size and rectangle, true however large the values a panel adds up, without each panel guarding every addition.

## 4. Children

The tree owns a panel's children; a panel keeps no list of its own. Two copies of "who are my children" would drift, and a method on the panel couldn't update both, since the panel lives inside the tree it would have to borrow. `Children` is an empty type that marks the content field, in `artic-element`:

```rust,ignore
/// Marks an element's children, as a UI language writes them: they become its logical and visual children.
#[derive(Debug, Default, Clone, Copy)]
pub struct Children;

impl Children {
  pub fn add(tree: &mut UiTree, parent: NodeId, child: NodeId);
  pub fn insert(tree: &mut UiTree, parent: NodeId, index: usize, child: NodeId);
  /// Detaches `child` from `parent` in both trees; the node stays allocated.
  pub fn remove(tree: &mut UiTree, parent: NodeId, child: NodeId);
}
```

A single child works the same way, as a collection of at most one:

```rust,ignore
/// Marks an element's single child, as a UI language writes it: its only logical and visual child.
#[derive(Debug, Default, Clone, Copy)]
pub struct Child;

impl Child {
  pub fn get(tree: &UiTree, parent: NodeId) -> Option<NodeId>;
  /// Replaces the child. Checks first that the current child, if any, is `parent`'s child in
  /// both trees and that `child` can be linked in both, then detaches the old child and links
  /// `child`. When a check fails, nothing changes.
  pub fn set(tree: &mut UiTree, parent: NodeId, child: Option<NodeId>);
}
```

`Children` and `Child` are for content whose logical and visual parent are the same element, as a panel's and a `Border`'s are. A templated control (#84) is different: its content is a logical child of the control and a visual child of a template part, which #84 links itself. So the "same parent in both trees" rule lives with `Children` and `Child`, not in `UiTree`.

The two trees can disagree, since a template gives a node different logical and visual parents, so a change to both is checked in both before either is made: otherwise the second link could fail after the first succeeded and leave the trees out of step. `UiTree` exposes the checks its link methods already make, as `check_link(tree_kind, parent, index, child) -> Result<(), LinkError>` (a node not in the tree, an index past the end, a cycle), and `Children` runs it for both trees first. `remove` takes the parent and checks that `child` is its child in both trees, so it can't detach a node from another panel.

`Children::add`, `insert` and `remove` panic when a check fails, as the `UiTree` methods do: in code, those are bugs. The loader reaches them through `UiCollection`, which returns the failed check as an error instead, since markup is input.

`UiCollection` changes so one trait serves both a collection that lives in the tree and one that lives in its field. The derive generates `add_child` for the content field, passing an accessor from the owner node to that field:

```rust,ignore
pub trait UiCollection: Sized {
  /// Adds `child` as content of `owner`. `field` reaches this collection inside the owner.
  fn push_child(
    tree: &mut UiTree,
    owner: NodeId,
    child: NodeId,
    field: for<'a> fn(&'a mut UiTree, NodeId) -> Option<&'a mut Self>,
  ) -> Result<(), ValueError>;
}

// generated for StackPanel's content field:
//   |tree, owner, child| Children::push_child(tree, owner, child,
//     |tree, owner| tree.get_as_mut::<StackPanel>(owner).map(|panel| &mut panel.children))
```

Every implementation calls `field` first, and refuses with `ValueError::WrongNode` an owner it doesn't reach, a node of another type, before changing anything. `Children` then links `child` into both trees, returning `ValueError::InvalidChild` (new), with the `LinkError`, when a check fails. `Child` does the same, and also refuses a child when the owner has one already, since a UI language gives a single-child element one child. A value collection such as `LinearGradientBrush`'s `Vec<GradientStop>` ([value objects](value-objects.md)) will be served by one implementation in `artic-registry`, `impl<T: FromUiObject> UiCollection for Vec<T>` (with #57), which works in three steps so no two borrows of the tree overlap: it calls `field` to check the owner and lets that borrow end, takes the staged value out of the tree with `T::from_object`, then calls `field` again to push it, which can't fail once the first call succeeded; no crate that defines a value type needs its own. The `Vec<NodeId>` implementation is removed; nothing calls `push_child` yet. Its remaining uses move with it, in the same pull request: the collection check in `artic-registry`'s `__private`, the `children: Vec<NodeId>` fields in `artic-macros/tests/derive.rs`, and the expected output of the `ui_on_plain_field` trybuild case become `Children`.

The builder ([builder API](builder-api.md), #43) takes children in the same expression as the properties, before `build` has the tree. The ids wait in a wrapper, not in the node, since a relationship is tree data:

```rust,ignore
let ok = Button::new().content("Hi").build(tree);
let panel = StackPanel::new()
  .spacing(8.0)
  .children([ok])   // WithChildren<StackPanel, [Handle<Button>; 1]>
  .build(tree);     // builds the node, then links each child

/// A node waiting to be built, with the children to link once it is.
pub struct WithChildren<T, I> {
  node: T,
  children: I,
}

impl<T: UiType + Object, I: IntoIterator<Item: Into<NodeId>>> WithChildren<T, I> {
  pub fn build(self, tree: &mut UiTree) -> Handle<T>;
}
```

`build` checks that every item is in the tree and appears once before it allocates the node, and panics if not, since a repeated item would be linked twice and end up once: in code that is a bug, as for `Children::add`. Once those checks pass no link can fail, because a node that was just allocated has no ancestors to make a cycle and no children to collide with, so a failed build never leaves a parent or some of its children in the tree.

`WithChildren` is only for child elements, which are tree data. A value collection such as `Vec<GradientStop>` is node data, so its builder method writes the field directly, as a property's does: `LinearGradientBrush::new().gradient_stops([a, b])`, with the values built inline as the [value objects](value-objects.md) design builds them. The derive generates `.children(items)` for a `Children` field and `.child(item)` for a `Child` field, which wraps `Some(item)` in the same `WithChildren`; a leaf gets neither. `WithChildren::build` adds each item through the type's content field, the path a UI language takes, so a second child for a `Child` field is refused the same way in both. `WithChildren` has no property methods, so `.children` comes last before `.build`, as children come after attributes in XAML. It is one generic type, unlike the per-type builder types `builder-api.md` rejects, and that document's "`build` links its children" becomes "`WithChildren::build` links them". The builder methods don't exist yet, so `WithChildren` and the generated `.children` come with #43; this design fixes their shape.

## 5. `Panel` and `StackPanel`

`Panel` is the property group every panel flattens, in `artic-controls`:

```rust,ignore
/// What every panel has: a background drawn under its children.
#[derive(Debug, UiProperties)]
pub struct Panel {
  #[ui(affects = render)]
  pub background: Property<Option<Brush>>,
}

impl Panel {
  /// Fills the element's bounds with `Background`, if it's set.
  pub fn render(&self, cx: &RenderCx<'_>, scene: &mut SceneBuilder);
}
```

Every panel's `Background` is the same property with one key, so a style that sets `Background` works on any panel. Each panel declares its own `#[ui(content)] children: Children`, so a type's content is visible where the type is. There is no `<Panel>` element.

`StackPanel` has `Orientation`, `Vertical` by default, and `Spacing`, 0 by default. Below, the stacking axis is height when `Vertical` and width when `Horizontal`, and the cross axis is the other.

| Step | Rule |
| --- | --- |
| Measure | each child is offered the panel's available cross size and infinity on the stacking axis. The desired stacking size is the children's sum plus `Spacing` between each pair; the desired cross size is the largest child's. No children gives 0×0 |
| Arrange | children are placed from the start of the stacking axis with `Spacing` between them. Each slot is as long as the child asked for on the stacking axis and as wide as the panel on the cross axis; the child's own margin and alignment place it inside. Space left over stays empty, and children past the end are still arranged |
| `Spacing` | a value that isn't a finite number of 0 or more counts as 0, so children don't overlap by accident |
| Overflow | sums and positions along the stacking axis use the saturating operations of section 3, so a huge `Spacing` or huge children can't add up to infinity |
| Render | `self.panel.render(cx, scene)` |
| Accessibility | nothing added: the default role is `GenericContainer` |

Measure and arrange are each written once, over a private `Axis` helper that splits a size into its stacking and cross parts and joins them back.

## 6. Accessibility

Elements return their own node; the tree of nodes is built by #54's walk, which takes children from the visual tree in order and skips nodes layout didn't place. Child order is that walk's job for every element, so `StackPanel` only checks its role here, and #54's `stack_panel_children_in_visual_order` checks its children's order, with collapsed children left out.

## 7. Documents that change

| Document | Change |
| --- | --- |
| `docs/architecture.md` | the group types renamed in the `artic-element` rule (`Layout`, `Appearance`, `Interaction`, `Identity`); `artic-layout` depends on `artic-core` and `artic-macros`; `artic-macros`' generated code may also name `artic_core::__private`; the rule about `artic-macros`' own tests covers dev-dependencies on any crate that uses the macros, `artic-element` included; the adapter rule gains that an adapter's tests may dev-depend on `artic-controls` for control goldens, since goldens belong to each renderer (#38), while the adapter's library still depends only on `artic-render` and `artic-platform` |
| `docs/design/builder-api.md` | children wait in `WithChildren`, and `WithChildren::build` links them in both trees; a single-child slot is a `Child` field, not `Property<Option<NodeId>>`, with `.child(item)` |
| `docs/design/loader-values.md` | `TypeInfo.add_child` becomes `TypeInfo.content: Option<ContentInfo>`; `ValueError` gains `InvalidChild(LinkError)` |
| `docs/design/element.md` | the group names; `ElementImpl::defaults` replaced by `override_defaults`; the `Collapsed` row and the paragraph on forgetting skipped children |
| Spec crate table | panels live in `artic-controls`, not `artic-layout` |
| Spec §3 | the `Border` example and the content rule use `#[ui(content)]` on the field, with `child: Child`; `#[ui(override_defaults)]` and `OverrideDefaults` join the attribute list; `UiEnum` is usable from any crate that depends on `artic-core` |
| Spec §7 element composition | the group types are `Layout`, `Appearance`, `Interaction` and `Identity`; a control changes a group's default through `override_defaults`, not `ElementImpl::defaults`; `Collapsed` takes no part in layout and has no rectangle |
| Spec §7 Panels | `Panel` is a property group with `Background`; each panel has its own `Children` |

## 8. What waits

| Feature | How it fits | Tracked in |
| --- | --- | --- |
| `Grid`, `DockPanel`, `WrapPanel`, `UniformGrid`, `Canvas` | each flattens `Panel`, declares `Children` and writes measure and arrange | #67 to #70 |
| Child order in the accesskit tree | the walk in section 6 | #54 |
| `.children(…)` on the builder, through `WithChildren` | comes beside the other builder methods | #43 |
| Filling `Children` from markup | the loader calls `UiCollection::push_child` | the XAML runtime and compiler |
| Virtualization | a different panel over the same `Children` | #131 |

## 9. Testing

| Crate | Tests |
| --- | --- |
| `artic-macros` | `content_child_on_a_field`, `content_children_on_a_field`, `content_property_in_a_group`, `two_content_fields`, `struct_level_content`, `override_defaults_runs_after_field_defaults`, `override_defaults_without_an_impl`, `element_adds_derive_ui_type` |
| `artic-layout` | `collapsed_children_are_left_out`, `collapsing_a_child_remeasures_the_parent`, `showing_a_child_again_measures_it`, `collapsed_root_lays_out_nothing`, `measuring_a_collapsed_child_directly_does_nothing`, `measure_of_a_collapsed_root_is_none`, `infinite_available_stays_infinite`, `dirty_descendant_of_a_collapsed_child_isnt_measured`, the property test `geometry_stays_finite` (snap, deflate, inflate and `arrange_with` with every alignment, over huge finite rectangles, margins and scales), `alignments_parse_with_the_derive`, `orientation_parses_ignoring_case` |
| `artic-render` | `nodes_not_laid_out_are_not_drawn` (with a child under the node, which isn't drawn either) |
| `artic-element` | `panel_children_update_tree_links` (add, insert and remove, in both trees), `push_child_refuses_nodes_not_in_the_tree`, `add_checks_both_trees_before_linking`, `child_set_replaces_in_both_trees`, `child_set_leaves_a_mislinked_child_alone`, `second_child_is_refused`, `build_refuses_a_repeated_child`, `remove_leaves_another_parents_child_alone`, `knob_overrides_group_defaults` |
| `artic-controls` | `stack_vertical_measure`, `stack_horizontal_measure`, `stack_spacing_skips_collapsed`, `stack_arrange_positions_children` (both orientations), `stack_spacing_ignores_invalid_values`, `empty_stack_is_zero`, `stack_children_are_its_content`, `stack_panel_accesskit_node` (the role; child order is checked by #54's `stack_panel_children_in_visual_order`), and the property test `stack_invariants_hold` over random children in both orientations |
| `artic-vello` | the golden `stack_panel_background`: a panel with a background and three fixed-size children |

The `artic-controls` tests use `artic_test::layout::Headless` with a `LayoutNodes` that asks `ElementTypes` first and falls back to `Boxes`, so the panel is a real element and its children are `Fixed` leaves; `artic-controls` takes `artic-test` as a dev-dependency. The golden follows #38's rule that goldens belong to each renderer: it lives in `artic-vello/tests/goldens.rs` and builds its tree there, and `artic-vello` takes `artic-controls` as a dev-dependency, which the adapter rule in `architecture.md` is updated to allow for tests (section 7).

## 10. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | this document | approved |
| B | section 1: `#[element]` adding `derive(UiType)`, `override_defaults`, `#[ui(content)]` on fields, the group renames, with no change in behavior | the macro tests pass and every existing test passes after the renames |
| C | sections 2 and 3: `UiEnum` from `artic-core`, `Orientation`, the derived alignment parsers, `collapsed()`, the manager's filter, composition and hit testing skipping nodes not laid out, the saturating geometry, and the `architecture.md` and `element.md` changes | the layout and render tests pass |
| D | sections 4 and 5: `Children`, `UiCollection` on the tree, `Panel`, `StackPanel`, the controls tests and the golden; closes #44 | the tests in section 9 pass |

B and C are independent; D needs both.
