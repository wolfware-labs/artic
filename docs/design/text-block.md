# Design: TextBlock

Issue: #46. Builds on: [text shaping and fonts](text.md) (#37), [elements](element.md) (#42), [brushes](brush.md) (#35), [golden images](golden-images.md) (#38), [layout tests](layout-tests.md) (#56), [border](border.md) (#45). Unblocks: #47, #54, #65, #170, #49, #175.

`TextBlock` shows a string: shaped, wrapped to its width, cut with "…" when it runs out of room, aligned, and painted with a brush. Its font and brush come from the nearest ancestor that sets them when it doesn't set them itself, so a panel can set the font of everything inside it.

```xml
<StackPanel TextElement.FontSize="16" TextElement.Foreground="#202020">
  <TextBlock Text="Settings" FontWeight="Bold" FontSize="20"/>
  <TextBlock Text="{Binding Description}" TextWrapping="Wrap" TextTrimming="WordEllipsis"
             MaxLines="3" LineHeight="22" TextAlignment="Justify"/>
  <TextBlock Text="C:\Users\ada\projects\artic\docs\design\text-block.md"
             TextTrimming="PathSegmentEllipsis"/>
</StackPanel>
```

The issue predates the [text design](text.md), which put shaping behind `TextEngine` and gave layout a place to keep a control's paragraph. That replaces several things in the issue:

- the entry point is `cx.text()`, not `Renderer::measure_text`;
- there's no control-owned `parley` cache: `LayoutManager` skips a measure whose `available` hasn't changed, and `set_measured` and `set_arranged` keep the paragraph;
- text goldens are shared, not per platform, since the bundled test font draws the same everywhere.

Section 8 lists the acceptance criteria that change.

## 1. Inherited text properties

The font and the brush are attached properties of `TextElement`, as in WPF and Avalonia, so any node can hold them and every `TextBlock` below follows the nearest one:

```rust
/// The font and brush text is drawn with, as attached properties any node can hold. Each
/// inherits, so text follows the nearest ancestor that sets them.
#[derive(Debug, XamlType)]
#[xaml(namespace = "https://artic.rs/ui")]
#[xaml(attached(name = "FontFamily", ty = FontFamily, affects = layout, target = any, inherits))]
#[xaml(attached(name = "FontSize", ty = f32, default = 14.0, affects = layout, target = any, inherits))]
#[xaml(attached(name = "FontWeight", ty = FontWeight, affects = layout, target = any, inherits))]
#[xaml(attached(name = "FontStyle", ty = FontStyle, affects = layout, target = any, inherits))]
#[xaml(attached(name = "Foreground", ty = Brush, default = Brush::from(Color::rgb(0, 0, 0)), affects = render, target = any, inherits))]
pub struct TextElement;
```

`TextElement` lives in `artic-controls`, beside `TextBlock`. The defaults are `system-ui` (text.md §2), 14 px as in Avalonia, normal weight and style, and black. Themes (#116) replace the defaults with setters on the root.

A node that doesn't hold a key costs nothing, and inheritance skips it, so a `Border` between a panel and its text neither carries a `Foreground` nor breaks the chain. That's why these aren't a property group flattened into every element: every `Border` and panel would hold five properties it never reads. `affects` belongs to the key, so a panel that holds `TextElement.FontSize` is itself marked for measure when it changes; that costs one measure of the panel, which its text's new size causes anyway.

`TextBlock` also lets you write `FontSize="20"` on it directly, by declaring fields that share the attached keys, as WPF's `AddOwner` does. Today `#[xaml(key = STATIC)]` takes a `PropertyKey` static only. It's extended to also take an attached key's constant, such as `TextElement::FONT_SIZE`:

- the field is stored under the attached property's `PropertyKey`, so a value set as `FontSize` on a `TextBlock` and one set as `TextElement.FontSize` on a panel are the same property, and the `TextBlock`'s descendants inherit either;
- the field's default is the attached key's, so the two can't disagree.

The derive can't tell the two kinds of key apart by their path, so the generated code goes through a trait, `SharedKey<T>`, that `PropertyKey<T>` and `AttachedKey<T>` implement: it gives the `PropertyKey` to store under and, for an attached key, its default. `default = ...` beside a shared key requires a second trait that only `PropertyKey` implements, so writing it beside an attached key is a type error.

## 2. `TextBlock`

```rust
#[element]
#[derive(Debug)]
#[xaml(namespace = "https://artic.rs/ui")]
pub struct TextBlock {
  #[xaml(affects = layout)] pub text: Property<String>,
  #[xaml(affects = layout)] pub text_wrapping: Property<TextWrapping>,
  #[xaml(affects = layout)] pub text_trimming: Property<TextTrimming>,
  #[xaml(affects = layout)] pub text_alignment: Property<TextAlignment>,
  #[xaml(affects = layout)] pub max_lines: Property<Option<NonZeroU32>>,
  #[xaml(affects = layout)] pub line_height: Property<Option<f32>>,
  #[xaml(key = TextElement::FONT_FAMILY)] pub font_family: Property<FontFamily>,
  #[xaml(key = TextElement::FONT_SIZE)] pub font_size: Property<f32>,
  #[xaml(key = TextElement::FONT_WEIGHT)] pub font_weight: Property<FontWeight>,
  #[xaml(key = TextElement::FONT_STYLE)] pub font_style: Property<FontStyle>,
  #[xaml(key = TextElement::FOREGROUND)] pub foreground: Property<Brush>,
}
```

| Property | Values | Default |
| --- | --- | --- |
| `Text` | any string | empty |
| `TextWrapping` | `NoWrap`, `Wrap`, `WrapWithOverflow` (section 3) | `NoWrap` |
| `TextTrimming` | `None`, `CharacterEllipsis`, `WordEllipsis`, `PrefixCharacterEllipsis`, `LeadingCharacterEllipsis`, `PathSegmentEllipsis` (section 4) | `None` |
| `TextAlignment` | `Start`, `Center`, `End`, `Left`, `Right`, `Justify` (section 5) | `Start` |
| `MaxLines` | a whole number above 0; `0` doesn't parse | unset: every line |
| `LineHeight` | a distance in logical pixels from one line's top to the next | unset: the font's own spacing |

`Text` isn't the content property: inline runs (#47) will be, so `<TextBlock>Hello</TextBlock>` waits for them. An unset `Option` property is "auto" here as `Width` is: XAML has no `"Auto"` spelling for them.

The properties in the table belong to the `TextBlock` alone. Avalonia makes `TextAlignment`, `LineHeight` and the others inherited attached properties too; here they stay local until a use case asks for a panel to set them. Moving one to `TextElement` later keeps `TextAlignment="Center"` on a `TextBlock` working, since the field would share the new key as the font fields do.

## 3. What the engine is asked

The engine takes three inputs, split by who decides them, as Avalonia splits run and paragraph properties:

```rust
pub trait TextEngine {
  fn paragraph(&mut self, text: &str, style: &TextStyle, paragraph: &ParagraphStyle, limits: TextLimits) -> Paragraph;
}

/// How the characters look: the font. A span of text (#47) will have one each.
pub struct TextStyle {
  pub family: FontFamily,
  pub size: f32,
  pub weight: FontWeight,
  pub style: FontStyle,
}

/// How the paragraph is laid out: one per paragraph, from the control's properties.
pub struct ParagraphStyle {
  /// The distance from one line's top to the next one's, or `None` for the font's own spacing.
  pub line_height: Option<f32>,
  /// Where each line sits in the paragraph's width.
  pub alignment: TextAlignment,
  /// Whether and how lines break to fit `max_width`.
  pub wrapping: TextWrapping,
  /// How a line that doesn't fit is cut.
  pub trimming: TextTrimming,
  /// The number of lines kept, or `None` to keep every line.
  pub max_lines: Option<NonZeroU32>,
}

/// How much room layout gives the paragraph.
pub struct TextLimits {
  /// The width lines break at and trimmed lines fit, or `None` for no limit.
  pub max_width: Option<f32>,
  /// The height the kept lines fit, or `None` for no limit. The first line is always kept.
  pub max_height: Option<f32>,
}
```

`TextWrapping`, `TextTrimming` and `TextAlignment` are `XamlEnum`s in `artic-layout`, beside `FontStyle`, so `TextBlock` and the engine share one type each. `ParagraphStyle::default()` matches `TextBlock`'s defaults. `TextLimits::NONE` has no limits and `TextLimits::width(width)` limits the width only. Only `TextLimits` changes between measure and arrange, which is what lets `TextBlock` decide when to shape again (section 6).

| `TextWrapping` | Lines break | `parley` |
| --- | --- | --- |
| `NoWrap` | only at hard line breaks | `TextWrapMode::NoWrap` |
| `Wrap` | between words, and inside a word wider than `max_width`, so nothing sticks out | `TextWrapMode::Wrap`, `OverflowWrap::Anywhere` |
| `WrapWithOverflow` | between words only, so a long word or URL sticks out | `TextWrapMode::Wrap`, `OverflowWrap::Normal` |

`max_width` used to turn wrapping on. Now it only limits, and `wrapping` says whether lines break, since a `NoWrap` line that is trimmed still needs a width to fit. Today's callers that wrap become `WrapWithOverflow`, which is what `parley` did for them, so their lines and the existing goldens don't change. A negative `max_width` is 0, so `Wrap` puts each cluster on its own line and `WrapWithOverflow` breaks at every opportunity.

The engine keeps lines in order until `max_lines` is reached or the next line's bottom would pass `max_height`, whichever comes first, and always keeps the first line, as WPF does. A `max_height` that is NaN or infinite is no limit, and a negative one keeps one line.

`Paragraph` gains three things from the engine:

- `right_to_left()`: the direction found from the first strong character, so `TextBlock` places the paragraph in its box by the rule the engine places lines with (section 5);
- `limited_width()`: whether `max_width` shaped the paragraph, because a line broke other than at a hard break, was trimmed, or was justified;
- `limited_height()`: whether `max_height` dropped lines.

`Paragraph::new` makes a left-to-right paragraph that no limit shaped, and `with_right_to_left` and `with_limited` set the rest, for engines outside artic.

## 4. Trimming

`parley` has no ellipsis, so `artic-text` trims, when `trimming` isn't `None`. A kept line is trimmed when either:

- it's wider than `max_width`; or
- it's the last kept line, and the text after it, where lines were dropped by `max_lines` or `max_height`, holds something other than whitespace.

One rule covers a `NoWrap` line that is too long, a `WrapWithOverflow` word that sticks out, and a paragraph cut by `MaxLines` or the height. A text that ends in a line break leaves only whitespace after its last line, so that line isn't trimmed.

The line's text runs from its start to the next hard line break, or the end of the text, so trimming never crosses a hard break. For a line cut because lines after it were dropped, that text can be longer than the line, and the mode picks which part of it to show. A line whose own text fits but which is cut because lines after it were dropped still gets "…", as CSS's `line-clamp` does, so the reader sees there is more.

The engine shapes each candidate on its own, as one line with wrapping off, with the paragraph's direction forced: it puts U+200E LEFT-TO-RIGHT MARK or U+200F RIGHT-TO-LEFT MARK first and drops that cluster from the result. `parley` finds a layout's direction from its text, so without the mark a later line of a right-to-left paragraph that starts with Latin letters would be laid out left to right, with its ellipsis on the wrong side. The trimmed line keeps the original line's `top`, `height` and `baseline`, so an ellipsis from a fallback font can't move the lines or break the `max_height` check. The ellipsis is U+2026, in the paragraph's style, with the same font fallback as the text.

Cuts fall between `parley` clusters, which keep a base character with its combining marks. A word cut falls where `parley` marks a cluster as a word boundary.

| `TextTrimming` | Shows | When even that doesn't fit |
| --- | --- | --- |
| `CharacterEllipsis` | the text's start, cut between clusters, then "…" | "…" alone |
| `WordEllipsis` | the text's start, cut at a word boundary, then "…" | `CharacterEllipsis` |
| `PrefixCharacterEllipsis` | the first 8 clusters, "…", then as much of the end as fits | `CharacterEllipsis` |
| `LeadingCharacterEllipsis` | "…", then as much of the end as fits, cut between clusters | "…" alone |
| `PathSegmentEllipsis` | the text with whole segments near its middle replaced by one "…", keeping the first and last segments; `/` and `\` separate segments on every system | `LeadingCharacterEllipsis` |

These match Avalonia's six values, including its prefix length of 8. "Start" and "end" are logical, in the paragraph's direction, so in right-to-left text the trailing ellipsis lands on the left. The cut for `CharacterEllipsis`, `WordEllipsis` and `LeadingCharacterEllipsis` is found by a binary search over cluster boundaries, which then steps back while the candidate doesn't fit, since kerning and ligatures make widths only nearly monotonic. That's a handful of one-line shapes per trimmed line, and only trimmed lines pay it. `PathSegmentEllipsis` drops segments outward from the middle one at a time. "…" alone can be wider than `max_width` and is then drawn overhanging, as a long word is under `WrapWithOverflow`.

A trimmed paragraph is an ordinary `Paragraph`: its size is what it draws, and its runs hold the ellipsis glyph. Accessibility gets the whole `Text` (section 6).

## 5. Alignment

The engine already places each line by its free width, the paragraph's width minus the line's, times a factor (text.md §4). `TextAlignment::factor(right_to_left) -> f32` gives it, and `TextBlock` uses the same function to place the paragraph in its box, so one rule places both:

| `TextAlignment` | Left-to-right | Right-to-left |
| --- | --- | --- |
| `Start`, and `Justify` | 0 | 1 |
| `End` | 1 | 0 |
| `Center` | ½ | ½ |
| `Left` | 0 | 0 |
| `Right` | 1 | 1 |

`Justify` places a line by `Start`, then stretches it to the paragraph's width by sharing its free width evenly among its space clusters (`parley`'s space or no-break space), as CSS's `text-justify: inter-word` does. The hanging whitespace at the line's end isn't counted. These lines aren't stretched:

- the last kept line, and a line that ends at a hard break;
- a trimmed line;
- a line with no spaces, such as CJK text or one long word;
- a line wider than the paragraph.

Spacing between characters in scripts without spaces waits for a use case: it needs a rule per script, and Arabic must never be spaced apart.

A justified line has to reach the box's edge, but a paragraph's width is its widest line, which can be a word short of `max_width`. So a paragraph with at least one stretched line is as wide as `max_width` or its widest line, whichever is wider, and its `limited_width()` is true, so a box of another width shapes it again (section 6).

## 6. Measure, arrange, render

`TextBlock` keeps, as its layout result, a private `Shaped { paragraph: Paragraph, limits: TextLimits }`: the paragraph and the limits it was shaped with.

| Step | Rule |
| --- | --- |
| Properties | everything but `Foreground` affects layout, since alignment and the font change the glyphs; `Foreground` affects render. An inherited change reaches the `TextBlock` through artic-core's inheritance and dirties it like a local one |
| Measure | `TextStyle` from the font properties, `ParagraphStyle` from the others, and `TextLimits` with `max_width` the available width and `max_height` the available height when `TextTrimming` isn't `None`. Without trimming, lines that don't fit the height are still drawn, and a parent that clips decides what shows, as in WPF. The desired size is the paragraph's size, and `cx.set_measured` keeps the `Shaped` |
| Arrange | reads back `cx.arranged::<Shaped>()` after a move, or `cx.measured::<Shaped>()`, which `ArrangeCx` gains. It shapes again, with the box's width and, when trimming, its height, when the box would give another paragraph. On each axis, that's when the box is smaller than the paragraph, or the paragraph's limit on that axis shaped it (`limited_width()`, `limited_height()`) and the box differs from that limit, either by more than one device pixel (`1.0 / cx.scale()`). Width or height lost to snapping alone never wraps or trims text, a word that sticks out never makes it shape again, and wrapped or justified text in a box wider than it was measured for wraps again to fill it. A new `Shaped` goes to `cx.set_arranged` |
| Render | `scene.text(paragraph, origin, foreground)` with the paragraph from `cx.layout_result::<Shaped>()`. `origin.x` is the box's width minus the paragraph's, times `text_alignment.factor(paragraph.right_to_left())`, and `origin.y` is 0. The box doesn't clip: text that overflows, as `NoWrap` without trimming does, draws past it, and a clipping parent such as `Border` cuts it |
| Accessibility | role `Label`, and the whole `Text` as the node's value (accesskit puts a label's text in `value`, not `label`), trimmed or not |

The issue's cache-key question is answered by layout: a measure runs only when the available size or a layout property changes, and a move keeps the arranged result. A `NoWrap` `TextBlock` without trimming shapes again in measure when only its available width changes, though the width can't change its lines; skipping that waits until profiling shows it matters.

## 7. Documents that change

| Document | Change |
| --- | --- |
| `docs/design/text.md` | §2: the three inputs of section 3, replacing `TextLimits`' old fields and `TextStyle`'s `line_height`, and the new `Paragraph` methods; §3: `TextBlock` returns the paragraph's size (it has no padding), `ArrangeCx::measured`, and section 6's arrange rule replacing "only when it got less than the paragraph's width"; §4: step 2 breaks by `wrapping` and keeps lines by `max_height` too, step 3's factor is `TextAlignment::factor` and justification stretches lines, a step for trimming, and the input table's rows for a negative `max_width` under each wrapping mode and for `max_height`; §9: the #46 row points here |
| Spec §7 | the `TextBlock` row lists `TextAlignment`, `MaxLines` and `LineHeight`, and says the font and `Foreground` are `TextElement`'s inherited attached properties (`TextElement.FontSize` and the others), as the `Surface` row names `Desktop`'s |
| Spec §12 | the `TextBlock` bullet gains `TextAlignment`, and says the font and `Foreground` inherit through `TextElement`'s attached properties |

These change with the code, in the pull request that changes it: `Paragraph::size`'s doc (a justified paragraph can be wider than its widest line), `TextEngine::paragraph`'s, and the `artic-macros` docs for `key =`.

## 8. Issue text that changes

With the owner's approval, issue #46's body changes to match: the entry point is `cx.text()` and layout results, not `Renderer::measure_text` and a control cache; goldens are shared, not per platform; `font_size_inherited_from_parent` sets `TextElement.FontSize` on the panel; `textblock_trims_with_ellipsis` checks the ellipsis glyph where the mode puts it, not only at the line's end.

## 9. What waits

| Feature | How it fits | Tracked in |
| --- | --- | --- |
| Inline runs, and text as the content | a `TextStyle` per span in `paragraph`, and `Inlines` as the content property | #47 |
| An explicit `FlowDirection` | the engine forces the direction with the same mark it puts in front of trimming candidates; `Start` and `End` follow it | #170 |
| Pushing the accesskit node to the OS | the tree walk reads `TextBlock`'s node | #54 |
| Theme defaults for the font and brush | setters on the root for `TextElement`'s properties | #116 |
| Inherited paragraph properties | the property moves to `TextElement`, and the `TextBlock` field shares its key | when a use case asks |
| Justifying between characters | a rule per script | a later issue |
| Selection and copy | not in v1 for `TextBlock` | a later issue |

## 10. Testing

The test font has U+2026 but no Hebrew letters and no direction marks, so right-to-left tests measure `.notdef` glyphs, as `artic-text`'s existing right-to-left test does; direction comes from the characters, not the glyphs, so placement is still checked. Trimming tests find the ellipsis by its glyph id and check it sits at the logical position the mode puts it: last for the trailing modes, first for `LeadingCharacterEllipsis`, after the kept start for `PrefixCharacterEllipsis` and `PathSegmentEllipsis`.

| Crate | Tests |
| --- | --- |
| `artic-layout` | `text_wrapping_parses_ignoring_case`, `text_trimming_parses_ignoring_case`, `text_alignment_parses_ignoring_case`; `alignment_factor_follows_direction` (every row of section 5's table); `arrange_reads_the_measured_result` |
| `artic-text` | wrapping: `no_wrap_keeps_one_line`, `wrap_breaks_a_long_word`, `wrap_with_overflow_keeps_a_long_word`, `max_lines_with_no_wrap`; limits: `max_height_drops_lines_that_do_not_fit`, `max_height_keeps_the_first_line`, `negative_max_height_keeps_one_line`, `unusable_max_height_is_no_limit`; reports: `paragraph_reports_its_direction`, `wrapped_paragraph_is_limited_by_width`, `unwrapped_paragraph_is_not_limited`, `dropped_lines_limit_the_height`; trimming: `character_ellipsis_cuts_between_clusters`, `word_ellipsis_cuts_at_a_word_boundary`, `word_ellipsis_falls_back_to_characters`, `prefix_ellipsis_keeps_eight_clusters_and_the_end`, `prefix_ellipsis_falls_back_to_characters`, `leading_ellipsis_keeps_the_end`, `path_ellipsis_drops_middle_segments`, `path_ellipsis_falls_back_to_leading`, `trimming_stops_at_a_hard_break`, `clamped_line_that_fits_still_gets_an_ellipsis`, `trailing_line_break_is_not_trimmed`, `ellipsis_alone_when_nothing_fits`, `no_trimming_draws_no_ellipsis`, `trimmed_line_keeps_its_metrics`, `trimmed_latin_line_in_rtl_paragraph` (the ellipsis on the left), `trimmed_right_to_left_text_stays_inside_its_size`, `combining_mark_stays_with_its_base` (a cut never splits `e` + U+0301); alignment: `center_and_end_place_lines`, `left_and_right_ignore_direction`, `justify_fills_every_line_but_the_last`, `justify_skips_hard_breaks_and_trimmed_lines`, `justify_skips_a_line_without_spaces`, `justify_ignores_hanging_whitespace`, `justified_paragraph_takes_max_width`, `overflowing_line_is_not_justified` |
| `artic-macros` | `key_takes_an_attached_key` (the field and the attached property are one value, and the field starts at the attached default), and a trybuild case for `default` beside an attached key, which fails to type-check |
| `artic-controls` | `textblock_nowrap_single_line`, `textblock_wraps_at_width`, `textblock_max_lines`, `textblock_trims_with_ellipsis`, `textblock_trims_to_the_available_height`, `empty_textblock_is_one_line_high`, `font_size_inherited_from_parent` (`TextElement.FontSize` on a `StackPanel`), `own_font_size_wins_over_inherited`, `attached_font_size_on_a_textblock_is_its_own` (XAML `TextElement.FontSize` on a `TextBlock` reads back through `font_size`), `inherited_change_remeasures`, `text_change_invalidates_measure`, `snapping_alone_does_not_shape_again`, `narrower_box_shapes_again`, `overflowing_word_does_not_shape_again`, `wider_box_wraps_again`, `alignment_places_the_paragraph` (`Center` in a box wider than the text), `textblock_accesskit_node` (role `Label`, value the whole text, empty for empty text, updated when `Text` changes), and the property test `textblock_invariants_hold` over random text, widths and modes, with containment off since overflow is allowed |
| `artic-vello` | goldens `text_block_wrapping` (the three modes in one narrow column), `text_block_trimming` (the five trimming modes), `text_block_line_height`, `text_block_alignment` (the six values, `Justify` over several lines), shared like the other text goldens |

## 11. Pull requests

| PR | Contents | Done when |
| --- | --- | --- |
| A | this document and the documents in section 7 | approved |
| B | sections 3 and 5: the three inputs and the enums, the new `Paragraph` methods, `TextAlignment::factor`, `ArrangeCx::measured`, and wrapping, `max_height`, alignment and justification in `artic-text`; every caller moves to the new `paragraph` signature (the `artic-layout` fakes, `artic-text`'s tests, `artic-test`'s scenes and engine helpers, `artic-vello`'s tests and the `primitives` example) with `WrapWithOverflow` where it wrapped | the `artic-layout` tests, and the wrapping, limits, reports and alignment tests in `artic-text`, pass; the existing goldens are unchanged |
| C | section 4: trimming in `artic-text` | the trimming tests in `artic-text` pass |
| D | sections 1, 2 and 6: `key =` with an attached key, `TextElement`, `TextBlock`, its tests and goldens; closes #46 | the `artic-macros`, `artic-controls` and `artic-vello` tests pass |
