//! Accès aux fenêtres via l'API d'Accessibilité : identifiant CoreGraphics
//! d'un `AXUIElement`, titre (`AXTitle`) et état minimisé (`AXMinimized`).
//! Ne nécessite pas la permission d'enregistrement d'écran.

use core::ptr::{self, NonNull};

use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFArray, CFBoolean, CFRetained, CFString, CFType};

use super::WindowId;

// API privée : associe un AXUIElement de fenêtre à son identifiant CoreGraphics.
unsafe extern "C" {
    fn _AXUIElementGetWindow(element: &AXUIElement, out: *mut WindowId) -> i32;
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

/// Fenêtres de l'application `pid` (y compris minimisées), via l'Accessibilité.
pub fn windows_for_pid(pid: i32) -> Vec<AxWindow> {
    let app = unsafe { AXUIElement::new_application(pid) };
    let Some(array) = copy_attribute_array(&app, "AXWindows") else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for i in 0..array.count() {
        let ptr = unsafe { array.value_at_index(i) } as *const AXUIElement;
        if ptr.is_null() {
            continue;
        }
        let element = unsafe { &*ptr };
        if let Some(id) = window_id(element) {
            out.push(AxWindow {
                id,
                title: ax_string(element, "AXTitle").unwrap_or_default(),
                minimized: ax_bool(element, "AXMinimized").unwrap_or(false),
            });
        }
    }
    out
}

/// Lit un attribut tableau (ex. `AXWindows`).
fn copy_attribute_array(
    element: &AXUIElement,
    attribute: &'static str,
) -> Option<CFRetained<CFArray>> {
    copy_attribute(element, attribute)?.downcast::<CFArray>().ok()
}

/// Lit un attribut chaîne (ex. `AXTitle`).
fn ax_string(element: &AXUIElement, attribute: &'static str) -> Option<String> {
    Some(copy_attribute(element, attribute)?.downcast_ref::<CFString>()?.to_string())
}

/// Lit un attribut booléen (ex. `AXMinimized`).
fn ax_bool(element: &AXUIElement, attribute: &'static str) -> Option<bool> {
    Some(copy_attribute(element, attribute)?.downcast_ref::<CFBoolean>()?.value())
}

/// Copie une valeur d'attribut AX (possédée, +1).
fn copy_attribute(element: &AXUIElement, attribute: &'static str) -> Option<CFRetained<CFType>> {
    let attr = CFString::from_static_str(attribute);
    let mut value: *const CFType = ptr::null();
    let err = unsafe { element.copy_attribute_value(&attr, NonNull::from(&mut value).cast()) };
    if err != AXError::Success || value.is_null() {
        return None;
    }
    // SAFETY: valeur possédée (+1) que l'on confie à CFRetained.
    Some(unsafe { CFRetained::from_raw(NonNull::new_unchecked(value as *mut CFType)) })
}
