# Design: value objects written as XAML elements

Issue: #32. Builds on: [loader values](loader-values.md) (#20). Related: #21 (builder API), #35 (`Color` and `Brush`), #43 (builder generation), #57 (loader values implementation).

Some XAML elements aren't UI nodes. `<SolidColorBrush Color="#3B82F6"/>` is a value, like a number: it ends up inside a property such as `Border.Background`, or in a resource dictionary, and never in the tree. The registry from #17 can only build nodes (`TypeInfo::new: fn(&mut UiTree) -> NodeId`). This document decides how both XAML paths build values, without `Box<dyn Any>` (D9) and without a second registry.

## Inventory

A type is a **node** if it lives in the tree: it has a parent, takes part in layout, input or rendering, or needs a `Named<T>` handle. It is a **value** if it is only data that a property holds, copied into each place that uses it.

| Type | Spec | Kind | Why |
| --- | --- | --- | --- |
| `SolidColorBrush`, `LinearGradientBrush`, `GradientStop` | §7, #35 | Value | Paint data held by `Background`, `Foreground` and similar properties |
| `RowDefinition`, `ColumnDefinition` | §7 | Value | Sizing data in a `Grid`'s lists; not drawn or hit-tested |
| `KeyBinding` | §9 | Value | Gesture-to-command data in an element's input bindings |
| `Setter` | §8 | Value | A property and a value inside a `Style`; it is applied, never placed |
| `Style` | §8 | Value | A selector and setters held by resources and style lists |
| `DoubleTransition` and other transitions | §8 | Value | Animation parameters held by `Transitions` |
| Converters declared as resources | §6 | Value | Stateless helpers referenced by bindings |
| `Run`, `Bold`, `Italic` | §7 | Node | Inline text has a parent `TextBlock`, takes part in text layout and selection, and can carry `x:Name` |
| `ControlTemplate`, `DataTemplate` | §8 | Neither | Deferred markup ([loader values](loader-values.md#resources-and-templates-deferred-markup)): kept as infoset and instantiated per use |

Values are immutable Rust data (`Clone`, usually `PartialEq`). Unlike WPF's `Freezable` objects, they are never shared or mutated in place, so they need no freezing rules.

## Node and value types in the registry

`TypeInfo` gains a kind. Both kinds keep the one constructor signature:

```rust,ignore
pub enum TypeKind {
  /// The object stays in the tree.
  Node,
  /// The object is staged in the tree while its members are set, then moved into a property.
  Value,
}

pub struct TypeInfo {
  // existing fields …
  pub new: fn(&mut UiTree) -> NodeId,   // unchanged
  pub kind: TypeKind,                      // new
}
```

A single `fn` pointer type can't return a `Brush` for one entry and a `RowDefinition` for another, so a value constructor that returns the value is impossible without erasing its type. Instead, `new` **stages** a value: it allocates the value struct in the tree with its defaults, the loader sets its members through the usual thunks, and the target property then moves it out (below). The tree serves as a typed construction site; only what happens after construction depends on the kind.

A value type uses the same derive with a `value` flag. Its fields are plain, not `Property`:

```rust,ignore
#[derive(Debug, Clone, PartialEq, UiType)]
#[ui(value)]
pub struct SolidColorBrush {
  #[ui] pub color: Color,
  #[ui(default = 1.0)] pub opacity: f32,
}
```

| `#[ui(value)]` changes | Why |
| --- | --- |
| `#[ui]` fields are plain `T`; their `PropertyAccess` thunks assign the field and ignore `Precedence` | A value is immutable data, with no signals and no precedence stack; reactivity lives in the property that holds it |
| `TypeInfo.kind` is `TypeKind::Value` | Both back ends can tell nodes from values |
| `Event<A>` fields are a compile error | Values aren't in the tree, so they raise no events |
| A content collection of values is allowed | `LinearGradientBrush` holds its `GradientStop`s; each child is a staged value moved in by `add_child` |
| The type must be `Clone` | Each use gets its own copy |

## From a staged object to the property's type

A property's value type may be a family rather than the element's type: #35 plans `Brush` as an enum with a variant per brush, while `<SolidColorBrush>` maps 1:1 to the struct `SolidColorBrush` (§3 rule 1). The **target type** decides what it accepts:

```rust,ignore
// artic-registry
/// A property value type that can be taken from a staged object.
pub trait FromUiObject: Sized {
  /// Takes the staged object `object` out of the tree as `Self`.
  fn from_object(tree: &mut UiTree, object: NodeId) -> Result<Self, ValueError>;
}

/// Every UI type takes itself: free the staging node and check it is a `T`.
impl<T: UiType + 'static> FromUiObject for T { /* … */ }

// In #35's crate: a family accepts its members, written in terms of its `From` impls.
impl FromUiObject for Brush {
  fn from_object(tree: &mut UiTree, object: NodeId) -> Result<Brush, ValueError> {
    // a SolidColorBrush becomes Brush::from(solid), a LinearGradientBrush Brush::from(linear),
    // anything else ValueError::WrongType
  }
}
```

The blanket impl and a family's impl don't conflict: only the crate that owns `Brush` could make it a `UiType`, so Rust's coherence rules accept both (checked with a two-crate prototype).

The [loader values](loader-values.md) thunk `set_object` for `Property<V>` or `Property<Option<V>>` is then `V::from_object(tree, object)?` followed by `set_at(level, value)`. The derive can't know at macro time whether `V` implements `FromUiObject`, so `set_object` is generated for every property: with the same autoref selection #18 uses for its value check, it calls `from_object` when available, and otherwise returns `ValueError::WrongType { expected, found: "an object element" }`. `PropertyAccess.set_object` is therefore always present, not an `Option` (the loader values design is updated accordingly).

## Loader steps

For `<Border.Background><SolidColorBrush Color="#3B82F6"/></Border.Background>`:

1. Resolve `SolidColorBrush` in the registry. Its `kind` is `Value`.
2. Stage it: `(info.new)(tree)` allocates a `SolidColorBrush` with its defaults.
3. Set its members through their thunks: `Color`'s `set_text("#3B82F6")` parses a `Color` and assigns the field.
4. Call `Background`'s `set_object(tree, border, staged, Precedence::Local)`.
5. The thunk calls `Brush::from_object`, which frees the staging node, takes the `SolidColorBrush` out and converts it with `Brush::from`, then calls `background.set_at(Local, Some(brush))`.

The staging node is always freed by `from_object`, on success and on error, so no value is left in the tree.

## Compiled output

The compiled path knows every type at build time, so it stages nothing and never calls `FromUiObject`. It builds the value with the builder methods (#21, generated by #43 for value types too) and converts it with `Into`:

```rust,ignore
border.background.set(Some(
  SolidColorBrush::new()
    .color(COLOR_3B82F6)   // validated at build time; how the constant is emitted is #49's design
    .into(),               // SolidColorBrush → Brush through `From`
));
```

Because a family's `FromUiObject` is written in terms of its `From` impls, the two paths can't convert differently.

## Resources

For `<SolidColorBrush x:Key="Accent" Color="#3B82F6"/>` used as `Background="{StaticResource Accent}"`:

| Path | Storage | At each use |
| --- | --- | --- |
| Loader | The resource's infoset node, unconverted ([deferred markup](loader-values.md#resources-and-templates-deferred-markup)) | Run the loader steps above on the stored node |
| Compiled | No runtime dictionary for static resources: one generated function per resource | `border.background.set(Some(resource_accent().into()))` |

```rust,ignore
// compiled output for the resource itself
fn resource_accent() -> SolidColorBrush {
  SolidColorBrush::new().color(COLOR_3B82F6)
}
```

Both paths produce one value per use, and a missing key stays a compile error in the compiled path (§8). `{DynamicResource}` needs a runtime dictionary built on signals; that is #113.

A node written as a resource (`<Button x:Key="…"/>`) is built anew at each use, because deferred markup instantiates per use. WPF would share one instance and fail when it gets a second parent; artic doesn't.

## `x:Name` on values

`x:Name` on a value is an error in both paths: a `Named<T>` handle (§9) points at a node, and a value has none once it is moved into its property. Both report ``x:Name names an element; `SolidColorBrush` is a value`` at the attribute's span. `x:Key` remains the way to refer to a value.

## Walkthrough: the §4 brush resource

```xml
<UserControl.Resources>
  <SolidColorBrush x:Key="Accent" Color="#3B82F6"/>
</UserControl.Resources>
…
<Border Background="{StaticResource Accent}"/>
```

**Compiled path.** The compiler emits `resource_accent()` as above and, for the `Border`, `border.background.set(Some(resource_accent().into()))`. Type mismatches are found while compiling the generated code.

**Loader path.**

1. Loading `UserControl.Resources` stores `Accent`'s infoset node and span in the control's dictionary.
2. Loading the `Border` meets `{StaticResource Accent}` on `Background` and resolves the key by walking up the logical tree.
3. It runs the loader steps on the stored node: stage a `SolidColorBrush`, set `Color`, then call `Background`'s `set_object`.
4. `Brush::from_object` converts the staged brush, and `background.set_at(Local, Some(brush))` stores it.
5. Had `Accent` been a `Thickness`, step 4 would return `WrongType { expected: "Brush", found: "Thickness" }`, reported at the `{StaticResource Accent}` span with a note at the `x:Key`.

## Changes to other issues

| Issue | Change |
| --- | --- |
| #35 `Color` and `Brush` | `Brush` implements `From` for each brush struct and `FromUiObject` written in terms of them |
| #43 Builder generation | `#[ui(value)]` in the derive: plain fields, `TypeKind::Value`, no events, `Clone` required, builder methods for value types |
| #57 Loader values | Stage values with `TypeInfo::new`; `set_object` always present and calling `FromUiObject`; free the staging node in every case |
