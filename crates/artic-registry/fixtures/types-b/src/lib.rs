//! Test fixture: derives two UI types, `Beta` and `Twin`, and nothing else.

use artic_macros::UiType;

#[derive(Debug, UiType)]
struct Beta;

#[derive(Debug, UiType)]
struct Twin;
