//! The hello app's code type: the element at the root of `app.xaml`.

/// The app, with its one window.
#[artic::ui_class("app.xaml")]
#[derive(Debug)]
pub struct HelloApp;
