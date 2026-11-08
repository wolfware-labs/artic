//! What an app crate hands its launchers, and running it.

use crate::hooks;
use artic_core::tree::{Handle, NodeId, UiTree};
use artic_element::{Application, ElementKind};
use artic_platform::{Error, Host, Runner};
use artic_render::Renderer;
use std::fmt::{self, Debug};

/// What an app crate hands its launchers: how to build its tree, which [`run`] does just before
/// the host starts.
pub struct App {
  build: Box<dyn FnOnce(&mut UiTree) -> NodeId>,
}

impl App {
  /// The app whose tree `build` creates, with an `Application`, or a code type of one, at its root.
  ///
  /// A root of another type doesn't compile:
  ///
  /// ```compile_fail
  /// artic::App::new(|tree| artic::controls::Border::new().build(tree));
  /// ```
  pub fn new<T: ElementKind<Kind = Application>>(build: impl FnOnce(&mut UiTree) -> Handle<T> + 'static) -> App {
    App {
      build: Box::new(move |tree| build(tree).id()),
    }
  }
}

impl Debug for App {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("App").finish_non_exhaustive()
  }
}

/// Runs `app` on `host`, drawing with `renderer`: builds its tree, then runs it as
/// [`artic_platform::run`] does, with text shaped by the system's fonts.
///
/// # Errors
///
/// [`Error::Unsupported`] before the host starts if `renderer` can't draw on its targets; then the
/// first frame's [`Error::Render`], or else the host's own [`Error::Host`].
pub fn run<H: Host, R: Renderer + 'static>(host: H, renderer: R, app: App) -> Result<(), Error> {
  let mut tree = UiTree::new();
  let root = (app.build)(&mut tree);
  let mut runner = Runner::new(tree, renderer, Box::new(artic_text::Engine::system()), root);
  hooks::register(&mut runner);
  artic_platform::run(host, runner)
}
