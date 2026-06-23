//! Petits constructeurs de vues AppKit partagés par l'interface (overlay et
//! fenêtre de préférences), pour éviter de dupliquer la configuration de base.

use objc2::rc::Retained;
use objc2::{msg_send, MainThreadOnly};
use objc2_app_kit::{NSBox, NSBoxType, NSColor, NSTitlePosition};
use objc2_foundation::{MainThreadMarker, NSRect};

/// Crée une `NSBox` personnalisée, sans titre ni bordure, aux coins arrondis.
/// Le remplissage est transparent par défaut ; l'appelant pose la couleur de
/// fond qu'il souhaite (`setFillColor`).
pub(crate) fn make_box(
    mtm: MainThreadMarker,
    frame: NSRect,
    corner_radius: f64,
) -> Retained<NSBox> {
    // SAFETY: `init` est la méthode d'initialisation correcte de NSBox.
    let boxed: Retained<NSBox> = unsafe { msg_send![NSBox::alloc(mtm), init] };
    boxed.setBoxType(NSBoxType::Custom);
    boxed.setTitlePosition(NSTitlePosition::NoTitle);
    boxed.setBorderWidth(0.0);
    boxed.setFillColor(&NSColor::clearColor());
    boxed.setCornerRadius(corner_radius);
    boxed.setFrame(frame);
    boxed
}
