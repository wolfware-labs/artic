//! Test fixture: derives two UI types, `Alpha` and `Twin`, and nothing else.

use artic_macros::UiType;

#[derive(Debug, UiType)]
struct Alpha;

#[derive(Debug, UiType)]
struct Twin;
