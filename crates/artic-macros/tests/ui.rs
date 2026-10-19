// Cases for the macros. Each `tests/ui/*.rs` file must fail to build with exactly the error in
// its `.stderr` file; each `tests/ui/pass/*.rs` file must build and run. Regenerate the errors
// with `TRYBUILD=overwrite`.

#[test]
fn ui() {
  let cases = trybuild::TestCases::new();
  cases.compile_fail("tests/ui/*.rs");
  cases.pass("tests/ui/pass/*.rs");
}
