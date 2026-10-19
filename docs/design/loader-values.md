# Design: typed values in the runtime loader

Issue: #20. Implemented by: #57. Related designs: #32 (value objects), #31 (attached properties and value providers), #21 (builder API).

The runtime loader (spec §5, hot reload and `runtime-xaml`) reads XAML text and builds the same tree the compiled path builds. The compiled path knows every type while it generates code. The loader knows only strings and registry entries, yet it must end up calling a typed `Property<T>::set_at`. This document decides where the type `T` is known, without `Box<dyn Any>` and without `TypeId`-keyed downcasts in any public API (D9).

## Constraints

- **D9:** the spec's original `Value::Parsed(TypeId, Box<dyn Any>)` is rejected. No value travels through the loader with its type erased.
- **D1:** no string-path reflection for bindings. This design doesn't touch bindings; `{Binding}` follows the markup extension contract below and is designed in #96.
- **§3 rule 2:** text converts through the value type's `FromStr` (or `TryFrom`), never a custom converter API.
- **Inputs:** the loader walks the infoset from spec §4 (`XamlNode`, `Member`, `MemberKey`, `Span`). #23 implements those names; this design is written against the spec.
- **Outputs:** writes go to #13's `Property<T>::set_at(Precedence, T)` and `clear`. No new level type.

## Value sources

| Source | Example | What the loader ends up holding |
| --- | --- | --- |
| Attribute text | `Padding="4,8"` | Text |
| Property element with text | `<Border.Padding>4,8</Border.Padding>` | Text |
| XAML primitive | `<x:Double x:Key="Gap">8</x:Double>` | Text (`"8"`) |
| Property element object | `<Border.Background><SolidColorBrush Color="#3B82F6"/></Border.Background>` | An object of a UI type |
| Child element | `<Border><TextBlock/></Border>` | A child node |
| Collection items | `<StackPanel><A/><B/></StackPanel>` | Child nodes, in order |
| `x:Null` | `Background="{x:Null}"` | Null |
| `{StaticResource}` | `Background="{StaticResource Accent}"` | The resource's deferred markup, then one of the above |
| Style `Setter` | `<Setter Property="Padding" Value="4"/>` | Text or an object, at precedence `StyleSetter` |
| Other markup extensions | `{x:Static …}`, `{Binding …}` | One of the four inputs (see the contract below) |
| Attached property | `Grid.Row="1"` on a `Button` | Text, usually |
| Template content | `<ControlTemplate>…</ControlTemplate>` | Deferred markup, materialized per templated control |

Everything reduces to four inputs: **text**, **a child node**, **an object**, or **null**.

## Options

| | Option A: erased values | Option B: typed thunks (chosen) |
| --- | --- | --- |
| Where `T` is known | In the value: `Box<dyn Any>` tagged with a `TypeId`, downcast by the setter | In generated functions: each property gets small functions whose signature is type-free and whose body is typed |
| How text becomes `T` | The loader would need a per-type parse function returning an erased value | The thunk calls `T::from_str` and then `set_at` directly |
| How a `{StaticResource}` reaches `Property<T>` | Store erased values in the dictionary, downcast at use | Store the resource's markup, convert it at use through the target property's thunks |
| Type mismatch | Found by a failed downcast, far from the markup | Found inside the target's thunk, reported at the use site's span |
| Fits D9 | No | Yes |

Also rejected:

- **A closed enum of value kinds** (`Value::Brush`, `Value::Thickness`, …). It can't represent an app's own types, which §3 rule 5 requires ("adding a type from any crate needs only the derive").
- **A type-level loader token** to stop compiled code from calling the thunks. Rust can't restrict construction to one other crate, and `artic-registry` sits below the loader, so the token would have to be public. The test below proves the property instead.
- **Building each resource once and cloning it per use.** It needs an extra clone thunk and `T: Clone` for every resource type, and gains nothing observable over deferred markup for immutable values.

## Chosen design

### Thunks on the existing descriptors

`#[derive(UiType)]` (#18) already analyzes every property field and knows its type. It also emits one `static PropertyAccess` per property. The loader calls these function pointers; it never names a property's type.

```rust,ignore
// artic-registry

pub struct PropertyInfo {
  pub name: &'static str,
  pub field: &'static str,
  pub value_type: &'static str,
  pub affects: Affects,
  pub inherits: bool,
  pub access: &'static PropertyAccess,   // new
}

/// How the loader writes one property. Signatures are type-free; bodies are typed.
pub struct PropertyAccess {
  /// Parses `text` with the value type's `FromStr` and sets it at `level`.
  pub set_text: fn(&mut UiTree, NodeId, &str, Precedence) -> Result<(), ValueError>,
  /// Sets a node, for properties that name one (`Property<NodeId>` / `Property<Option<NodeId>>`, such as `DataContext`).
  pub set_node: Option<fn(&mut UiTree, NodeId, NodeId, Precedence) -> Result<(), ValueError>>,
  /// Converts a built object into the value type with `FromUiObject` and sets it. Present for
  /// every property; types that can't come from an object return `ValueError::WrongType`.
  pub set_object: fn(&mut UiTree, NodeId, NodeId, Precedence) -> Result<(), ValueError>,
  /// Sets `None`, only for `Property<Option<T>>`. `None` here means "not nullable".
  pub set_null: Option<fn(&mut UiTree, NodeId, Precedence) -> Result<(), ValueError>>,
  /// Empties `level`, when a style stops matching or a reload removes the attribute.
  pub clear: fn(&mut UiTree, NodeId, Precedence),
}

pub struct AttachedPropertyInfo {
  // existing fields …
  pub access: &'static PropertyAccess,   // new; where attached values are stored is #31 / #59
}

pub struct TypeInfo {
  // existing fields …
  /// What receives content: a property, or a collection field whose `add_child` appends a child
  /// node ([panels design](panels.md)).
  pub content: Option<ContentInfo>,   // replaces `content: Option<&str>` and a separate `add_child`
}

/// Why a thunk refused a value. Positions are added by the loader.
pub enum ValueError {
  /// The text didn't parse. `message` is the value type's `FromStr` error, displayed.
  Parse { expected: &'static str, message: String },
  /// `x:Null` on a property whose type isn't `Option<T>`.
  NotNullable { property: &'static str },
  /// An object or resource of the wrong type.
  WrongType { expected: &'static str, found: &'static str },
  /// The target node isn't the property's owner type.
  WrongNode,
  /// The child can't be linked to its parent.
  InvalidChild(LinkError),
  /// A second child for an element that takes one.
  OneChildOnly,
}
```

Function pointers are `Copy`, `const`-friendly and fit in the `static` descriptors from #17. `Option<fn(...)>` is the size of `fn(...)` because a function pointer can't be null.

What the derive would generate for `Border.padding`, shown as plain Rust:

```rust,ignore
fn padding_set_text(tree: &mut UiTree, node: NodeId, text: &str, level: Precedence) -> Result<(), ValueError> {
  let value: Thickness = text.parse().map_err(|error: <Thickness as FromStr>::Err| ValueError::Parse {
    expected: "Thickness",
    message: error.to_string(),
  })?;
  let border = tree.get_as::<Border>(node).ok_or(ValueError::WrongNode)?;
  border.padding.set_at(level, value);
  Ok(())
}
```

`set_object`'s body converts the built object with the value type's `FromUiObject`, which checks "is this a `T`?" with `T` known statically, the same check `UiTree::get_as::<T>` already makes. It happens inside typed code; the loader and the public API never see `Any`. How value objects such as `SolidColorBrush` are staged and converted is the [value objects design](value-objects.md).

### Precedence

`Precedence` is a plain parameter. The loader passes `Local` for attributes and child elements, `StyleSetter` for style setters, and calls `clear(StyleSetter)` when a selector stops matching (spec §8). `Inherited` is never passed: the tree sets it (#15).

### Spanned errors

Thunks know types but not positions. The loader knows the span of the attribute, element or `{StaticResource …}` it is processing, and wraps every `ValueError` into a #29 `Diagnostic` at that span. For resources, the diagnostic adds a "defined here" note at the resource's `x:Key`.

### Resources and templates: deferred markup

A resource dictionary maps each key to the resource's **infoset node and span, unconverted**. A use such as `{StaticResource Accent}` resolves the key (logical parents, then application, then theme; spec §8) and feeds the entry's node through the **target property's** thunks as if it had been written inline. The type check is the existing `set_text` / `set_object`, so a mismatch is reported where the resource is used.

Each use gets its own value; there is no shared instance. For immutable values (brushes, thicknesses, numbers) that is indistinguishable from sharing. The compiled path must behave the same way (emit the value per use, or clone it), so both back ends agree.

Template content is deferred markup too: a `ControlTemplate` keeps its infoset and is materialized once per templated control (#84), each instance going through the same rules.

`{DynamicResource}` (#113) re-runs the same conversion when the key's signal changes.

### Markup extension contract

Every markup extension, a value provider in [the UI language contract](../spec/dsl.md#8-value-providers), resolves to one of the four inputs (text, child node, object, null), and the loader then uses the thunks above. `{StaticResource}` and `{x:Null}` are specified here; `{x:Static}`, other extensions and attached property storage are #31; `{Binding}` is #96.

## Worked examples

**Attribute text.** `<Border Padding="4,8"/>`

1. The loader resolves `Border` with `Registry::type_by_name` (or `type_by_rust_path` for a `using:` prefix) and creates the node with `TypeInfo::new`.
2. It finds `PropertyInfo` `Padding` in `border.properties`.
3. It calls `(padding.access.set_text)(tree, node, "4,8", Precedence::Local)`.
4. The thunk parses a `Thickness` and calls `border.padding.set_at(Local, value)`.

**A `StaticResource` brush.** `<Border Background="{StaticResource Accent}"/>`, with `<SolidColorBrush x:Key="Accent" Color="#3B82F6"/>` in an ancestor's resources.

1. The loader resolves `Accent` to the entry's node: an `Object` of type `SolidColorBrush`.
2. It builds that object as it would inline (`TypeInfo::new`, then its members through their own thunks).
3. It calls `(background.access.set_object)(tree, border, brush, Precedence::Local)`.
4. The thunk converts the object with `Brush::from_object` ([value objects](value-objects.md)) and sets it.

**A style `Setter`.** `<Style Selector="Border.card"><Setter Property="Padding" Value="12"/></Style>`

1. When the selector matches a `Border`, the style system finds `PropertyInfo` `Padding` on the target type.
2. It calls `(padding.access.set_text)(tree, node, "12", Precedence::StyleSetter)`.
3. A local `Padding` on the same `Border` still wins (#13); when the selector stops matching, it calls `(padding.access.clear)(tree, node, Precedence::StyleSetter)`.

**A type mismatch.** `<Border Background="{StaticResource Gap}"/>`, where `Gap` is an `x:Double`.

1. `Gap`'s node is text (`"8"`), so the loader calls `background.access.set_text(…, "8", …)`.
2. `Option<Brush>`'s value type fails to parse `"8"` and returns `ValueError::Parse { expected: "Brush", message }`.
3. The loader reports `view.xaml:3:21: expected Brush, got '8'` at the `{StaticResource Gap}` span, with a note at `Gap`'s definition.

## Proving the compiled path never uses the thunks

There is no loader value type to forbid. The guarantee is that code generated by the XAML compiler calls typed setters directly and never the thunks. #57 adds the test:

**`compiled_output_has_no_loader_values`.** Compile a fixture `.xaml` that uses every value source in the table above (text, object, child, collection, `{StaticResource}`, `Setter`, `x:Null`). Parse the generated Rust with `syn`, visit every path and identifier, and fail if any of `PropertyAccess`, `access`, `set_text`, `set_node`, `set_object`, `set_null`, `add_child` or `ValueError` appears. Parsing with `syn` rather than searching text keeps a comment or string that mentions those words from triggering it.

## Open questions

- **Binary size.** Every property's thunks are compiled into every binary, release builds included, because `UI_TYPES` keeps the descriptors alive. #57 should measure it. If it matters, the derive can gate `PropertyAccess` behind a feature reached through the `__private` hub.
- **Value objects.** Settled by the [value objects design](value-objects.md): values are staged through `TypeInfo::new` and converted with `FromUiObject`, which made `set_object` always present.
- **Attached property storage.** `AttachedPropertyInfo.access` fixes how the loader writes an attached property, not where the value lives on the target node. That is #31 and #59.
