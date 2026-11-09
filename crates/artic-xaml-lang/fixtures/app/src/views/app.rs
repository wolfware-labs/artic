//! A code type on an `Application`: the element at the root of `app.xaml`.

/// The app, with its one window.
#[artic::ui_class("app.xaml")]
#[derive(Debug)]
pub struct FixtureApp;
