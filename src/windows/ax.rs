//! Accès aux fenêtres via l'API d'Accessibilité : identifiant CoreGraphics
//! d'un `AXUIElement`, titre (`AXTitle`) et état minimisé (`AXMinimized`).
//! Ne nécessite pas la permission d'enregistrement d'écran.

use core::ptr::{self, NonNull};
use std::collections::HashSet;

use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFArray, CFBoolean, CFRetained, CFString, CFType};

use super::WindowId;

// API privée : associe un AXUIElement de fenêtre à son identifiant CoreGraphics.
unsafe extern "C" {
    fn _AXUIElementGetWindow(element: &AXUIElement, out: *mut WindowId) -> i32;
}

// Noms d'attributs AX, construits une seule fois par thread. La liste des
// fenêtres lit `AXTitle`/`AXMinimized` une fois par fenêtre et `AXWindows` une
// fois par application : recréer ces `CFString` à chaque accès était du gaspillage
// et dispersait les littéraux. On les centralise ici, source unique de vérité.
thread_local! {
    static ATTRS: Attrs = Attrs::new();
}

struct Attrs {
    windows: CFRetained<CFString>,
    title: CFRetained<CFString>,
    minimized: CFRetained<CFString>,
    close_button: CFRetained<CFString>,
    press: CFRetained<CFString>,
}

impl Attrs {
    fn new() -> Self {
        Self {
            windows: CFString::from_static_str("AXWindows"),
            title: CFString::from_static_str("AXTitle"),
            minimized: CFString::from_static_str("AXMinimized"),
            close_button: CFString::from_static_str("AXCloseButton"),
            press: CFString::from_static_str("AXPress"),
        }
    }
}

/// Une fenêtre vue par l'Accessibilité.
pub struct AxWindow {
    pub id: WindowId,
    pub title: String,
    pub minimized: bool,
}

/// Identifiant CoreGraphics d'une fenêtre AX, le cas échéant.
pub fn window_id(element: &AXUIElement) -> Option<WindowId> {
    let mut id: WindowId = 0;
    let err = unsafe { _AXUIElementGetWindow(element, &mut id) };
    if err == 0 && id != 0 {
        Some(id)
    } else {
        None
    }
}

/// Ferme la fenêtre d'identifiant `id` de l'application `pid` en actionnant son
/// bouton de fermeture (`AXCloseButton` → `AXPress`), sans quitter l'application.
/// Retourne `true` si le bouton de fermeture a été trouvé et actionné.
pub fn close_window(pid: i32, id: WindowId) -> bool {
    let app = unsafe { AXUIElement::new_application(pid) };
    ATTRS.with(|a| {
        let Some(array) = copy_attribute_array(&app, &a.windows) else {
            return false;
        };

        for i in 0..array.count() {
            let ptr = unsafe { array.value_at_index(i) } as *const AXUIElement;
            if ptr.is_null() {
                continue;
            }
            let element = unsafe { &*ptr };
            if window_id(element) != Some(id) {
                continue;
            }

            let Some(button) = copy_attribute(element, &a.close_button) else {
                return false;
            };
            // SAFETY: `AXCloseButton` renvoie un AXUIElement ; `button` (CFRetained)
            // le maintient en vie le temps de l'action.
            let button = unsafe { &*(&*button as *const CFType as *const AXUIElement) };
            return unsafe { button.perform_action(&a.press) } == AXError::Success;
        }
        false
    })
}

/// Fenêtres de l'application `pid` (y compris minimisées), via l'Accessibilité.
///
/// Le titre (`AXTitle`, un aller-retour IPC) n'est lu que pour les fenêtres dont
/// le titre est réellement affiché : celles visibles à l'écran (`onscreen`) et
/// les fenêtres minimisées. Pour les autres (p. ex. une fenêtre sur un autre
/// Space, jamais listée), on évite cet aller-retour inutile.
pub fn windows_for_pid(pid: i32, onscreen: &HashSet<WindowId>) -> Vec<AxWindow> {
    let app = unsafe { AXUIElement::new_application(pid) };
    ATTRS.with(|a| {
        let Some(array) = copy_attribute_array(&app, &a.windows) else {
            return Vec::new();
        };

        let mut out = Vec::with_capacity(array.count() as usize);
        for i in 0..array.count() {
            let ptr = unsafe { array.value_at_index(i) } as *const AXUIElement;
            if ptr.is_null() {
                continue;
            }
            let element = unsafe { &*ptr };
            if let Some(id) = window_id(element) {
                let minimized = ax_bool(element, &a.minimized).unwrap_or(false);
                // Le titre n'est consommé que pour les fenêtres affichées.
                let title = if minimized || onscreen.contains(&id) {
                    ax_string(element, &a.title).unwrap_or_default()
                } else {
                    String::new()
                };
                out.push(AxWindow {
                    id,
                    title,
                    minimized,
                });
            }
        }
        out
    })
}

/// Lit un attribut tableau (ex. `AXWindows`).
pub(crate) fn copy_attribute_array(
    element: &AXUIElement,
    attribute: &CFString,
) -> Option<CFRetained<CFArray>> {
    copy_attribute(element, attribute)?.downcast::<CFArray>().ok()
}

/// Lit un attribut chaîne (ex. `AXTitle`).
fn ax_string(element: &AXUIElement, attribute: &CFString) -> Option<String> {
    Some(copy_attribute(element, attribute)?.downcast_ref::<CFString>()?.to_string())
}

/// Lit un attribut booléen (ex. `AXMinimized`).
fn ax_bool(element: &AXUIElement, attribute: &CFString) -> Option<bool> {
    Some(copy_attribute(element, attribute)?.downcast_ref::<CFBoolean>()?.value())
}

/// Copie une valeur d'attribut AX (possédée, +1).
fn copy_attribute(element: &AXUIElement, attribute: &CFString) -> Option<CFRetained<CFType>> {
    let mut value: *const CFType = ptr::null();
    let err = unsafe { element.copy_attribute_value(attribute, NonNull::from(&mut value).cast()) };
    if err != AXError::Success || value.is_null() {
        return None;
    }
    // SAFETY: valeur possédée (+1) que l'on confie à CFRetained.
    Some(unsafe { CFRetained::from_raw(NonNull::new_unchecked(value as *mut CFType)) })
}
