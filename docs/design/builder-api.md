# Design: the builder API that generated code targets

Issue: #21, amended for #43. Builds on: `derive(UiType)` (#18), [value objects](value-objects.md) (#32), geometry (#22). Implemented by: #43 (builder generation in the derive), #49 (compiler emission), #31 (attached-property helpers).

The XAML compiler and, in v2, hand-written Rust views both build trees through one API: `Button::new().content("Hi").on_click(…)` (D8; `on_click` comes with routed events, #75). This document fixes that API's shape, so every later codegen issue follows it.

## Constraints

- A node ends in the tree through `UiTree::alloc(Box<dyn Object>) -> NodeId` (#7). There is no second allocator.
- A property's builder value is its `Local` level: `Property<T>::set` (#13). There is no separate "initial value" path.
- Which fields are properties, events, content or collections comes from #18's field analysis; names come from the field names and the registry vocabulary (#17).
- `Property<T>` values live in the reactive runtime, not in the arena, so a property can be set before its node is allocated.
- Rust allows one mutable borrow at a time: a value holding `&mut UiTree` can't also build its children in the same chain.

## Where the tree enters

The same `StackPanel` with two children, under each option:

| Option | Shape | Verdict |
| --- | --- | --- |
| Arena at the start | `StackPanel::build(tree).spacing(8.0).child(\|tree\| Button::build(tree).content("Hi"))` | Rejected: the builder holds `&mut UiTree`, so children must be built in closures, and generated code nests one closure per level |
| Arena at the end, unbuilt children inside the builder | `StackPanel::new().child(Button::new().content("Hi")).build(tree)` | Rejected: unbuilt children must be stored as `Box<dyn …>`, and every node type needs a separate builder type |
| **Arena at the end, children built first** | `let ok = Button::new().content("Hi").build(tree);` then `StackPanel::new().spacing(8.0).children([ok]).build(tree)` | **Chosen**: no closures, no `dyn`, no builder types |

## The builder is the node struct

- `T::new()` is the `Default` that `#[derive(UiType)]` generates: every property created with its key and default.
- Every builder method takes `self` by value, writes one member, and returns `Self`. `Property::set` takes `&self`, so the methods don't need `mut self`.
- `build(self, tree: &mut UiTree) -> Handle<T>` allocates the struct and returns a typed handle. A node with children is built by `WithChildren::build` instead, which links them after allocating ([children](#children)); linking a node resolves its inherited values (#15).

Children are built before their parents. The compiler emits one `let` per element in post-order, so generated code stays flat. Hand-written code can still nest, because the inner `build(tree)` finishes its borrow before the outer call starts:

```rust,ignore
let panel = StackPanel::new()
  .spacing(8.0)
  .children([Button::new().content("Hi").build(tree)])
  .build(tree);
```

## Typed handles and `x:Name`

```rust,ignore
// artic-core, tree module
pub struct Handle<T> {
  id: NodeId,
  node_type: PhantomData<fn() -> T>,
}

impl<T: Object> Handle<T> {
  /// The untyped id.
  pub fn id(self) -> NodeId;
  /// The node. Panics if it was freed.
  pub fn get(self, tree: &UiTree) -> &T;
  /// The node, mutably. Panics if it was freed.
  pub fn get_mut(self, tree: &mut UiTree) -> &mut T;
  /// The node, or `None` if it was freed.
  pub fn try_get(self, tree: &UiTree) -> Option<&T>;
}

impl<T> From<Handle<T>> for NodeId { /* … */ }
```

- `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash` and `Debug` are written by hand: derived, they would require `T` to implement them too, and `Handle<Border>` wouldn't be `Copy`.
- `Handle<T>` is `Copy` and the size of `NodeId`. Like `NodeId` it is `Send` and `Sync`, but every method needs the tree, which isn't.
- Using a handle to a freed node is a bug and panics, as disposed signals do (#11); `try_get` is for code that expects nodes to go away.
- `x:Name` gives the code-behind a `Handle<T>` field. Spec §9's `Named<T>` is this type.
- Child parameters take `impl Into<NodeId>`, so a `Handle<Button>` or a raw `NodeId` both work.

## Member methods

A method is named after its field, which is already snake_case: the name UI languages use, `CornerRadius`, comes from the field `corner_radius`, whose method is `.corner_radius(…)`. Event methods, with routed events (#75), add the prefix `on_`.

| Member | Generated method | What it does |
| --- | --- | --- |
| `Property<T>` | `fn padding(self, value: impl Into<Thickness>) -> Self` | `self.padding.set(value.into())`, the `Local` level |
| `Property<f32>` and the other number types | `fn spacing(self, value: f32) -> Self` | `self.spacing.set(value)`. A number is taken as is: a literal such as `8.0` is an `f64` until something fixes its type, and `impl Into<f32>` doesn't |
| `Property<Option<V>>` | `fn background(self, value: impl Into<Brush>) -> Self` | `set(Some(value.into()))`. It takes `V`, so a family conversion (`SolidColorBrush` → `Brush`) still applies; `None` is the default |
| `Child` field | `fn child(self, child: impl Into<NodeId>) -> WithChildren<Self, Option<NodeId>>` | Wraps the node; `WithChildren::build` links the child in both trees ([children](#children)) |
| `Children` field | `fn children<I: IntoIterator<Item: Into<NodeId>>>(self, items: I) -> WithChildren<Self, I>` | Wraps the node: the ids wait in `WithChildren`, not in the node, and `WithChildren::build` links them in order in both trees ([children](#children)) |
| `Event<A>` | None yet | `Event<A>` has nowhere to store a handler. Routed events (#75) add the storage and the `on_<event>` method together, with the handler's parameters |
| Fields of a `#[ui(flatten)]` group | `.margin(…)`, `.width(…)`, `.classes(…)` on every element | Provided methods of the group's trait ([property groups](#property-groups)) |

Each property's key is also exposed, for styles. A key has one public name, on the type that owns it:

| Field | Constant |
| --- | --- |
| A property with its own key | `Border::PADDING`, on the type |
| A property with a shared key (`#[ui(key = TextElement::FONT_SIZE)]`) | None. The owner's constant, `TextElement::FONT_SIZE`, is the name |
| A field of a property group | `Layout::MARGIN`, on the group, from `derive(UiProperties)` |

```rust,ignore
impl Border {
  pub const PADDING: &'static PropertyKey<Thickness> = &__KEY_PADDING;
}
```

A constant may point at a `static` since Rust 1.83, so the generated key stays private in the derive's block and gets a public name here. The constant's name is the field name in SCREAMING_SNAKE_CASE. A second name for a shared key, such as `TextBlock::FONT_SIZE`, would read as a key for text blocks only, while a setter on it changes every type that shares the key.

Value types (`#[ui(value)]`, [value objects](value-objects.md)) get the same `new` and property methods, with no `build`: a value is passed straight to the property that holds it. They come with the loader's value types (#57), their first user.

### Property groups

A derive sees only the struct it's on: `derive(UiType)` on `Border` sees the field `layout: Layout`, not `Layout`'s fields. The group's methods travel through traits instead, so neither derive has to guess a name the other one chose:

```rust,ignore
// artic-registry
/// A node that holds the property group `G` in a `#[ui(flatten)]` field.
pub trait HasGroup<G> {
  /// The group.
  fn group(&self) -> &G;
}

// generated by derive(UiProperties) on Layout, next to it
pub trait LayoutMethods: HasGroup<Layout> + Sized {
  /// Sets `Margin`, the `Local` level.
  fn margin(self, value: impl Into<Thickness>) -> Self {
    self.group().margin.set(value.into());
    self
  }
  // one method per field of Layout
}

impl<T: HasGroup<Layout>> LayoutMethods for T {}

// generated by derive(UiType) for the field `#[ui(flatten)] layout: Layout`
impl HasGroup<Layout> for Border {
  fn group(&self) -> &Layout {
    &self.layout
  }
}
```

- `derive(UiType)` writes `HasGroup<T>` with the field's type as written, so it needs no trait name, and a type alias or a renamed import works.
- `derive(UiProperties)` names the trait after the group, with the suffix `Methods`; it's exported wherever the group is.
- The group stays the only list of its fields, so a group defined by an app works the same as `Layout`, and so does `Panel`'s `background`.
- A type that flattens the same group twice gets Rust's conflicting-implementations error, since one group per type is what the registry describes.
- Callers bring the traits into scope. The facade's `artic::prelude` exports `Handle` and the built-in group traits (`LayoutMethods`, `AppearanceMethods`, `InteractionMethods`, `IdentityMethods`, `PanelMethods`).
- `derive(UiProperties)` also writes `PropertyGroup::METHODS`, the trait's path, built from `module_path!()` as `TypeInfo::rust_path` is. Generated code imports a group's trait from it, an app's groups included, the same way it names a type from `rust_path`.

### Conversions

Property methods take `impl Into<T>`, numbers aside. With #43, a value type converts from the one value that already means the same thing in a UI language's text:

| Conversion | Same as |
| --- | --- |
| `Thickness: From<f32>` | `Padding="8"`, all four sides |
| `CornerRadius: From<f32>` | `CornerRadius="4"`, all four corners. It is `CornerRadius::uniform`, which, like every constructor, doesn't check the sign; only text is checked |
| `Classes: From<[&str; N]>` | `Classes="primary large"`; each item is split at whitespace, as the text is |
| `Brush: From<Color>`, `From<SolidColorBrush>` and the gradients | already built |

A tuple carries no names, so `From<(f32, f32)>` would leave the reader to guess which number is horizontal; it isn't added, and callers write `Thickness::symmetric(12.0, 6.0)`.

### Attached properties

An attached property is stored on its target node, so it can only be set once that node exists. #31 generates two associated functions on the owner, called after `build`:

```rust,ignore
impl Grid {
  pub const ROW: &'static AttachedKey<usize>;
  pub fn set_row(tree: &mut UiTree, node: impl Into<NodeId>, row: usize);
  pub fn row(tree: &UiTree, node: impl Into<NodeId>) -> usize;
}

let ok = Button::new().content("OK").build(tree);
Grid::set_row(tree, ok, 1);
```

There is no second setter in builder form; the builder doesn't wrap these helpers. `ROW` is the key styles use, and with it `UiTree::attached_property` gives the node's `Property<usize>` to set at another precedence (#31).

### Children

`WithChildren` lives in `artic-registry`, the crate the derive's generated code reaches:

```rust,ignore
/// A node waiting to be built, with the children to link once it is.
pub struct WithChildren<T, I> {
  node: T,
  children: I,
}

impl<T: UiType + Object, I: IntoIterator<Item: Into<NodeId>>> WithChildren<T, I> {
  /// Allocates the node, links the children in order through its content field, and returns it.
  pub fn build(self, tree: &mut UiTree) -> Handle<T>;
}
```

- `.child` and `.children` are generated only for the `#[ui(content)]` field; the derive rejects a `Child` or `Children` field that isn't the content, so a node has one way to take children.
- `build` links each child through `T::TYPE_INFO.content`, whose `ContentInfo::Collection` entry calls `UiCollection::push_child`. `Child` and `Children` keep their own rules that way, such as refusing a second child.
- The checks run first and panic on a bad child, as the [panels design](panels.md) says. After them no link can fail, so `build` treats an error from `push_child` as a bug.

### Naming collisions

| Case | Result |
| --- | --- |
| A field named `new` or `build` | Derive error: the name is the builder's start or finish; rename the field |
| A Rust keyword field (`r#type`) | The method is `r#type` too (a raw identifier) |
| A user's inherent method with a builder method's name | Rust's duplicate-definition error; builder method names are reserved on derived types |
| An attached getter clashing with the owner's own property method | Derive error naming both; `#[ui(name = …)]` renames one |
| A property constant and an attached constant with one name, such as two `ROW`s | Derive error naming both. A field `row` beside an attached `Row` reports this once, as the getter clash above |
| A field named like a field of a flattened group, such as an own `width` | Compile error naming both, from a `const` check against the group's `PROPERTIES`; otherwise the inherent method would hide the group's |
| Two flattened groups with a field of the same name | The same `const` check, naming the two groups |
| A field named `type_info` | Derive error: its constant would hide `UiType::TYPE_INFO` |

## Bindings, resources, classes and styles

| Feature | v1 builder form |
| --- | --- |
| `{StaticResource Key}` | A plain value: the compiler generates one function per resource ([value objects](value-objects.md#resources)) and passes its result, as in `.background(resource_accent())` |
| `{DynamicResource Key}` | Designed with #113 |
| `{Binding …}`, `Command="{Binding …}"` | **Compiler-only.** A binding needs `x:DataType` and the view-model, which a builder doesn't have. The compiler emits the typed binding calls from spec §6 after `build`, such as `bind_two_way(tree, email, \|vm: &LoginVm\| &vm.email, \|text_box: &TextBox\| &text_box.text)`. A builder-level binding is a v2 question |
| `Classes="primary"` | `.classes(["primary"])`, from `IdentityMethods` (#108) |
| `Style` (D6) | `Style::new(selector).setter(key, value)`, below |

```rust,ignore
// artic-style
impl Style {
  /// A style for the nodes `selector` matches; the selector syntax is #107's.
  pub fn new(selector: &str) -> Style;
  /// Adds a setter that writes `value` at precedence `StyleSetter` on matched nodes.
  pub fn setter<T: PartialEq + Clone + 'static>(self, key: &'static PropertyKey<T>, value: impl Into<T>) -> Style;
}

let primary = Style::new("Button.primary")
  .setter(Button::PADDING, Thickness::symmetric(12.0, 6.0))
  .setter(Button::BACKGROUND, Some(resource_accent().into()));
```

The key carries the value type, so `.setter(Button::PADDING, "red")` doesn't compile. A shared key (`FONT_SIZE`) styles every type that uses it. `Style`, `Setter` and the keys contain nothing XAML-specific, as D28 requires for the v2 CSS front end. Applying setters to matched nodes is #109.

## `Border`'s generated items

For `Border` as built:

| Kind | Items |
| --- | --- |
| Start and finish | `Border::new() -> Border`, `fn build(self, tree: &mut UiTree) -> Handle<Border>` |
| Content | `.child(item)`, returning `WithChildren` ([panels](panels.md)) |
| Properties | `.padding(impl Into<Thickness>)`, `.background(impl Into<Brush>)`, `.corner_radius(impl Into<CornerRadius>)` |
| Events | none; `on_…` methods come with #75 |
| Keys | `Border::PADDING`, `Border::BACKGROUND`, `Border::CORNER_RADIUS` |
| Registry | `Border::TYPE_INFO` (#18) |
| Through the groups | `.margin(…)`, `.width(…)`, `.height(…)`, `.classes(…)` and the other group members, from `LayoutMethods`, `IdentityMethods` and the other group traits |

## The spec's §4 example view

```xml
<UserControl xmlns="https://artic.rs/ui"
             xmlns:x="https://artic.rs/markup"
             x:Class="app::views::LoginView"
             x:DataType="app::vm::LoginVm">
  <UserControl.Resources>
    <SolidColorBrush x:Key="Accent" Color="#3B82F6"/>
  </UserControl.Resources>
  <StackPanel Spacing="8" Margin="16">
    <TextBox x:Name="email" Text="{Binding Email, Mode=TwoWay}" Placeholder="Email"/>
    <Button Classes="primary" Content="Sign in"
            Command="{Binding SignIn}" Click="on_click"/>
  </StackPanel>
</UserControl>
```

What the compiler (#49) emits for it:

```rust,ignore
// The resource: one function, called at each use (value objects design).
fn resource_accent() -> SolidColorBrush {
  SolidColorBrush::new().color(Color::rgb(0x3B, 0x82, 0xF6))   // "#3B82F6", parsed at build time
}

impl LoginView {
  fn build_view(&mut self, tree: &mut UiTree) -> Handle<UserControl> {
    let email = TextBox::new()
      .watermark("Email")
      .build(tree);
    self.email = email;                                            // x:Name="email"
    bind_two_way(tree, email, |vm: &LoginVm| &vm.email, |text_box: &TextBox| &text_box.text);

    let sign_in = Button::new()
      .classes(["primary"])
      .content("Sign in")
      .on_click(/* LoginView::on_click, wired as spec §9 describes */)
      .build(tree);
    bind_command(tree, sign_in, |vm: &LoginVm| &vm.sign_in);

    let panel = StackPanel::new()
      .spacing(8.0)
      .margin(Thickness::uniform(16.0))                           // "16", parsed at build time
      .children([email.id(), sign_in.id()])
      .build(tree);

    UserControl::new()
      .content(panel)
      .build(tree)
  }
}
```

- Text literals are parsed when the view is compiled, so the output constructs values (`Thickness::uniform(16.0)`) and never parses at runtime.
- Elements are emitted in post-order: each child is built before the element that holds it.
- `Accent` isn't used in the example. A use such as `Background="{StaticResource Accent}"` would emit `.background(resource_accent())`.
- How a code-behind method becomes a click handler belongs to the code-behind design (§9); the comment marks that spot.
- `TextBox` and `Button` aren't built yet, `UserControl` has no content field yet, and `SolidColorBrush::new().color(…)` is a value-type builder, which comes with #57. The example shows the shape the compiler emits once they exist.

## Changes to other issues and to the spec

| Where | Change |
| --- | --- |
| #43 Builder generation | `Handle<T>`, `new`, `build`, the property methods, the key constants and the collision errors; `WithChildren` with `.child` and `.children`; `HasGroup`, the group traits, `PropertyGroup::METHODS` and `Layout::MARGIN`-style constants; `artic::prelude`; the conversions above |
| #57 Loader value type | Builders for value types (`new` and property methods, no `build`) |
| #75 Routed events | Handler storage in `Event<A>` and the `on_<event>` methods |
| #42 `#[element]` | None: the group methods come from the group traits (#43) |
| #31 Attached properties | The `set_<name>` / `<name>` helper pair on the owner, called after `build` |
| #49 Compiler emission | Post-order `let` statements, build-time literals, resource functions, binding calls after `build`; imports of the group traits from `PropertyGroup::METHODS` |
| Spec §5 | Step 3's sample output becomes the builder shape |
| Spec §9 | `Named<T>` becomes `Handle<T>` |
