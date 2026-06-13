//! Accès aux fenêtres via l'API d'Accessibilité : identifiant CoreGraphics
//! d'un `AXUIElement` et titres de fenêtres (sans permission d'enregistrement
//! d'écran).

use core::ptr::{self, NonNull};

use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFArray, CFRetained, CFString, CFType};

use super::WindowId;

// API privée : associe un AXUIElement de fenêtre à son identifiant CoreGraphics.
unsafe extern "C" {
    fn _AXUIElementGetWindow(element: &AXUIElement, out: *mut WindowId) -> i32;
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

/// Titres (`AXTitle`) des fenêtres de l'application `pid`, indexés par
/// identifiant CoreGraphics. Vide si l'accès échoue.
pub fn titles_for_pid(pid: i32) -> Vec<(WindowId, String)> {
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
            if let Some(title) = ax_string(element, "AXTitle") {
                if !title.is_empty() {
                    out.push((id, title));
                }
            }
        }
    }
    out
}

/// Lit un attribut tableau (ex. `AXWindows`) d'un élément.
fn copy_attribute_array(element: &AXUIElement, attribute: &'static str) -> Option<CFRetained<CFArray>> {
    let attr = CFString::from_static_str(attribute);
    let mut value: *const CFType = ptr::null();
    let err = unsafe { element.copy_attribute_value(&attr, NonNull::from(&mut value).cast()) };
    if err != AXError::Success || value.is_null() {
        return None;
    }
    // SAFETY: valeur possédée (+1) que l'on confie à CFRetained.
    Some(unsafe { CFRetained::from_raw(NonNull::new_unchecked(value as *mut CFArray)) })
}

/// Lit un attribut chaîne (ex. `AXTitle`) d'un élément.
fn ax_string(element: &AXUIElement, attribute: &'static str) -> Option<String> {
    let attr = CFString::from_static_str(attribute);
    let mut value: *const CFType = ptr::null();
    let err = unsafe { element.copy_attribute_value(&attr, NonNull::from(&mut value).cast()) };
    if err != AXError::Success || value.is_null() {
        return None;
    }
    // SAFETY: valeur possédée (+1).
    let cf = unsafe { CFRetained::from_raw(NonNull::new_unchecked(value as *mut CFType)) };
    cf.downcast_ref::<CFString>().map(|s| s.to_string())
}
