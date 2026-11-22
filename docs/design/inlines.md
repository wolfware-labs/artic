# Design: inline runs

Issue: #47. Builds on: [TextBlock](text-block.md) (#46), [text shaping and fonts](text.md) (#37), [frames](frame.md), [panels](panels.md) (#43). Unblocks: #49, #59.

One `TextBlock` can mix styles:

```xml
<TextBlock FontSize="16" TextWrapping="Wrap"><Run>Plain, </Run><Bold>bold, </Bold><Italic>italic, </Italic><Run Foreground="Red">red </Run><Bold><Run Foreground="Blue">and bold blue.</Run></Bold></TextBlock>
```

## 1. The types

`artic-controls` adds four:

| Type | What it is | Content |
| --- | --- | --- |
| `Run` | a piece of text in its own font and brush | `Text`, a property |
| `Bold` | inlines in a bold weight | `Inlines` |
| `Italic` | inlines in italics | `Inlines` |
| `Inlines` | the collection a `TextBlock`, `Bold` and `Italic` hold their inlines in | |

`Run`, `Bold` and `Italic` flatten `TextFormat`, so each takes `FontFamily`, `FontSize`, `FontWeight`, `FontStyle` and `Foreground`, as `TextBlock` does. Each, and `TextBlock`, is `#[ui(whitespace = "significant")]`, so `Hello <Bold>world</Bold>` keeps its space once the languages read it (#49, #59).

```rust
# use artic_controls::{Bold, Run, TextBlock, TextFormatMethods};
# use artic_core::tree::UiTree;
let mut tree = UiTree::new();
let hello = Run::new().text("Hello ").build(&mut tree);
let world = Run::new().text("world").font_size(20.0).build(&mut tree);
let bold = Bold::new().inlines([world]).build(&mut tree);
let block = TextBlock::new().inlines([hello.id(), bold.id()]).build(&mut tree);
```

## 2. Inlines are nodes in the logical tree

An inline is a tree node, as the [value objects](value-objects.md) design decided, linked under its parent in the logical tree only: it takes no space and draws nothing, so layout and rendering never visit it. `Inlines::push_child`, the path both the builder and a UI language take, links the child logically and marks it drawn by its parent (`UiTree::set_drawn_by_parent`), so a change to a run, a property write or a link, marks the `TextBlock` that draws it. It refuses a node that isn't a `Run`, `Bold` or `Italic` with `ValueError::UnexpectedChild`.

`UiTree::link(tree, parent, index, child)` is the single-tree form of `link_both`: a link it refuses is an error rather than a panic, which `push_child` returns.

## 3. Styles combine by inheritance

Font properties and `Foreground` inherit down the logical tree, so a `Run` inside `Bold` inside `Italic` reads bold and italic with no merge rule of its own, and a value the run sets itself wins over both.

`Bold` must beat the weight its `TextBlock` passes down, which a default can't: `Default` sits below `Inherited`. A new precedence level, `TypeValue`, sits between `Inherited` and `StyleSetter`: a value a type gives its own instances over what they inherit. `Bold` sets `FontWeight` there and `Italic` sets `FontStyle`, in `override_defaults`. A style, a binding or a local value still wins:

| Written | Weight of the run inside |
| --- | --- |
| `<TextBlock FontWeight="Light"><Bold><Run/></Bold>` | bold: `TypeValue` beats the inherited light |
| `<Bold FontWeight="Black"><Run/></Bold>` | black: local beats `TypeValue` |
| `<Bold><Run FontWeight="Thin"/></Bold>` | thin: the run's own value |

## 4. Flattening

`Inlines::runs(tree, owner)` walks the logical tree under `owner` depth first (`UiTree::descendants`) and yields each `Run` in reading order, those inside `Bold` and `Italic` included. Each run is one push to a `StyledText` in its own `TextStyle`, so its index is both its style's index and its brush's:

| Reads the runs | For |
| --- | --- |
| `Inlines::styled_text` | the text `TextBlock` shapes in measure and arrange, and the accessibility value |
| `TextBlock::render` | one brush per run, `Foreground`, which only affects rendering, so it's read when drawing, not kept from the last measure |

## 5. `Text` and `Inlines`

While a `TextBlock` holds any inline, it shows its inlines' text, and `Text` is ignored; with none, it shows `Text` in its own font. Neither rewrites the other, unlike WPF, where setting `Text` replaces the inlines with one run: a binding to `Text` and inlines written in markup don't fight, and removing the last inline shows `Text` again.

## 6. Builder methods named after the field

The derive gave a builder method only to a field written `Child` or `Children`, chosen by the type's name. Every collection of child elements now gets one named after its field: `.child(node)` for a `Child` field, and `.children(..)`, `.surfaces(..)` or `.inlines(..)` taking any number. A `Vec` content holds values and gets none until #57. The view compiler calls the method by the content field's name, so it needs no list of collection types either. A `code_type_root` type's content collection must still be a `Child` or `Children` field, which the derive now checks: a view's code type takes the root's content as a field of one of those two types, and `ContentMethod::Children` no longer tells `Children` apart from another collection.

## 7. What waits

| Feature | Tracked in |
| --- | --- |
| Bare text inside a `TextBlock`, `Bold` or `Italic` becomes a `Run`, with its whitespace kept | #47 |
| The loader applying significant whitespace | #59 |
| Hyperlinks and other inline types | not in v1 |
| Right-to-left runs | #170 |

## 8. Testing

| Crate | Tests |
| --- | --- |
| `artic-core` | `type_value_wins_over_the_link_and_loses_to_a_style`, `link_places_in_one_tree_and_refuses_a_cycle` |
| `artic-macros` | `other_collections_take_children_by_their_field_name`, `content_method_clashing_with_a_reserved_name_is_an_error`, `raw_content_method_clashing_with_a_reserved_name_is_an_error`, `code_type_root_with_another_collection_is_an_error`, `other_collection_content_takes_children` |
| `artic-registry` | `unexpected_child_names_what_the_content_takes` |
| `artic-controls` | `inline_runs_flatten_with_styles`, `the_innermost_weight_wins`, `inline_whitespace_preserved`, `inlines_win_over_text_while_present`, `run_change_invalidates_measure`, `each_run_paints_with_its_foreground`, `textblock_inlines_accesskit_value`, `a_non_inline_child_is_refused` |
| `artic-xaml-lang` | `own_collection_takes_children_by_its_field_name` |
| `artic-vello` | golden `text_block_inlines` |
