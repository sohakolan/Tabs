//! Activation d'une fenêtre : la mettre au premier plan et lui donner le focus.
//!
//! Stratégie (inspirée d'commutateur de fenêtres) :
//! 1. On crée l'`AXUIElement` de l'application propriétaire (`pid`).
//! 2. On parcourt ses fenêtres (`AXWindows`) en faisant correspondre chacune à
//!    son identifiant CoreGraphics via l'API privée `_AXUIElementGetWindow`,
//!    jusqu'à retrouver celle visée.
//! 3. On la lève (`AXRaise`) et on la marque comme principale (`AXMain`).
//! 4. On active l'application propriétaire pour qu'elle passe au premier plan.
//!
//! Tout repose sur la permission d'Accessibilité.

use core::ptr::{self, NonNull};

use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{kCFBooleanTrue, CFArray, CFRetained, CFString, CFType};

use super::{Window, WindowId};

// API privée : associe un AXUIElement de fenêtre à son identifiant CoreGraphics.
unsafe extern "C" {
    fn _AXUIElementGetWindow(element: &AXUIElement, out: *mut WindowId) -> i32;
}

/// Met la fenêtre `window` au premier plan et lui donne le focus.
///
/// Retourne `true` si la fenêtre AX correspondante a été trouvée et levée.
/// L'application est activée dans tous les cas (à condition d'exister).
pub fn activate(window: &Window) -> bool {
    let app = unsafe { AXUIElement::new_application(window.pid) };
    let raised = raise_matching_window(&app, window.id);

    // Amène l'application au premier plan (toutes ses fenêtres).
    if let Some(running) = NSRunningApplication::runningApplicationWithProcessIdentifier(window.pid)
    {
        running.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
    }

    raised
}

/// Cherche dans les fenêtres de l'application celle dont l'identifiant CG
/// correspond, puis la lève et la marque comme principale.
fn raise_matching_window(app: &AXUIElement, target: WindowId) -> bool {
    let Some(windows) = copy_windows(app) else {
        return false;
    };

    for i in 0..windows.count() {
        // SAFETY: index borné par count ; les entrées sont des AXUIElement.
        let ptr = unsafe { windows.value_at_index(i) } as *const AXUIElement;
        if ptr.is_null() {
            continue;
        }
        let element = unsafe { &*ptr };

        let mut id: WindowId = 0;
        let err = unsafe { _AXUIElementGetWindow(element, &mut id) };
        if err != 0 || id != target {
            continue;
        }

        // Fenêtre trouvée : on la lève et on la rend principale.
        let raise = CFString::from_static_str("AXRaise");
        let main = CFString::from_static_str("AXMain");
        unsafe {
            element.perform_action(&raise);
            if let Some(yes) = kCFBooleanTrue {
                let value: &CFType = &*(yes as *const _ as *const CFType);
                element.set_attribute_value(&main, value);
            }
        }
        return true;
    }

    false
}

/// Récupère le tableau des fenêtres (`AXWindows`) de l'application.
fn copy_windows(app: &AXUIElement) -> Option<CFRetained<CFArray>> {
    let attribute = CFString::from_static_str("AXWindows");
    let mut value: *const CFType = ptr::null();

    // SAFETY: `value` est un pointeur de sortie valide ; en cas de succès il
    // reçoit une valeur possédée (+1) que l'on confie à CFRetained.
    let err = unsafe { app.copy_attribute_value(&attribute, NonNull::from(&mut value).cast()) };
    if err != AXError::Success || value.is_null() {
        return None;
    }

    // SAFETY: AXWindows renvoie un CFArray possédé ; CFRetained le libérera.
    let array = unsafe { CFRetained::from_raw(NonNull::new_unchecked(value as *mut CFArray)) };
    Some(array)
}
