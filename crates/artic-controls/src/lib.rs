//! artic's controls: [`Border`], [`StackPanel`] and the [`Panel`] group every panel flattens,
//! [`UserControl`], [`TextBlock`] and the inlines inside it ([`Run`], [`Bold`], [`Italic`]), and
//! [`TextElement`], the font and brush text inherits, which [`TextFormat`] sets.

mod border;
mod inline;
mod panel;
mod stack_panel;
mod text_block;
mod text_element;
mod user_control;

pub use border::Border;
pub use inline::{Bold, Inlines, Italic, Run};
pub use panel::{Panel, PanelMethods};
pub use stack_panel::StackPanel;
pub use text_block::TextBlock;
pub use text_element::{TextElement, TextFormat, TextFormatMethods};
pub use user_control::UserControl;
