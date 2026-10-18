//! Test fixture: derives one XAML type, `Beta`, and nothing else.

use artic_macros::UiType;

#[derive(Debug, UiType)]
#[ui(namespace = "https://fixtures.artic.test/b")]
struct Beta;
