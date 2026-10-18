//! One [`FontData`] per face, so a face keeps one id for the engine's life.
//!
//! Faces are keyed by `fontique`'s blob id and face index. A blob keeps its id only while someone
//! holds it: the table does, through each `FontData`'s bytes, and the system engine's shared source
//! cache hands back that same blob when it maps a dropped file again. So entries are never evicted:
//! the table holds every face the engine has drawn from, which the installed fonts bound.

use artic_layout::text::FontData;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct Faces(HashMap<(u64, u32), FontData>);

impl Faces {
  pub(crate) fn font(&mut self, font: &parley::FontData) -> FontData {
    self
      .0
      .entry((font.data.id(), font.index))
      .or_insert_with(|| FontData::new(font.data.clone().into_raw_parts().0, font.index))
      .clone()
  }

  pub(crate) fn len(&self) -> usize {
    self.0.len()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use fontique::Blob;
  use std::sync::Arc;

  // Two faces of one collection file share its blob and differ by index, so each gets its own id,
  // and asking for a face again gives back the same font data.
  #[test]
  fn collection_faces_get_their_own_ids() {
    let blob = Blob::new(Arc::new(vec![0_u8; 8]));
    let first = parley::FontData::new(blob.clone(), 0);
    let second = parley::FontData::new(blob, 1);
    let mut faces = Faces::default();

    let font = faces.font(&first);

    assert_ne!(font.id(), faces.font(&second).id());
    assert_eq!(faces.font(&first), font);
    assert_eq!(faces.len(), 2);
  }

  // The font data shares the blob's bytes instead of copying them.
  #[test]
  fn font_data_shares_the_blob_bytes() {
    let blob = Blob::new(Arc::new(vec![1_u8, 2, 3]));
    let font = Faces::default().font(&parley::FontData::new(blob.clone(), 0));

    assert!(std::ptr::eq((**font.bytes()).as_ref(), blob.data()));
  }
}
