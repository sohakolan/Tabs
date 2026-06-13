//! Énumération des fenêtres ouvertes.
//!
//! M2 s'appuie sur `CGWindowListCopyWindowInfo`, qui fournit la liste des
//! fenêtres visibles dans l'ordre z (de l'avant vers l'arrière) avec, pour
//! chacune, son identifiant, le PID et le nom de l'application propriétaire, et
//! éventuellement son titre.
//!
//! Notes :
//! - Le nom de l'application (`kCGWindowOwnerName`) est toujours disponible ;
//!   le titre de la fenêtre (`kCGWindowName`) nécessite la permission
//!   d'Enregistrement de l'écran, sinon il est vide.
//! - On filtre sur la couche 0 pour ne garder que les fenêtres applicatives
//!   normales (on écarte le Dock, la barre des menus, les overlays, etc.).
//!
//! L'élément d'accessibilité (`AXUIElement`) nécessaire pour *activer* une
//! fenêtre sera ajouté au modèle en M4.

pub mod focus;

use core::ffi::c_void;

use objc2_core_foundation::{CFDictionary, CFNumber, CFString, CFType};
use objc2_core_graphics::{
    kCGWindowLayer, kCGWindowName, kCGWindowNumber, kCGWindowOwnerName, kCGWindowOwnerPID,
    CGWindowListCopyWindowInfo, CGWindowListOption,
};

/// Identifiant de fenêtre CoreGraphics.
pub type WindowId = u32;

/// Une fenêtre applicative candidate au basculement.
#[derive(Debug, Clone)]
pub struct Window {
    pub id: WindowId,
    /// PID de l'application propriétaire. Utilisé en M4 pour retrouver
    /// l'`AXUIElement` de la fenêtre et l'activer.
    #[allow(dead_code)]
    pub pid: i32,
    pub app_name: String,
    /// Titre de la fenêtre, ou chaîne vide si indisponible (permission
    /// d'Enregistrement de l'écran non accordée).
    pub title: String,
}

/// Liste les fenêtres applicatives visibles à l'écran, de l'avant vers
/// l'arrière (ordre z), en ne gardant que les fenêtres normales (couche 0).
pub fn list_windows() -> Vec<Window> {
    let option =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(array) = CGWindowListCopyWindowInfo(option, 0) else {
        return Vec::new();
    };

    let count = array.count();
    let mut windows = Vec::new();
    for i in 0..count {
        // SAFETY: `i` est borné par `count` ; chaque entrée de la liste
        // CGWindow est un CFDictionary valide pour la durée de l'itération.
        let ptr = unsafe { array.value_at_index(i) };
        if ptr.is_null() {
            continue;
        }
        let dict: &CFDictionary = unsafe { &*(ptr as *const CFDictionary) };

        // On ne garde que les fenêtres applicatives normales.
        if dict_i64(dict, unsafe { kCGWindowLayer }).unwrap_or(0) != 0 {
            continue;
        }

        let id = dict_i64(dict, unsafe { kCGWindowNumber }).unwrap_or(0) as WindowId;
        if id == 0 {
            continue;
        }
        let pid = dict_i64(dict, unsafe { kCGWindowOwnerPID }).unwrap_or(0) as i32;
        let app_name = dict_string(dict, unsafe { kCGWindowOwnerName }).unwrap_or_default();
        let title = dict_string(dict, unsafe { kCGWindowName }).unwrap_or_default();

        windows.push(Window {
            id,
            pid,
            app_name,
            title,
        });
    }
    windows
}

/// Récupère une valeur du dictionnaire CGWindow par sa clé CFString.
fn dict_value<'a>(dict: &'a CFDictionary, key: &CFString) -> Option<&'a CFType> {
    // SAFETY: `key` est une CFString valide ; la valeur appartient au
    // dictionnaire et vit aussi longtemps que lui.
    let value = unsafe { dict.value(key as *const CFString as *const c_void) };
    if value.is_null() {
        None
    } else {
        Some(unsafe { &*(value as *const CFType) })
    }
}

fn dict_i64(dict: &CFDictionary, key: &CFString) -> Option<i64> {
    dict_value(dict, key)?.downcast_ref::<CFNumber>()?.as_i64()
}

fn dict_string(dict: &CFDictionary, key: &CFString) -> Option<String> {
    Some(dict_value(dict, key)?.downcast_ref::<CFString>()?.to_string())
}
