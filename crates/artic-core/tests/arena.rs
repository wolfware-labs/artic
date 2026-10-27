use artic_core::Object;
use artic_core::property::PropertyVisitor;
use artic_core::tree::UiTree;

#[test]
fn allocate_control() {
  #[derive(Debug, PartialEq, Eq)]
  struct Control;
  impl Object for Control {
    fn visit_properties(&self, _visitor: &mut PropertyVisitor<'_>) {}
  }

  let mut tree = UiTree::new();
  let id = tree.alloc(Box::new(Control));
  assert_eq!(tree.get_as::<Control>(id), Some(&Control));
}
