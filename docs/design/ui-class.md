# Design: code types with `#[ui_class]`

Issue: #52. Builds on: the [XAML compiler](xaml-compiler.md) (#49), the [builder API](builder-api.md) (#43), [UI languages beside XAML](dsl-neutral.md) (#305), `UserControl` (#48). Unblocks: #55 hello world, #82 event handlers, #180 the language server's type index.

A view's code type is the Rust type that holds its handlers and its named nodes ([dsl.md §9](../spec/dsl.md#9-language-features)); XAML names it with `x:Class`. `#[ui_class]` turns a struct into that type: the struct becomes the element at the root of its view, and holds the view's named nodes in one field.

## What an app writes

```rust,ignore
// src/views/login.rs
#[ui_class("login.xaml")]
pub struct LoginView {
  attempts: u32,
}

impl LoginView {
  fn on_click(&mut self, cx: &mut ViewCx, _: &Click) {
    self.attempts += 1;
    cx.get(self.ui.email).text.set(String::new());
  }
}
```

```xml
<!-- src/views/login.xaml -->
<UserControl xmlns="https://artic.rs/ui" xmlns:x="https://artic.rs/markup"
             x:Class="app::views::LoginView" Padding="16">
  <StackPanel Spacing="8">
    <TextBox x:Name="email" x:FieldModifier="pub(crate)"/>
    <Button Content="Sign in" Click="on_click"/>
  </StackPanel>
</UserControl>
```

```rust,ignore
let login: Handle<LoginView> = LoginView::create(&mut tree);
```

`ViewCx`, `Click` and the `Click="on_click"` wiring come with #82; the sample shows where they go.

## The code type is the element

In XAML, `x:Class` on the root says the root *is* that class. Rust has no inheritance, so the code type takes the root's place instead of wrapping it: `LoginView` is an element with `UserControl`'s members, and the view's tree has one node where the spec's first sketch had two.

```text
LoginView { layout, appearance, interaction, identity, child, ui, attempts }
  └─ StackPanel
       ├─ TextBox "email"
       └─ Button
```

- `#[ui_class]` adds `#[element]` and a `#[ui(content)] pub child: Child` field, so the struct has exactly `UserControl`'s members: the four groups and one child. The root's attributes (`Padding="16"`) set those members on `LoginView`.
- A handler (#82) gets `&mut LoginView` from the tree, typed, with no downcast and no second node.
- The root element of a view with a code type must be a `UserControl` for now. Another root, such as a `Surface` for a window, is a later issue: the code type would take that element's members instead.

The layout `UserControl` writes by hand today, "the children share my box and fill it", becomes `ElementImpl`'s default, as in Avalonia:

```rust,ignore
pub trait ElementImpl {
  /// The content's size within `available`. By default, the largest size the children want, as
  /// children sharing one box.
  fn measure_override(&self, cx: &mut MeasureCx<'_>, available: Size) -> Size {
    cx.measure_children(available)
  }

  /// Places the children within `size`. By default, each child fills it.
  fn arrange_override(&self, cx: &mut ArrangeCx<'_>, size: Size) {
    cx.arrange_children(Rect::new(0.0, 0.0, size.width, size.height));
  }
  // …
}
```

`UserControl` and `Surface` then have empty impls, and `#[ui_class]` writes `impl ElementImpl for LoginView {}`. A leaf with no children still measures as nothing, and `Border`, `StackPanel` and `TextBlock` override both methods already. `Application`, whose children are surfaces the host lays out on their own, is checked to make sure the default does nothing harmful there.

## The named nodes: one `ui` field

The compiled file already gathers every named node in a `View` struct of `Handle<T>`s. The code type holds that struct as one field, `ui`, instead of copying each handle onto itself:

```rust,ignore
pub struct LoginView {
  // added by #[ui_class]
  pub ui: login_view::View,
  // …
}
```

- `self.ui.email` is a `Handle<TextBox>`. `View` is `Copy`, so it can be passed around or destructured: `let View { email, .. } = self.ui;`.
- A named node can't collide with a field of the struct, so there is no collision error to report.
- The proc macro reads no file to learn the names: rustc resolves every field and type from the included code.
- For a view with a code type, `View` has no `root` field, since the code type is the root.

The compiled file is included in a module named after the struct in snake case (`login_view`), with the struct's visibility. The struct's `ui` field has the struct's visibility too, and each field of `View` has its node's field visibility (`x:FieldModifier`):

| Field visibility | `View` field | Reachable from |
| --- | --- | --- |
| none (the default) | `pub(super)` | the code type's module |
| `pub(crate)` | `pub(crate)` | the crate |
| `pub` | `pub` | anywhere the code type is |

`pub(super)` in the included module is the code type's own module, which is what "private" means for a field of the code type. The compiler writes these visibilities. A view without a code type keeps every field `pub`, as now.

## Building it

Every element is built from `Default`, and a code type is too: each field of the struct starts at its `Default` (`attempts` at 0). Data a view needs arrives after it's built, through the handle or, later, the view-model (#82, #94).

`ui` has no default: its handles exist only once the view's nodes are built. The `UiType` derive gains a field setting for that, usable by any type: `#[ui(required)]`. The derive then writes `new(ui: View) -> Self`, with every other field as `Default` would write it, instead of `Default` and `new()`. A type with a required field names the function the registry builds it with, `#[ui(new = path)]`, since `TypeInfo::new` takes only the tree.

`#[ui_class]` writes both:

```rust,ignore
impl LoginView {
  /// Builds the view in `tree`: every node of `login.xaml`, with this type at the root.
  pub fn create(tree: &mut UiTree) -> Handle<LoginView> {
    login_view::build(tree)
  }
}
```

`create`, not `build`, since `build(self, tree)` is already the builder's last call. The registry's `TypeInfo::new` for `LoginView` calls `create`, so the runtime loader (#59) and a view in another crate that uses `LoginView` build the whole view, not an empty node.

The compiled `build` constructs the code type as the root:

```rust,ignore
// $OUT_DIR/views/src/views/login.rs
pub fn build(tree: &mut UiTree) -> Handle<crate::views::LoginView> {
  let __e0 = ::artic::controls::TextBox::new().build(tree);
  let __e1 = ::artic::controls::Button::new().content("Sign in").build(tree);
  let __e2 = ::artic::controls::StackPanel::new().spacing(8.0_f32).children([__e0.id(), __e1.id()]).build(tree);
  crate::views::LoginView::new(View { email: __e0 })
    .padding(::artic::layout::Thickness::uniform(16.0_f32))
    .child(__e2)
    .build(tree)
}
```

## How the compiled file names the code type

The code type's path is the root's Rust type. The compiler reads `x:Class="app::views::LoginView"` as a path and replaces its first segment, the crate, with `crate`: generated code is included in that crate. A path whose first segment isn't the crate being built (`CARGO_CRATE_NAME`) is an error: a code type lives in the crate its view is compiled into.

The root's members resolve through the element the document's root names, `UserControl`, which the code type mirrors. The code type itself isn't in the build script's registry, since it's defined in the app crate ([which types a view can use](xaml-compiler.md#which-types-a-view-can-use)).

Two checks keep the struct and the file together:

- rustc checks the path: a code type that doesn't exist, or isn't a type with that `View`, fails to compile.
- The compiled file states its code type, `pub const CODE_TYPE: Option<&str> = Some("app::views::LoginView");`, or `None` without one. `#[ui_class]` adds a constant assertion comparing it with `concat!(module_path!(), "::LoginView")`, whose message names the source file: "`src/views/login.xaml` names `app::views::Login` as its code type, not `app::views::LoginView`", or "`src/views/login.xaml` has no code type: add `x:Class="app::views::LoginView"` to its root".

## The file argument

`#[ui_class("login.xaml")]` names the view relative to the file the struct is in, as `include_str!` does. The macro finds that file with `Span::local_file` and makes the path relative to the crate (`src/views/login.xaml`).

`compile_dir` writes each view at its path in the crate: `$OUT_DIR/views/src/views/login.rs`. That is the same path its header and diagnostics already show, so the macro needs nothing else to find the compiled file. A missing compiled file is the macro's error, at the string: "no compiled view for `src/views/login.xaml`: the file doesn't exist, or the build script's `compile_dir` doesn't cover it".

## Where each piece lives

`#[ui_class]` doesn't know XAML. It includes a compiled file, adds the `ui` field and the element, and checks `CODE_TYPE`, so any UI language whose compiler writes the same `View` and `build` shape works with it.

| Piece | Crate |
| --- | --- |
| `#[ui_class]`, `#[ui(required)]`, `#[ui(new = …)]` | `artic-macros`, re-exported as `artic::ui_class` |
| `ElementImpl`'s fill default | `artic-element` |
| Code type, field visibility, `CODE_TYPE`, the root as the code type, the output path | the UI-language-neutral view compiler |
| Reading `x:Class` and `x:FieldModifier` into the code type and field visibility | `artic-xaml-compiler` |

Today the view model, Rust emission, member resolution, `compile_dir` and the diagnostics all sit in the XAML crates. An issue of their own moves them into neutral crates before #52's compiler changes, so the code type and field visibility land in the neutral model the first time, and only their XAML spellings land in the XAML crate.

## What is reported

| Problem | Reported by | Where |
| --- | --- | --- |
| `x:Class` on an element other than the root | compiler | the attribute |
| A code type whose root isn't a `UserControl` | compiler | the root element |
| A code type outside the crate being built | compiler | the attribute |
| A field visibility other than `pub` or `pub(crate)` | compiler | the attribute |
| A field visibility on an element without a name | compiler | the attribute |
| The file and the struct name different code types, or the file names none | `#[ui_class]`'s assertion | the struct, naming the `.xaml` |
| No compiled file | `#[ui_class]` | the path string |
| A struct with generics, a tuple struct, or a field named `ui` or `child` | `#[ui_class]` | the struct or the field |

## Not in this design

- Event handlers, the handler table and `view_model::<T>()`: #82.
- Building a code type from a file at run time: #59, through `TypeInfo::new`.
- A code type as an element in another compiled view: a compiled view uses only build-dependency types (D1 of the compiler), so a library's code type in an app's view is the case, and the compiler would emit `create` for it. A later issue.
- `App` opening its first window from a code type: #55.

## Testing

- `artic-element`: an element with no `ElementImpl` methods fills its box with its children; `UserControl` and `Surface` layouts are unchanged.
- `artic-macros` (trybuild): a code type builds; a field named `ui`; generics; a missing compiled file, with `.stderr` naming the `.xaml`; `#[ui(required)]` without `#[ui(new = …)]`.
- The view compiler: snapshots of a view with a code type and field visibilities; each new diagnostic.
- The fixture app (`crates/artic-xaml-compiler/fixtures/app`) gains a code type:
  - `build_returns_code_type_root`: `create` returns the code type at the root of the tree, with the view's root content as its child;
  - two named nodes resolve to the right nodes;
  - a mismatched or missing `x:Class` fails (trybuild, against the fixture's views);
  - `field_visibility_sets_access`: a `pub(crate)` handle is reachable from another module, and a default one isn't (trybuild).

## Changes to other documents and issues

| Where | Change |
| --- | --- |
| Spec §9 | the sample uses `self.ui.email`; the code type is the root element, not a `UserControl` owning `Self`; `create(&mut UiTree) -> Handle<Self>` |
| Spec §4 | `x:Name` adds a field to the view's `ui`; `x:FieldModifier` sets that field's visibility |
| `dsl.md` §9 | code type and field visibility point here |
| `xaml-compiler.md` | "recorded for #52" and the wrapper question point here; `View` has no `root` for a view with a code type; output paths are relative to the crate |
| `builder-api.md` | the `build_view` sample points here |
| `element.md` | `ElementImpl`'s defaults fill the box |
| #52 | "an `x:Name` colliding with a user field" leaves the criteria; `build_returns_user_control_root` becomes `build_returns_code_type_root` |

## Pull requests

| PR | Contents |
| --- | --- |
| 1 | this document and the changes above |
| 2 | `ElementImpl`'s fill default; `UserControl` and `Surface` lose their impls |
| 3 | the view compiler: code type and field visibility, the root as the code type, `CODE_TYPE`, output paths relative to the crate, the diagnostics above; XAML reads `x:Class` and `x:FieldModifier` into them |
| 4 | `#[ui(required)]`, `#[ui(new = …)]`, `#[ui_class]`, the facade's `artic::ui_class`, the fixture app's code type and the trybuild cases; closes #52 |

PR 3 waits for the issue that moves the neutral pieces out of the XAML crates.
