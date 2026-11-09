//! Compiles the views of the hello example.

fn main() {
  artic::xaml::compile_dir("examples/hello", &artic::xaml::namespaces()).unwrap_or_else(|error| panic!("{error:?}"));
}
