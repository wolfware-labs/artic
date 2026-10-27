// The facade's modules are its crates, so code that depends only on the facade reaches every
// artic type at `artic::<crate>::<Name>`.

use artic::core::tree::{Handle, UiTree};

// A control built through the facade's paths alone, the way generated code writes it.
#[test]
fn facade_paths_build_a_control() {
  let mut tree = UiTree::new();

  let border: Handle<::artic::controls::Border> = ::artic::controls::Border::new()
    .padding(::artic::layout::Thickness::uniform(2.0))
    .build(&mut tree);

  assert_eq!(border.get(&tree).padding.get(), ::artic::layout::Thickness::uniform(2.0));
  let _: ::artic::render::Color = ::artic::render::Color::rgb(0, 0, 0);
  let _: Option<&::artic::element::Surface> = None;
}
