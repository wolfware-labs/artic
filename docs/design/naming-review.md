# Design: review of WPF-specific API names

Issue: #30. Rule: D20 (#1). Applied by: each API's own issue, when it's built.

D20: "The markup keeps the XAML name and `.xaml` files. Docs and APIs present artic as a Rust
framework without Microsoft or WPF framing. WPF-specific names are replaced where a neutral one
exists (`AutomationProperties` becomes `Accessibility`)."

This review applies that rule to every name in the spec that a reader might only understand with WPF
experience. A name stays when it says what it does without that experience, when it's MS-XAML
syntax, or when no widely known neutral name exists. The spec uses the decided names; code picks
them up as each API is built.

## Renamed

| Current name | Where | What it does | New name | Reason | Decision |
| --- | --- | --- | --- | --- | --- |
| `AutomationProperties` | §12 Accessibility | accessibility metadata on elements | `Accessibility` | The model row, decided in D20 | Renamed (D20) |
| `InputScope` | §12 Text | which on-screen keyboard to show | `InputMode`: `Text`, `Numeric`, `Decimal`, `Tel`, `Email`, `Url`, `Search` | HTML `inputmode`; "scope" says nothing about keyboards | Renamed |
| `UseLayoutRounding` | §7 Layout | rounds arranged rectangles to device pixels | `SnapToPixels` (`snap_to_pixels: bool`) | Names the effect; "layout rounding" is WPF jargon | Renamed |
| `Watermark` | §7 `TextBox` | hint text shown in an empty box | `Placeholder` | HTML `placeholder` and WinUI `PlaceholderText`; a watermark means something else in imaging | Renamed |
| `Holding` | §9 Input | the touch long-press gesture | `LongPressed` | The term iOS and Android use, in the past tense like `Tapped` | Renamed |
| `ExtendClientAreaToDecorations` | §10 Windows | draws content under a custom title bar | `ExtendIntoTitleBar` | WinUI's `ExtendsContentIntoTitleBar`, shorter; "client area" is Win32 jargon | Renamed |
| `Visibility` beside `IsVisible` | §7 property groups | `Visible` / `Hidden` / `Collapsed` | `Visibility` stays; `IsVisible` removed | Two properties for one thing. First decided the other way (keep `IsVisible`, hide with `Opacity = 0`, as in Avalonia); reversed in the elements design (#42): the three states are also CSS's and Android's, so the name isn't WPF-specific, and `Opacity = 0` still takes clicks and is still read by screen readers | `IsVisible` removed |
| `subscribe_all(…)`, for WPF's `HandledEventsToo` | §9 Routed events | also receives events already handled | `subscribe_including_handled(…)` | `subscribe_all` reads as "every event" | Renamed |

## Kept

| Name | Where | Reason | Decision |
| --- | --- | --- | --- |
| `PART_*` template parts | §7 control table, §8 | Marks the names a control's code depends on, which a plain name wouldn't; WPF and Avalonia authors recognize it, and every default template uses it | Kept |
| `x:FieldModifier` | §4 | MS-XAML directive syntax (D20); the parser (#23) and its rejection table (#27) use it | Kept |
| `TemplatedParent` | §6 Bindings, §8 | Reasonably descriptive, and Avalonia's vocabulary; no well-known neutral name | Kept |
| `TemplateBinding` | §8 | Says what it does; the same in WPF and Avalonia | Kept |
| `Preview*` tunnel events | §9 | XAML handler attributes need one name per phase (`PreviewKeyDown="on_key"`); moving the phase to subscription would be a routed-events redesign, not a rename | Kept |
| `FlowDirection` | §12 Localization | Mirrors layout, not only text, so CSS `direction` would undersell it; WinUI and Avalonia use the name | Kept |
| `SizeToContent` | §7 `Window` | Says what it does | Kept |
| `PasswordChar` | §7 `TextBox` | Says what it does | Kept |
| `IsHitTestVisible` | §12 Input | Descriptive and Avalonia's name; CSS `pointer-events` is less clear off the web | Kept |
| `DataContext` | §6 | The MVVM term across WPF, Avalonia and WinUI; MAUI's `BindingContext` is less known | Kept |
| `OneWayToSource` | §6 Binding modes | One of the `OneWay` / `TwoWay` / `OneTime` family; renaming one breaks the pattern | Kept |
| `Dispatcher` | §11, `artic-core` (#16) | A general term (the dispatcher pattern), already in the merged API | Kept |

## Not reviewed

The XAML name, the `.xaml` extension and MS-XAML directive syntax stay by D20. Names that are
general UI vocabulary (`Margin`, `Padding`, `Button`, `Grid`, `StackPanel`) and artic's own names
(`Layout`, `Arena`, `ArticTheme`) aren't WPF-specific.
