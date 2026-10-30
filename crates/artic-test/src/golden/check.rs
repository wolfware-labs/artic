use super::{Image, flag};
use artic_core::error::Chain;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// How far an image may stray from its golden.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Tolerance {
  /// The most any channel of a pixel may differ before the pixel counts as differing. Colors are
  /// compared premultiplied by their alpha, so a clear pixel's color doesn't count.
  pub channel: u8,
  /// The fraction of the image's pixels that may differ, from `0` to `1`, rounded down to whole
  /// pixels. Below `0` or NaN counts as `0`, above `1` as `1`.
  pub pixels: f32,
}

/// A reference picture an image is checked against, usually made with [`golden!`](crate::golden!).
#[derive(Debug, Clone)]
pub struct Golden {
  path: PathBuf,
  tolerance: Tolerance,
  per_os: bool,
}

/// Why an image doesn't match its golden.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GoldenError {
  /// There's no golden yet.
  #[error("no golden at {}; run with ARTIC_UPDATE_GOLDENS=1 to create it", golden.display())]
  Missing {
    /// Where the golden should be.
    golden: PathBuf,
  },
  /// The image and the golden differ in size.
  #[error(
    "the image is {}×{} but the golden is {}×{}; the image is at {}",
    found.0, found.1, expected.0, expected.1, actual.display()
  )]
  Size {
    /// The golden's width and height.
    expected: (u32, u32),
    /// The image's width and height.
    found: (u32, u32),
    /// Where the image was written.
    actual: PathBuf,
  },
  /// More pixels differ than the tolerance allows.
  #[error(
    "{differing} pixels differ and {allowed} may; the image is at {} and its differences at {}",
    actual.display(), diff.display()
  )]
  Pixels {
    /// How many pixels differ.
    differing: usize,
    /// How many may.
    allowed: usize,
    /// Where the image was written.
    actual: PathBuf,
    /// Where the picture of the differences was written: differing pixels red, the rest faded.
    diff: PathBuf,
  },
  /// A file couldn't be read, decoded or written.
  #[error("{} couldn't be read, decoded or written", path.display())]
  Io {
    /// The file.
    path: PathBuf,
    /// What went wrong.
    #[source]
    error: io::Error,
  },
}

#[derive(Debug, Clone, Copy)]
enum Mode {
  Compare,
  Update,
}

const RED: [u8; 4] = [255, 0, 0, 255];

impl Default for Tolerance {
  fn default() -> Tolerance {
    Tolerance {
      channel: 8,
      pixels: 0.001,
    }
  }
}

impl Tolerance {
  /// For drawn text: the default, with 0.6% of the pixels allowed to differ. Rasterizers cover a
  /// glyph's edge pixels slightly differently, such as a GPU and lavapipe, a Vulkan driver on the
  /// CPU, which differ in up to 0.3% of a text golden's pixels; shapes and fills match closer.
  pub fn text() -> Tolerance {
    Tolerance::default().with_pixels(0.006)
  }

  /// The same tolerance with `channel`.
  #[must_use]
  pub fn with_channel(self, channel: u8) -> Tolerance {
    Tolerance { channel, ..self }
  }

  /// The same tolerance with `pixels`.
  #[must_use]
  pub fn with_pixels(self, pixels: f32) -> Tolerance {
    Tolerance { pixels, ..self }
  }

  fn allowed(self, area: usize) -> usize {
    let fraction = if self.pixels.is_nan() {
      0.0
    } else {
      self.pixels.clamp(0.0, 1.0)
    };
    (f64::from(fraction) * area as f64).floor() as usize
  }
}

impl Mode {
  fn from_env() -> Mode {
    if flag("ARTIC_UPDATE_GOLDENS") {
      Mode::Update
    } else {
      Mode::Compare
    }
  }
}

impl Golden {
  /// The golden at `path`, with the default tolerance.
  #[must_use]
  pub fn at(path: impl Into<PathBuf>) -> Golden {
    Golden {
      path: path.into(),
      tolerance: Tolerance::default(),
      per_os: false,
    }
  }

  /// The same golden with `tolerance`.
  #[must_use]
  pub fn with_tolerance(self, tolerance: Tolerance) -> Golden {
    Golden { tolerance, ..self }
  }

  /// The same golden, kept once per operating system: for tests that draw with the system's
  /// fonts, which differ between systems. Text in the bundled test font draws the same everywhere
  /// and needs no `per_os`.
  #[must_use]
  pub fn per_os(self) -> Golden {
    Golden { per_os: true, ..self }
  }

  /// The golden's file: its path, with the operating system before `.png` when it's
  /// [per OS](Golden::per_os).
  #[must_use]
  pub fn path(&self) -> PathBuf {
    if self.per_os {
      self.path.with_extension(format!("{}.png", std::env::consts::OS))
    } else {
      self.path.clone()
    }
  }

  /// Checks `actual` against the golden, or writes it as the golden when `ARTIC_UPDATE_GOLDENS`
  /// is `1`.
  ///
  /// On a mismatch it writes `<name>.actual.png`, and `<name>.diff.png` when the sizes agree,
  /// beside the golden. Every check removes whichever of the two it doesn't write, so a pass
  /// removes both and no output outlives its run.
  ///
  /// # Errors
  ///
  /// Returns why `actual` doesn't match, or the file that couldn't be read or written.
  pub fn check(&self, actual: &Image) -> Result<(), GoldenError> {
    self.check_in(actual, Mode::from_env())
  }

  /// [Checks](Golden::check) `actual` against the golden.
  ///
  /// # Panics
  ///
  /// When the check fails, with its error's message and causes.
  pub fn assert(&self, actual: &Image) {
    if let Err(error) = self.check(actual) {
      panic!("{}", Chain(&error));
    }
  }

  fn check_in(&self, actual: &Image, mode: Mode) -> Result<(), GoldenError> {
    let golden = self.path();
    let (written, diff) = (beside(&golden, "actual"), beside(&golden, "diff"));
    if let Mode::Update = mode {
      if let Some(folder) = golden.parent() {
        fs::create_dir_all(folder).map_err(at(folder))?;
      }
      actual.save(&golden).map_err(at(&golden))?;
      return remove(&[written.as_path(), diff.as_path()]);
    }
    let expected = match Image::load(&golden) {
      Ok(expected) => expected,
      Err(error) => {
        remove(&[written.as_path(), diff.as_path()])?;
        return Err(if error.kind() == io::ErrorKind::NotFound {
          GoldenError::Missing { golden }
        } else {
          GoldenError::Io { path: golden, error }
        });
      }
    };
    let (expected_size, found) = ((expected.width(), expected.height()), (actual.width(), actual.height()));
    if expected_size != found {
      remove(&[diff.as_path()])?;
      actual.save(&written).map_err(at(&written))?;
      return Err(GoldenError::Size {
        expected: expected_size,
        found,
        actual: written,
      });
    }
    let (differing, picture) = compare(&expected, actual, self.tolerance.channel);
    let allowed = self.tolerance.allowed(actual.pixels().len());
    if differing <= allowed {
      return remove(&[written.as_path(), diff.as_path()]);
    }
    actual.save(&written).map_err(at(&written))?;
    picture.save(&diff).map_err(at(&diff))?;
    Err(GoldenError::Pixels {
      differing,
      allowed,
      actual: written,
      diff,
    })
  }
}

fn compare(expected: &Image, actual: &Image, channel: u8) -> (usize, Image) {
  let mut differing = 0;
  let pixels = expected
    .pixels()
    .iter()
    .zip(actual.pixels())
    .map(|(expected, actual)| {
      let (expected_color, actual_color) = (premultiplied(*expected), premultiplied(*actual));
      if expected_color.iter().zip(actual_color).any(|(e, a)| e.abs_diff(a) > channel) {
        differing += 1;
        RED
      } else {
        faded(*expected)
      }
    })
    .collect();
  (differing, Image::new(expected.width(), expected.height(), pixels))
}

fn premultiplied([r, g, b, a]: [u8; 4]) -> [u8; 4] {
  let scale = |channel: u8| ((u16::from(channel) * u16::from(a) + 127) / 255) as u8;
  [scale(r), scale(g), scale(b), a]
}

fn faded([r, g, b, a]: [u8; 4]) -> [u8; 4] {
  let luma = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
  let gray = (192 + luma * u32::from(a) / 255 / 4) as u8;
  [gray, gray, gray, 255]
}

fn beside(golden: &Path, kind: &str) -> PathBuf {
  golden.with_extension(format!("{kind}.png"))
}

fn at(path: &Path) -> impl FnOnce(io::Error) -> GoldenError + '_ {
  move |error| GoldenError::Io {
    path: path.to_path_buf(),
    error,
  }
}

fn remove(paths: &[&Path]) -> Result<(), GoldenError> {
  for path in paths {
    match fs::remove_file(path) {
      Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(at(path)(error)),
      _ => {}
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::golden::scratch;
  use pretty_assertions::assert_eq;

  fn plain(color: [u8; 4]) -> Image {
    Image::new(64, 64, vec![color; 64 * 64])
  }

  fn with_changed(count: usize) -> Image {
    let mut pixels = plain([10, 20, 30, 255]).pixels().to_vec();
    for pixel in pixels.iter_mut().take(count) {
      *pixel = [200, 20, 30, 255];
    }
    Image::new(64, 64, pixels)
  }

  fn golden_in(name: &str) -> Golden {
    let golden = Golden::at(scratch(name).join("golden.png"));
    plain([10, 20, 30, 255]).save(&golden.path()).expect("saves");
    golden
  }

  // The diff image shows unchanged pixels as light gray, from 192 for black to 255 for white, and
  // lighter the more transparent they are.
  #[test]
  fn faded_grays_by_luma_under_alpha() {
    let faded = [[0, 0, 0, 255], [255, 255, 255, 255], [200, 100, 50, 255], [200, 100, 50, 128]].map(faded);

    assert_eq!(
      faded,
      [
        [192, 192, 192, 255],
        [255, 255, 255, 255],
        [223, 223, 223, 255],
        [207, 207, 207, 255]
      ]
    );
  }

  // Removing a file that isn't there is fine; any other failure, such as a directory in its place,
  // is reported.
  #[test]
  fn remove_reports_errors_other_than_missing() {
    let dir = scratch("remove");
    std::fs::create_dir_all(&dir).expect("creates");

    assert!(remove(&[&dir.join("missing.png")]).is_ok());
    assert!(matches!(remove(&[&dir]), Err(GoldenError::Io { .. })));
  }

  // An image compared with itself has no differing pixel.
  #[test]
  fn identical_images_match() {
    let (differing, _) = compare(&plain([1, 2, 3, 4]), &plain([1, 2, 3, 4]), 8);

    assert_eq!(differing, 0);
  }

  // A channel off by the tolerance still matches; one level more differs.
  #[test]
  fn small_differences_within_tolerance_match() {
    let (within, _) = compare(&plain([100, 100, 100, 255]), &plain([108, 92, 100, 255]), 8);
    let (beyond, _) = compare(&plain([100, 100, 100, 255]), &plain([109, 100, 100, 255]), 8);

    assert_eq!(within, 0);
    assert_eq!(beyond, 64 * 64);
  }

  // Colors are compared premultiplied by their alpha: a clear pixel matches whatever color a GPU
  // leaves in it, and a nearly clear one barely counts.
  #[test]
  fn transparent_pixels_match_whatever_their_color() {
    let (clear, _) = compare(&plain([0, 0, 0, 0]), &plain([113, 34, 73, 0]), 8);
    let (faint, _) = compare(&plain([0, 0, 0, 2]), &plain([255, 0, 0, 2]), 8);

    assert_eq!((clear, faint), (0, 0));
  }

  // 0.1% of a 64×64 image is 4 pixels: 4 differing pixels pass and 5 fail.
  #[test]
  fn too_many_differing_pixels_fail() {
    let golden = golden_in("too_many_differing_pixels_fail");

    assert!(golden.check_in(&with_changed(4), Mode::Compare).is_ok());
    let error = golden.check_in(&with_changed(5), Mode::Compare).expect_err("too many");

    assert!(
      matches!(
        error,
        GoldenError::Pixels {
          differing: 5,
          allowed: 4,
          ..
        }
      ),
      "{error:?}"
    );
  }

  // An image of another size fails and is written for a look.
  #[test]
  fn different_sizes_fail() {
    let golden = golden_in("different_sizes_fail");

    let error = golden
      .check_in(&Image::new(2, 1, vec![[0; 4]; 2]), Mode::Compare)
      .expect_err("size");

    let GoldenError::Size { expected, found, actual } = error else {
      panic!("{error:?}")
    };
    assert_eq!((expected, found), ((64, 64), (2, 1)));
    assert_eq!(Image::load(&actual).expect("written").width(), 2);
  }

  // A failure that writes no diff removes the one an earlier failure left, so no diff outlives
  // the run it belongs to.
  #[test]
  fn failures_without_a_diff_remove_the_old_one() {
    let golden = golden_in("failures_without_a_diff_remove_the_old_one");
    let diff = golden.path().with_file_name("golden.diff.png");
    let _ = golden.check_in(&with_changed(10), Mode::Compare);

    let _ = golden.check_in(&Image::new(2, 1, vec![[0; 4]; 2]), Mode::Compare);
    let size_left_it = diff.exists();
    let _ = golden.check_in(&with_changed(10), Mode::Compare);
    fs::remove_file(golden.path()).expect("removes");
    let _ = golden.check_in(&with_changed(10), Mode::Compare);

    assert_eq!((size_left_it, diff.exists()), (false, false));
    assert!(!golden.path().with_file_name("golden.actual.png").exists());
  }

  // A mismatch writes the image and a diff beside the golden: changed pixels red, the rest
  // faded to gray.
  #[test]
  fn golden_mismatch_writes_diff() {
    let golden = golden_in("golden_mismatch_writes_diff");

    let error = golden.check_in(&with_changed(10), Mode::Compare).expect_err("differs");

    let GoldenError::Pixels { actual, diff, .. } = error else {
      panic!("{error:?}")
    };
    assert_eq!(actual, golden.path().with_file_name("golden.actual.png"));
    assert_eq!(diff, golden.path().with_file_name("golden.diff.png"));
    assert_eq!(Image::load(&actual).expect("written"), with_changed(10));
    let diff = Image::load(&diff).expect("written");
    assert_eq!(diff.pixel(0, 0), RED);
    assert_ne!(diff.pixel(63, 63), RED);
    assert_eq!(diff.pixel(63, 63)[0], diff.pixel(63, 63)[1]);
  }

  // A passing check removes what an earlier failure left.
  #[test]
  fn passing_check_removes_old_outputs() {
    let golden = golden_in("passing_check_removes_old_outputs");
    let _ = golden.check_in(&with_changed(10), Mode::Compare);

    golden.check_in(&plain([10, 20, 30, 255]), Mode::Compare).expect("matches");

    assert!(!golden.path().with_file_name("golden.actual.png").exists());
    assert!(!golden.path().with_file_name("golden.diff.png").exists());
  }

  // A missing golden names the variable that creates it.
  #[test]
  fn missing_golden_says_how_to_create_it() {
    let golden = Golden::at(scratch("missing_golden_says_how_to_create_it").join("none.png"));

    let error = golden.check_in(&plain([0; 4]), Mode::Compare).expect_err("missing");

    assert!(matches!(error, GoldenError::Missing { .. }), "{error:?}");
    assert!(error.to_string().contains("ARTIC_UPDATE_GOLDENS=1"), "{error}");
  }

  // A golden that isn't a valid PNG is an I/O error of kind InvalidData, naming the file.
  #[test]
  fn unreadable_golden_is_an_io_error() {
    let golden = Golden::at(scratch("unreadable_golden_is_an_io_error").join("bad.png"));
    fs::write(golden.path(), b"not a png").expect("writes");

    let error = golden.check_in(&plain([0; 4]), Mode::Compare).expect_err("unreadable");

    let source = std::error::Error::source(&error).and_then(|source| source.downcast_ref::<io::Error>());
    assert_eq!(source.map(io::Error::kind), Some(io::ErrorKind::InvalidData));
    let GoldenError::Io { path, error } = error else {
      panic!("{error:?}")
    };
    assert_eq!((path, error.kind()), (golden.path(), io::ErrorKind::InvalidData));
  }

  // `assert` panics with the message and the cause, so the decoder's words still reach the test
  // log.
  #[test]
  fn assert_names_the_cause_of_an_unreadable_golden() {
    let golden = Golden::at(scratch("assert_names_the_cause_of_an_unreadable_golden").join("bad.png"));
    fs::write(golden.path(), b"not a png").expect("writes");
    let error = golden.check_in(&plain([0; 4]), Mode::Compare).expect_err("unreadable");
    let cause = std::error::Error::source(&error).expect("the io error").to_string();

    let panic =
      std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| golden.assert(&plain([0; 4])))).expect_err("assert panics");

    assert_eq!(panic.downcast_ref::<String>(), Some(&format!("{error}: {cause}")));
  }

  // A golden that can't be read removes what an earlier failure left beside it.
  #[test]
  fn unreadable_golden_removes_old_outputs() {
    let golden = golden_in("unreadable_golden_removes_old_outputs");
    let _ = golden.check_in(&with_changed(10), Mode::Compare);
    fs::write(golden.path(), b"not a png").expect("writes");

    let _ = golden.check_in(&with_changed(10), Mode::Compare);

    assert!(!golden.path().with_file_name("golden.actual.png").exists());
    assert!(!golden.path().with_file_name("golden.diff.png").exists());
  }

  // Updating creates the folder, writes the golden and clears old failures; updating again writes
  // the same bytes, and comparing afterwards passes.
  #[test]
  fn updating_twice_changes_no_bytes() {
    let golden = Golden::at(scratch("updating_twice_changes_no_bytes").join("new").join("golden.png"));
    let image = with_changed(3);

    golden.check_in(&image, Mode::Update).expect("writes");
    let first = fs::read(golden.path()).expect("written");
    fs::write(golden.path().with_file_name("golden.diff.png"), b"stale").expect("writes");
    golden.check_in(&image, Mode::Update).expect("writes");

    assert_eq!(fs::read(golden.path()).expect("written"), first);
    assert!(!golden.path().with_file_name("golden.diff.png").exists());
    golden.check_in(&image, Mode::Compare).expect("matches");
  }

  // A fraction below 0 or NaN allows no pixel, and one above 1 allows all.
  #[test]
  fn tolerance_fraction_is_clamped() {
    let allowed = |pixels| Tolerance::default().with_pixels(pixels).allowed(4096);

    assert_eq!(
      [allowed(f32::NAN), allowed(-1.0), allowed(0.001), allowed(2.0)],
      [0, 0, 4, 4096]
    );
  }

  // Text allows the glyph edges lavapipe draws differently from a GPU: 69 of a 160×160 image's
  // pixels, measured, fit with room to spare, and a pixel still counts at the default channel.
  #[test]
  fn text_tolerance_allows_glyph_edges_drawn_on_another_rasterizer() {
    let text = Tolerance::text();

    assert!(text.allowed(160 * 160) >= 2 * 69, "{text:?}");
    assert_eq!(text.channel, Tolerance::default().channel);
  }

  // A golden per system puts the system's name before `.png`, and its failures beside that name.
  #[test]
  fn per_os_names_the_os() {
    let golden = Golden::at("goldens/text.png").per_os();
    let os = std::env::consts::OS;

    assert_eq!(golden.path(), PathBuf::from(format!("goldens/text.{os}.png")));
    assert_eq!(
      beside(&golden.path(), "diff"),
      PathBuf::from(format!("goldens/text.{os}.diff.png"))
    );
  }

  // The defaults are a channel difference of 8 and 0.1% of the pixels.
  #[test]
  fn default_tolerance() {
    assert_eq!(
      Tolerance::default(),
      Tolerance {
        channel: 8,
        pixels: 0.001
      }
    );
    assert_eq!(Tolerance::default().with_channel(16).channel, 16);
  }
}
