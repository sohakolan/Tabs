//! Énumération des fenêtres ouvertes.
//!
//! On part de `CGWindowListCopyWindowInfo` (fenêtres visibles, ordre z) puis on
//! filtre pour ne garder que les **vraies fenêtres d'applications du Dock** :
//! - couche 0 (fenêtres normales) ;
//! - application propriétaire de type « regular » (présente dans le Dock) — ce
//!   qui écarte les agents, processus système et utilitaires d'arrière-plan ;
//! - on exclut nos propres fenêtres et les fenêtres trop petites.
//!
//! Le titre est ensuite enrichi via l'API d'Accessibilité (`AXTitle`) : c'est
//! le titre réel de la fenêtre (onglet actif d'un navigateur, piste de Spotify,
//! etc.) et il ne nécessite pas la permission d'Enregistrement de l'écran.

pub mod ax;
pub mod capture;
pub mod focus;

use core::ffi::c_void;
use std::collections::{HashMap, HashSet};

use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication};
use objc2_core_foundation::{CFDictionary, CFNumber, CFString, CFType};
use objc2_core_graphics::{
    kCGWindowBounds, kCGWindowLayer, kCGWindowNumber, kCGWindowOwnerName, kCGWindowOwnerPID,
    CGWindowListCopyWindowInfo, CGWindowListOption,
};

/// Identifiant de fenêtre CoreGraphics.
pub type WindowId = u32;

/// Taille minimale d'une fenêtre retenue (écarte tooltips, vignettes système…).
const MIN_W: f64 = 80.0;
const MIN_H: f64 = 60.0;

/// Une fenêtre applicative candidate au basculement.
#[derive(Debug, Clone)]
pub struct Window {
    pub id: WindowId,
    /// PID de l'application propriétaire.
    pub pid: i32,
    pub app_name: String,
    /// Titre réel de la fenêtre (via Accessibilité), ou chaîne vide.
    pub title: String,
}

/// Liste les vraies fenêtres d'applications du Dock, de l'avant vers l'arrière.
pub fn list_windows() -> Vec<Window> {
    let option =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(array) = CGWindowListCopyWindowInfo(option, 0) else {
        return Vec::new();
    };

    let self_pid = std::process::id() as i32;
    let mut regular_cache: HashMap<i32, bool> = HashMap::new();
    let mut windows = Vec::new();

    for i in 0..array.count() {
        // SAFETY: `i` borné par `count` ; chaque entrée est un CFDictionary.
        let ptr = unsafe { array.value_at_index(i) };
        if ptr.is_null() {
            continue;
        }
        let dict: &CFDictionary = unsafe { &*(ptr as *const CFDictionary) };

        // Fenêtres applicatives normales uniquement (couche 0).
        if dict_i64(dict, unsafe { kCGWindowLayer }).unwrap_or(0) != 0 {
            continue;
        }
        let id = dict_i64(dict, unsafe { kCGWindowNumber }).unwrap_or(0) as WindowId;
        if id == 0 {
            continue;
        }
        let pid = dict_i64(dict, unsafe { kCGWindowOwnerPID }).unwrap_or(0) as i32;
        // Pas nos propres fenêtres (préférences, overlay…).
        if pid == self_pid {
            continue;
        }
        // Écarte les fenêtres trop petites.
        if let Some((w, h)) = window_size(dict) {
            if w < MIN_W || h < MIN_H {
                continue;
            }
        }
        // Uniquement les applications « regular » (celles qui ont une icône au Dock).
        let regular = *regular_cache.entry(pid).or_insert_with(|| is_regular_app(pid));
        if !regular {
            continue;
        }

        let app_name = dict_string(dict, unsafe { kCGWindowOwnerName }).unwrap_or_default();
        windows.push(Window {
            id,
            pid,
            app_name,
            title: String::new(),
        });
    }

    enrich_titles(&mut windows);
    windows
}

/// Renseigne le titre de chaque fenêtre via l'API d'Accessibilité (un appel par
/// application).
fn enrich_titles(windows: &mut [Window]) {
    let mut titles: HashMap<WindowId, String> = HashMap::new();
    let mut seen_pids: HashSet<i32> = HashSet::new();
    for w in windows.iter() {
        if seen_pids.insert(w.pid) {
            for (id, title) in ax::titles_for_pid(w.pid) {
                titles.insert(id, title);
            }
        }
    }
    for w in windows.iter_mut() {
        if let Some(title) = titles.get(&w.id) {
            w.title = title.clone();
        }
    }
}

/// Vrai si l'application est de type « regular » (présente dans le Dock).
fn is_regular_app(pid: i32) -> bool {
    match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        Some(app) => app.activationPolicy() == NSApplicationActivationPolicy::Regular,
        None => false,
    }
}

/// Largeur/hauteur de la fenêtre depuis `kCGWindowBounds`.
fn window_size(dict: &CFDictionary) -> Option<(f64, f64)> {
    let bounds = dict_value(dict, unsafe { kCGWindowBounds })?.downcast_ref::<CFDictionary>()?;
    let w = dict_f64(bounds, &CFString::from_static_str("Width"))?;
    let h = dict_f64(bounds, &CFString::from_static_str("Height"))?;
    Some((w, h))
}

/// Récupère une valeur du dictionnaire par sa clé CFString.
fn dict_value<'a>(dict: &'a CFDictionary, key: &CFString) -> Option<&'a CFType> {
    // SAFETY: `key` est une CFString valide ; la valeur vit avec le dictionnaire.
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

fn dict_f64(dict: &CFDictionary, key: &CFString) -> Option<f64> {
    let n = dict_value(dict, key)?.downcast_ref::<CFNumber>()?;
    n.as_f64().or_else(|| n.as_i64().map(|v| v as f64))
}

fn dict_string(dict: &CFDictionary, key: &CFString) -> Option<String> {
    Some(dict_value(dict, key)?.downcast_ref::<CFString>()?.to_string())
}
