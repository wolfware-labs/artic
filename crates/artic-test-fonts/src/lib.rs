//! The font artic's tests shape and draw text with: Atkinson Hyperlegible, by the Braille
//! Institute of America, under the SIL Open Font License 1.1 (`fonts/OFL.txt`).
//!
//! Its family name is `Atkinson Hyperlegible`. It has a regular and a bold face and no italic one,
//! so a test sees bold select a real face and italic slant the upright one. Tests of every crate
//! share these bytes, so text measures the same in all of them.

/// The regular face, weight 400, as a TrueType file.
pub const REGULAR: &[u8] = include_bytes!("../fonts/AtkinsonHyperlegible-Regular.ttf");

/// The bold face, weight 700, as a TrueType file.
pub const BOLD: &[u8] = include_bytes!("../fonts/AtkinsonHyperlegible-Bold.ttf");

#[cfg(test)]
mod tests {
  use super::*;

  // Both faces are TrueType files: they start with the 0x00010000 version tag.
  #[test]
  fn faces_are_truetype_files() {
    assert_eq!(REGULAR[..4], [0, 1, 0, 0]);
    assert_eq!(BOLD[..4], [0, 1, 0, 0]);
    assert_ne!(REGULAR, BOLD);
  }
}
