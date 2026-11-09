// Hello world: an app built from one view, `app.xaml`, with its code type `HelloApp`, run through
// `artic::run` with the winit host and the vello renderer. With `ARTIC_HELLO_SECONDS` set, it ends
// with code 0 that many seconds after the host and the renderer start, as CI runs it.
//
// Run with `cargo run -p artic-examples --example hello`.

mod app;

use artic_examples::Report;
use std::fmt::{self, Display, Formatter};
use std::time::Duration;

/// The variable that says how many seconds to run.
const SECONDS_VARIABLE: &str = "ARTIC_HELLO_SECONDS";

/// The app every launcher of it runs.
fn app() -> artic::App {
  artic::App::new(app::HelloApp::create)
}

fn main() -> Result<(), Report> {
  let value = std::env::var_os(SECONDS_VARIABLE).map(|value| value.to_string_lossy().into_owned());
  let limit = seconds(value.as_deref())?;
  let (host, renderer) = (artic_winit::Host::new()?, artic_vello::Renderer::new()?);
  if let Some(after) = limit {
    std::thread::spawn(move || {
      std::thread::sleep(after);
      std::process::exit(0);
    });
  }
  artic::run(host, renderer, app())?;
  Ok(())
}

/// How long to run, from `ARTIC_HELLO_SECONDS`: until the window is closed when it isn't set.
fn seconds(value: Option<&str>) -> Result<Option<Duration>, BadSeconds> {
  value
    .map(|text| text.parse().map(Duration::from_secs).map_err(|_| BadSeconds(text.to_owned())))
    .transpose()
}

/// `ARTIC_HELLO_SECONDS` set to something other than a whole number of seconds.
#[derive(Debug, PartialEq, Eq)]
struct BadSeconds(String);

impl Display for BadSeconds {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    write!(f, "`{SECONDS_VARIABLE}` is `{}`, not a whole number of seconds", self.0)
  }
}

impl std::error::Error for BadSeconds {}

#[cfg(test)]
mod tests {
  use super::*;
  use artic::UiTree;
  use artic::controls::{Border, StackPanel, TextBlock};
  use artic::core::tree::TreeKind;
  use artic::element::Surface;

  // The app is one window titled "Hello", holding a border around a panel of the greeting's lines.
  #[test]
  fn hello_view_tree_shape() {
    let mut tree = UiTree::new();
    let app = app::HelloApp::create(&mut tree);
    let children = |node| tree.children(TreeKind::Logical, node).collect::<Vec<_>>();

    let [surface] = children(app.id())[..] else {
      panic!("the app holds one surface");
    };
    assert_eq!(tree.get_as::<Surface>(surface).expect("a surface").title.get(), "Hello");
    let [border] = children(surface)[..] else {
      panic!("the surface holds one child");
    };
    assert!(tree.get_as::<Border>(border).is_some());
    let [panel] = children(border)[..] else {
      panic!("the border holds one child");
    };
    assert!(tree.get_as::<StackPanel>(panel).is_some());
    let lines: Vec<String> = children(panel)
      .into_iter()
      .map(|node| tree.get_as::<TextBlock>(node).expect("a text block").text.get())
      .collect();
    assert_eq!(lines, ["Hello from artic", "A view written in XAML, with its code in Rust"]);
  }

  // Without the variable the app runs until its window is closed; with it, for that many seconds.
  #[test]
  fn seconds_reads_a_whole_number() {
    assert_eq!(seconds(None), Ok(None));
    assert_eq!(seconds(Some("3")), Ok(Some(Duration::from_secs(3))));
  }

  // A value that isn't a whole number of seconds is reported with the variable's name.
  #[test]
  fn seconds_rejects_other_text() {
    let error = seconds(Some("3s")).expect_err("not a number");

    assert_eq!(
      error.to_string(),
      "`ARTIC_HELLO_SECONDS` is `3s`, not a whole number of seconds"
    );
  }
}
