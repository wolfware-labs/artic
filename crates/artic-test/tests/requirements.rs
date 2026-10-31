// `ARTIC_REQUIRE_GPU=1` and `ARTIC_REQUIRE_DISPLAY=1` turn a skipped test into a failure. Setting
// the variables is sound only while no other thread reads the environment, so this test runs
// without the harness, on the main thread alone.

use std::panic;

// A job that promises a GPU or a display fails when a test finds none, naming the reason.
fn main() {
  // SAFETY: no other thread has started, so nothing reads the environment meanwhile.
  unsafe {
    std::env::set_var("ARTIC_REQUIRE_GPU", "1");
    std::env::set_var("ARTIC_REQUIRE_DISPLAY", "1");
  }
  let hook = panic::take_hook();
  panic::set_hook(Box::new(|_| {}));
  let gpu = panic::catch_unwind(|| artic_test::require::no_gpu("no adapter"));
  let display = panic::catch_unwind(|| artic_test::require::no_display("no server"));
  panic::set_hook(hook);

  let message = |failed: std::thread::Result<()>| failed.err().and_then(|payload| payload.downcast::<String>().ok());
  assert_eq!(
    message(gpu).as_deref().map(String::as_str),
    Some("no usable GPU, and ARTIC_REQUIRE_GPU=1: no adapter")
  );
  assert_eq!(
    message(display).as_deref().map(String::as_str),
    Some("no X11 display, and ARTIC_REQUIRE_DISPLAY=1: no server")
  );
}
