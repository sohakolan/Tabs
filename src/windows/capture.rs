//! Capture d'une miniature de fenêtre.
//!
//! M5 utilise `CGWindowListCreateImage`, synchrone et simple, qui rend une image
//! de la fenêtre par son identifiant. Cette API est dépréciée au profit de
//! ScreenCaptureKit ; une migration vers SCK (capture asynchrone, rendu
//! zéro-copie via IOSurface) est prévue pour les performances. Elle requiert,
//! comme SCK, la permission d'Enregistrement de l'écran.

use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{CGImage, CGRectNull, CGWindowImageOption, CGWindowListOption};
#[allow(deprecated)]
use objc2_core_graphics::CGWindowListCreateImage;

use super::WindowId;

/// Capture une image de la fenêtre `id`, ou `None` si indisponible (permission
/// d'Enregistrement de l'écran manquante, fenêtre hors écran, etc.).
pub fn capture(id: WindowId) -> Option<CFRetained<CGImage>> {
    // `CGRectNull` indique « utiliser les limites propres de la fenêtre ».
    let bounds = unsafe { CGRectNull };
    #[allow(deprecated)]
    CGWindowListCreateImage(
        bounds,
        CGWindowListOption::OptionIncludingWindow,
        id,
        CGWindowImageOption::BoundsIgnoreFraming | CGWindowImageOption::NominalResolution,
    )
}
