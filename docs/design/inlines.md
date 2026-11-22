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

`Run`, `Bold` and `Italic` flatten `TextFormat`, so each takes `FontFamily`, `FontSize`, `FontWeight`, `FontStyle` and `Foreground`, as `TextBlock` does. `TextBlock`, `Bold` and `Italic`, whose content is inlines, are `#[ui(whitespace = "significant")]`, so whitespace between two inlines counts as one space (section 7).

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

An inline is a tree node, as the [value objects](value-objects.md) design decided, linked under its parent in the logical tree only: it takes no space and draws nothing, so layout and rendering never visit it. `Inlines::push_child`, the path both the builder and a UI language take, links the child logically and marks it drawn by its parent (`UiTree::set_drawn_by_parent`), so a change to a run, a property write or a link, marks the `TextBlock` that draws it.

`Inlines` takes only `Run`, `Bold` and `Italic`, and says so through `UiCollection::takes`, a list of `TypeInfo`s that the registry carries as `ContentInfo::Collection::takes`. A collection that lists nothing takes any child, as `Children` does. The list is checked in three places, so a wrong child never panics halfway through a build:

| Where | What happens to `<TextBlock><Border/></TextBlock>` |
| --- | --- |
| the view checker | "`TextBlock` takes only `Run`, `Bold` or `Italic`", at the `Border`, at compile time |
| `WithChildren::build` | a panic before the block is allocated, as for a child listed twice |
| the generated `add_child`, which the loader calls | `ValueError::UnexpectedChild`, nothing linked |

Each `TypeInfo` gains `is`, which tells whether a node's object is of that type (`is_a::<T>` for a derived one), so the run-time checks need no list of concrete types. `takes` is a function rather than a constant: `Bold`'s descriptor holds the list, and the list holds `Bold`'s descriptor.

A content field is also checked against the fields of the groups its type flattens, as an own property is: its method, named after it, would hide the group's. The check compares names without `r#`, since `r#width` and `width` get the same method.

With no run under it, such as a lone empty `<Bold/>`, a block shows one empty line in its own font, as empty `Text` does, rather than collapsing.

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

## 7. Text inside a block

Text written inside a `TextBlock`, `Bold` or `Italic` becomes a `Run`, as in WPF. `Inlines` names `Run` as its `UiCollection::text_wrapper`; the view checker turns each piece of text in a collection with a wrapper into an element of that type with its content property set to the text, so `Hello <Bold>world</Bold>` gives a `Run`, then a `Bold` holding another `Run`. The checker knows no inline type: a collection with no wrapper reports text as before.

Whitespace follows WPF:

| Written | Text |
| --- | --- |
| `<TextBlock>  Hello   world  </TextBlock>` | `Hello world`: runs collapse to one space, and the content is trimmed at the block's tags |
| `<TextBlock>Hello <Bold>world</Bold></TextBlock>` | `Hello world`: text beside an element keeps the space on that side |
| `<TextBlock><Run>Hello</Run> <Run>world</Run></TextBlock>` | `Hello world`: whitespace-only text between two inlines counts as one space, because the types are `Whitespace::Significant` |
| the same split over lines, with indentation | `Hello world` |
| `<TextBlock xml:space="preserve">` with the same content | as written, line breaks and indentation included |

A content property, such as `<Run>  two  spaces </Run>`, follows the same rules and reads `two spaces`; before, a significant type read its text as written whatever `xml:space` said.

## 8. What waits

| Feature | Tracked in |
| --- | --- |
| The loader wrapping text and keeping the space between inlines | #59 |
| Hyperlinks and other inline types | not in v1 |
| Right-to-left runs | #170 |

## 9. Testing

| Crate | Tests |
| --- | --- |
| `artic-core` | `type_value_wins_over_the_link_and_loses_to_a_style`, `link_places_in_one_tree_and_refuses_a_cycle` |
| `artic-macros` | `other_collections_take_children_by_their_field_name`, `content_method_clashing_with_a_reserved_name_is_an_error`, `raw_content_method_clashing_with_a_reserved_name_is_an_error`, `code_type_root_with_another_collection_is_an_error`, `other_collection_content_takes_children`, and the trybuild cases `content_field_named_like_a_group_field` and `content_raw_field_named_like_a_group_field` |
| `artic-registry` | `unexpected_child_names_what_the_content_takes`, `type_names_read_as_a_list`, `check_child_follows_the_list`, `group_field_check_ignores_raw_spelling` |
| `artic-controls` | `inline_runs_flatten_with_styles`, `the_innermost_weight_wins`, `bold_own_weight_wins_and_remeasures`, `inline_whitespace_preserved`, `inlines_win_over_text_while_present`, `text_shows_again_once_the_last_inline_leaves`, `inlines_without_runs_keep_one_line`, `run_change_invalidates_measure`, `each_run_paints_with_its_foreground`, `textblock_inlines_accesskit_value`, `a_non_inline_child_is_refused`, `building_with_a_non_inline_child_leaves_the_tree_as_it_was`, `a_stale_child_is_refused_by_the_link` |
| `artic-view` | `text_becomes_wrapper_elements`, `blank_text_between_elements_adds_nothing_when_it_collapses`, `empty_text_between_elements_adds_nothing`, `a_wrapper_without_a_content_property_reports_the_text`, `a_wrapper_the_collection_cannot_use_reports_the_text`, `content_text_reads_the_ruled_text` |
| `artic-xaml-lang` | `own_collection_takes_children_by_its_field_name`, `child_of_a_type_the_content_does_not_take_reports_error`, `text_in_a_text_block_becomes_runs`, `preserved_text_in_a_text_block_keeps_its_whitespace`, `significant_content_collapses_unless_preserved`, `text_in_a_collection_without_a_wrapper_reports_error`; the corpus snapshots of text in a `TextBlock` now compile to runs |
| `artic-vello` | golden `text_block_inlines` |
