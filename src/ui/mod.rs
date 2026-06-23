//! Interface graphique de Tabs : l'overlay du sélecteur de fenêtres.

mod layout;
mod overlay;
mod widgets;

pub use layout::{DisplayMode, Direction};
pub use overlay::Overlay;
pub(crate) use widgets::make_box;
