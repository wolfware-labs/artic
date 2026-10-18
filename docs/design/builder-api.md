# Design: the builder API that generated code targets

Issue: #21. Builds on: `derive(UiType)` (#18), [value objects](value-objects.md) (#32), geometry (#22). Implemented by: #43 (builder generation in the derive), #49 (compiler emission), #31 (attached-property helpers), #42 (`#[element]` delegation).

The XAML compiler and, in v2, hand-written Rust views both build trees through one API: `Button::new().content("Hi").on_click(…)` (D8). This document fixes that API's shape, so every later codegen issue follows it.

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
- Every builder method takes `self` by value, writes one member, and returns `Self`.
- `build(self, tree: &mut UiTree) -> Handle<T>` allocates the struct, links its children (the content field and collection fields) in the logical and visual trees, which resolves inherited values (#15), and returns a typed handle.

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
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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

- `Handle<T>` is `Copy` and the size of `NodeId`. Like `NodeId` it is `Send` and `Sync`, but every method needs the tree, which isn't.
- Using a handle to a freed node is a bug and panics, as disposed signals do (#11); `try_get` is for code that expects nodes to go away.
- `x:Name` gives the code-behind a `Handle<T>` field. Spec §9's `Named<T>` is this type.
- Child parameters take `impl Into<NodeId>`, so a `Handle<Button>` or a raw `NodeId` both work.

## Member methods

A method is named after its field, which is already snake_case: the name UI languages use, `CornerRadius`, comes from the field `corner_radius`, whose method is `.corner_radius(…)`. Event methods add the prefix `on_`.

| Member | Generated method | What it does |
| --- | --- | --- |
| `Property<T>` | `fn padding(mut self, value: impl Into<Thickness>) -> Self` | `self.padding.set(value.into())`, the `Local` level |
| `Property<Option<V>>` | `fn background(mut self, value: impl Into<Brush>) -> Self` | `set(Some(value.into()))`. It takes `V`, so a family conversion (`SolidColorBrush` → `Brush`) still applies; `None` is the default |
| `Child` field | `fn child(self, child: impl Into<NodeId>) -> WithChildren<Self, Option<NodeId>>` | Wraps the node; `WithChildren::build` links the child in both trees ([panels design](panels.md)) |
| `Children` field | `fn children<I: IntoIterator<Item: Into<NodeId>>>(self, items: I) -> WithChildren<Self, I>` | Wraps the node: the ids wait in `WithChildren`, not in the node, and `WithChildren::build` links them in order in both trees ([panels design](panels.md)) |
| `Event<A>` | `fn on_pointer_pressed(mut self, handler: impl FnMut(&A) + 'static) -> Self` | Stores the handler in the field. The handler's exact parameters come from event routing (#75); the builder only forwards the closure |
| Fields of the property groups | `.margin(…)`, `.width(…)`, `.classes(…)` on every element | Delegating methods generated by `#[element]` (#42) |

Each property's key is also exposed, for styles:

```rust,ignore
impl Border {
  pub const PADDING: &'static PropertyKey<Thickness> = &__KEY_PADDING;   // or the shared `key = …`
}
```

A constant may point at a `static` since Rust 1.83, so the generated key stays private in the derive's block and gets a public name here. The constant's name is the field name in SCREAMING_SNAKE_CASE.

Value types (`#[ui(value)]`, [value objects](value-objects.md)) get the same `new` and property methods, with no `build`: a value is passed straight to the property that holds it.

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

### Naming collisions

| Case | Result |
| --- | --- |
| A field named `new` or `build` | Derive error: the name is the builder's start or finish; rename the field |
| A Rust keyword field (`r#type`) | The method is `r#type` too (a raw identifier) |
| A user's inherent method with a builder method's name | Rust's duplicate-definition error; builder method names are reserved on derived types |
| An attached getter clashing with the owner's own property method | Derive error naming both; `#[ui(name = …)]` renames one |

## Bindings, resources, classes and styles

| Feature | v1 builder form |
| --- | --- |
| `{StaticResource Key}` | A plain value: the compiler generates one function per resource ([value objects](value-objects.md#resources)) and passes its result, as in `.background(resource_accent())` |
| `{DynamicResource Key}` | Designed with #113 |
| `{Binding …}`, `Command="{Binding …}"` | **Compiler-only.** A binding needs `x:DataType` and the view-model, which a builder doesn't have. The compiler emits the typed binding calls from spec §6 after `build`, such as `bind_two_way(tree, email, \|vm: &LoginVm\| &vm.email, \|text_box: &TextBox\| &text_box.text)`. A builder-level binding is a v2 question |
| `Classes="primary"` | `.classes(["primary"])`, delegated from the `identity` group (#42, #108) |
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
| Events | one `.on_…` method per `Event` field; `Border` has none |
| Keys | `Border::PADDING`, `Border::BACKGROUND`, `Border::CORNER_RADIUS` |
| Registry | `Border::TYPE_INFO` (#18) |
| Through the groups (#42) | `.margin(…)`, `.width(…)`, `.height(…)`, `.classes(…)` and the other group members |

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

## Changes to other issues and to the spec

| Where | Change |
| --- | --- |
| #43 Builder generation | Generate `new`, `build`, the member methods, the `KEY` constants and the collision errors above, for node types and (without `build`) value types |
| #42 `#[element]` | Delegating builder methods for the property groups' fields (`margin`, `width`, `classes`, …) |
| #31 Attached properties | The `set_<name>` / `<name>` helper pair on the owner, called after `build` |
| #49 Compiler emission | Post-order `let` statements, build-time literals, resource functions, binding calls after `build` |
| Spec §5 | Step 3's sample output becomes the builder shape |
| Spec §9 | `Named<T>` becomes `Handle<T>` |
