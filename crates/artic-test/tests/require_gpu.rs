// `ARTIC_REQUIRE_GPU=1` turns a skipped golden test into a failure. The test sets it, so it has
// this file to itself.

// A job that promises a GPU fails when a golden test finds none, naming the reason.
#[test]
#[should_panic(expected = "no usable GPU, and ARTIC_REQUIRE_GPU=1: no adapter")]
fn a_required_gpu_that_is_missing_fails() {
  // SAFETY: this binary's only test runs alone, so nothing else reads the environment meanwhile.
  unsafe { std::env::set_var("ARTIC_REQUIRE_GPU", "1") };

  artic_test::golden::no_gpu("no adapter");
}
