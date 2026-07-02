//! Activation d'une fenêtre : la mettre au premier plan et lui donner le focus.
//!
//! Stratégie :
//! 1. On crée l'`AXUIElement` de l'application propriétaire (`pid`).
//! 2. On parcourt ses fenêtres (`AXWindows`) en faisant correspondre chacune à
//!    son identifiant CoreGraphics via l'API privée `_AXUIElementGetWindow`,
//!    jusqu'à retrouver celle visée.
//! 3. On la lève (`AXRaise`) et on la marque comme principale (`AXMain`).
//! 4. On active l'application propriétaire pour qu'elle passe au premier plan.
//!
//! Tout repose sur la permission d'Accessibilité.

use objc2::rc::Retained;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
use objc2_application_services::AXUIElement;
use objc2_core_foundation::{kCFBooleanFalse, kCFBooleanTrue, CFString, CFType};

use super::ax;
use super::{Window, WindowId};

/// Identifiant de bundle de Finder, toujours lancé mais souvent sans fenêtre.
pub const FINDER_BUNDLE_ID: &str = "com.apple.finder";

/// Application en cours d'exécution pour un `pid` (point d'accès unique aux
/// recherches `NSRunningApplication`, factorisé pour tous les appelants).
pub fn running_app(pid: i32) -> Option<Retained<NSRunningApplication>> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
}

/// Met la fenêtre `window` au premier plan et lui donne le focus.
///
/// Retourne `true` si la fenêtre AX correspondante a été trouvée et levée.
/// L'application est activée dans tous les cas (à condition d'exister).
pub fn activate(window: &Window) -> bool {
    let app = unsafe { AXUIElement::new_application(window.pid) };
    let raised = raise_matching_window(&app, window.id);

    // Amène l'application au premier plan (toutes ses fenêtres).
    if let Some(running) = running_app(window.pid) {
        running.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
    }

    raised
}

/// Demande la fermeture de l'application de PID `pid` (touche « q » : quitte
/// l'application sélectionnée).
pub fn quit_app(pid: i32) {
    if let Some(app) = running_app(pid) {
        app.terminate();
    }
}

/// L'application `pid` est-elle toujours en cours d'exécution (non terminée) ?
/// Sert au guetteur de la touche « q » : l'app ne quitte le sélecteur qu'une
/// fois réellement fermée (le `terminate` est asynchrone : dialogue de
/// confirmation, quit lent…).
pub fn is_running(pid: i32) -> bool {
    running_app(pid).is_some_and(|app| !app.isTerminated())
}

/// L'application `pid` est-elle Finder ? Évite une `String` intermédiaire et
/// centralise la comparaison (utilisée par l'énumération et par la touche « q »).
pub fn is_finder(pid: i32) -> bool {
    running_app(pid)
        .and_then(|app| app.bundleIdentifier())
        .is_some_and(|b| b.to_string() == FINDER_BUNDLE_ID)
}

/// Cherche dans les fenêtres de l'application celle dont l'identifiant CG
/// correspond, puis la lève et la marque comme principale.
fn raise_matching_window(app: &AXUIElement, target: WindowId) -> bool {
    let windows_attr = CFString::from_static_str("AXWindows");
    let Some(windows) = ax::copy_attribute_array(app, &windows_attr) else {
        return false;
    };

    for i in 0..windows.count() {
        // SAFETY: index borné par count ; les entrées sont des AXUIElement.
        let ptr = unsafe { windows.value_at_index(i) } as *const AXUIElement;
        if ptr.is_null() {
            continue;
        }
        let element = unsafe { &*ptr };

        if ax::window_id(element) != Some(target) {
            continue;
        }

        // Fenêtre trouvée : on la lève et on la rend principale.
        let minimized = CFString::from_static_str("AXMinimized");
        let raise = CFString::from_static_str("AXRaise");
        let main = CFString::from_static_str("AXMain");
        unsafe {
            // Dé-minimise la fenêtre si elle était repliée.
            if let Some(no) = kCFBooleanFalse {
                let value: &CFType = &*(no as *const _ as *const CFType);
                element.set_attribute_value(&minimized, value);
            }
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
