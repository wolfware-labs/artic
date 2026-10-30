// `ARTIC_REQUIRE_GPU=1` turns a skipped golden test into a failure. Setting the variable is sound
// only while no other thread reads the environment, so this test runs without the harness, on the
// main thread alone.

use std::panic;

// A job that promises a GPU fails when a golden test finds none, naming the reason.
fn main() {
  // SAFETY: no other thread has started, so nothing reads the environment meanwhile.
  unsafe { std::env::set_var("ARTIC_REQUIRE_GPU", "1") };
  let hook = panic::take_hook();
  panic::set_hook(Box::new(|_| {}));
  let failed = panic::catch_unwind(|| artic_test::golden::no_gpu("no adapter"));
  panic::set_hook(hook);

  let payload = failed.expect_err("no_gpu didn't fail with ARTIC_REQUIRE_GPU=1");
  assert_eq!(
    payload.downcast_ref::<String>().map(String::as_str),
    Some("no usable GPU, and ARTIC_REQUIRE_GPU=1: no adapter")
  );
}
