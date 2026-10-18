//! Text shaping and fonts for artic.
//!
//! [`Engine`] implements [`TextEngine`](artic_layout::text::TextEngine) with `parley`, which shapes
//! text and breaks it into lines, and `fontique`, which finds fonts. Layout asks it for paragraphs;
//! renderers draw them without depending on this crate. See the
//! [text design](https://github.com/wolfware-labs/artic/blob/main/docs/design/text.md).
//!
//! The `complex-scripts` feature, on by default, breaks lines between words in scripts written
//! without spaces, such as Thai, Lao, Khmer and Myanmar, with ICU's dictionaries. It adds about
//! 3.6 MB to a release binary; without it, those scripts break only at spaces and hard line breaks.

mod engine;
mod faces;
mod lines;
mod paragraph;
mod properties;
mod shaped;
mod shaper;
mod trim;

pub use engine::Engine;
