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

use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
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
    /// La fenêtre est minimisée (repliée dans le Dock).
    pub minimized: bool,
}

/// Liste les fenêtres d'applications du Dock : d'abord les fenêtres visibles
/// (ordre z, de l'avant vers l'arrière), puis les fenêtres minimisées.
pub fn list_windows() -> Vec<Window> {
    let self_pid = std::process::id() as i32;

    // Applications « regular » (présentes dans le Dock), hors la nôtre.
    let apps = regular_apps(self_pid);
    let regular_pids: HashSet<i32> = apps.iter().map(|(pid, _)| *pid).collect();

    // Fenêtres vues par l'Accessibilité (titre + état minimisé) par application.
    let mut ax_windows: HashMap<i32, Vec<ax::AxWindow>> = HashMap::new();
    for (pid, _) in &apps {
        ax_windows.insert(*pid, ax::windows_for_pid(*pid));
    }

    let mut out = Vec::new();
    let mut seen: HashSet<WindowId> = HashSet::new();

    // 1. Fenêtres visibles, dans l'ordre z de CGWindowList.
    for (id, pid, app_name) in onscreen_entries(&regular_pids, self_pid) {
        let title = ax_windows
            .get(&pid)
            .and_then(|v| v.iter().find(|w| w.id == id))
            .map(|w| w.title.clone())
            .unwrap_or_default();
        seen.insert(id);
        out.push(Window {
            id,
            pid,
            app_name,
            title,
            minimized: false,
        });
    }

    // 2. Fenêtres minimisées (absentes de la liste à l'écran).
    for (pid, app_name) in &apps {
        if let Some(windows) = ax_windows.get(pid) {
            for w in windows {
                if w.minimized && seen.insert(w.id) {
                    out.push(Window {
                        id: w.id,
                        pid: *pid,
                        app_name: app_name.clone(),
                        title: w.title.clone(),
                        minimized: true,
                    });
                }
            }
        }
    }

    // 3. Applications du Dock sans aucune fenêtre (ex. Aperçu ouvert mais sans
    //    document). Comportement Cmd-Tab : on les liste quand même ; les activer
    //    les ramène au premier plan. Id synthétique (dérivé du pid, hors plage
    //    des vrais numéros CoreGraphics) pour éviter toute collision.
    let represented: HashSet<i32> = out.iter().map(|w| w.pid).collect();
    for (pid, app_name) in &apps {
        if represented.contains(pid) {
            continue;
        }
        // Finder est toujours lancé mais le plus souvent sans fenêtre : on ne le
        // montre dans le sélecteur que s'il a une vraie fenêtre (sinon il
        // l'encombre en permanence).
        if focus::bundle_id(*pid).as_deref() == Some("com.apple.finder") {
            continue;
        }
        out.push(Window {
            id: app_only_id(*pid),
            pid: *pid,
            app_name: app_name.clone(),
            title: String::new(),
            minimized: false,
        });
    }

    out
}

/// Id de fenêtre synthétique pour une application sans fenêtre, dérivé du pid.
/// Placé tout en haut de la plage `u32`, là où les vrais numéros de fenêtre
/// CoreGraphics (séquentiels depuis de petites valeurs) n'arrivent jamais.
fn app_only_id(pid: i32) -> WindowId {
    0xF000_0000 | (pid as u32 & 0x0FFF_FFFF)
}

/// Réordonne `windows` selon l'historique d'usage `mru` (plus récent d'abord).
///
/// macOS regroupe l'ordre z par application dès qu'une fenêtre est activée, ce
/// qui détruit l'ordre MRU par fenêtre. On conserve donc notre propre ordre :
/// 1. les fenêtres connues du `mru`, dans cet ordre ;
/// 2. les nouvelles fenêtres, dans leur ordre z (telles que listées) ;
/// 3. la fenêtre réellement au premier plan (`windows[0]`, tête de l'ordre z)
///    est forcée à l'index 0, car c'est elle que l'on quitte au prochain Tab.
///
/// `mru` est mis à jour pour refléter le nouvel ordre (et purgé des fenêtres
/// fermées).
pub fn order_by_mru(windows: Vec<Window>, mru: &mut Vec<WindowId>) -> Vec<Window> {
    let present: HashSet<WindowId> = windows.iter().map(|w| w.id).collect();
    let front = windows.first().map(|w| w.id);

    // 1. MRU connu, restreint aux fenêtres encore présentes.
    let mut order: Vec<WindowId> = mru.iter().copied().filter(|id| present.contains(id)).collect();
    // 2. Nouvelles fenêtres, dans leur ordre z.
    let known: HashSet<WindowId> = order.iter().copied().collect();
    order.extend(windows.iter().map(|w| w.id).filter(|id| !known.contains(id)));
    // 3. Fenêtre au premier plan en tête.
    if let Some(front) = front {
        if let Some(pos) = order.iter().position(|&id| id == front) {
            let id = order.remove(pos);
            order.insert(0, id);
        }
    }

    *mru = order.clone();

    // Réindexe les fenêtres selon `order`.
    let mut by_id: HashMap<WindowId, Window> = windows.into_iter().map(|w| (w.id, w)).collect();
    order.iter().filter_map(|id| by_id.remove(id)).collect()
}

/// Applications « regular » en cours d'exécution (icône au Dock), hors la nôtre.
fn regular_apps(self_pid: i32) -> Vec<(i32, String)> {
    let running = NSWorkspace::sharedWorkspace().runningApplications();
    let mut apps = Vec::new();
    for i in 0..running.count() {
        let app = running.objectAtIndex(i);
        if app.activationPolicy() != NSApplicationActivationPolicy::Regular {
            continue;
        }
        let pid = app.processIdentifier();
        if pid == self_pid {
            continue;
        }
        let name = app
            .localizedName()
            .map(|n| n.to_string())
            .unwrap_or_default();
        apps.push((pid, name));
    }
    apps
}

/// Identifiants des fenêtres visibles, dans l'ordre z, filtrées sur les apps du
/// Dock, hors la nôtre, et de taille suffisante.
fn onscreen_entries(regular_pids: &HashSet<i32>, self_pid: i32) -> Vec<(WindowId, i32, String)> {
    let option =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(array) = CGWindowListCopyWindowInfo(option, 0) else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    for i in 0..array.count() {
        // SAFETY: `i` borné par `count` ; chaque entrée est un CFDictionary.
        let ptr = unsafe { array.value_at_index(i) };
        if ptr.is_null() {
            continue;
        }
        let dict: &CFDictionary = unsafe { &*(ptr as *const CFDictionary) };

        if dict_i64(dict, unsafe { kCGWindowLayer }).unwrap_or(0) != 0 {
            continue;
        }
        let id = dict_i64(dict, unsafe { kCGWindowNumber }).unwrap_or(0) as WindowId;
        if id == 0 {
            continue;
        }
        let pid = dict_i64(dict, unsafe { kCGWindowOwnerPID }).unwrap_or(0) as i32;
        if pid == self_pid || !regular_pids.contains(&pid) {
            continue;
        }
        if let Some((w, h)) = window_size(dict) {
            if w < MIN_W || h < MIN_H {
                continue;
            }
        }
        let name = dict_string(dict, unsafe { kCGWindowOwnerName }).unwrap_or_default();
        entries.push((id, pid, name));
    }
    entries
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Construit une fenêtre minimale pour les tests d'ordre.
    fn win(id: WindowId) -> Window {
        Window {
            id,
            pid: id as i32,
            app_name: String::new(),
            title: String::new(),
            minimized: false,
        }
    }

    fn ids(windows: &[Window]) -> Vec<WindowId> {
        windows.iter().map(|w| w.id).collect()
    }

    #[test]
    fn mru_vide_conserve_lordre_z() {
        let mut mru = Vec::new();
        let out = order_by_mru(vec![win(10), win(20), win(30)], &mut mru);
        assert_eq!(ids(&out), [10, 20, 30]);
        assert_eq!(mru, [10, 20, 30]);
    }

    #[test]
    fn front_force_a_lindex_0_sans_regrouper() {
        // MRU entrelacé (kitty, firefox, kitty, firefox), puis macOS regroupe
        // l'ordre z par app et met firefox au premier plan.
        let mut mru = vec![1, 2, 3, 4];
        // list_windows renverrait l'ordre z groupé : [firefox 2, firefox 4, kitty 1, kitty 3].
        let out = order_by_mru(vec![win(2), win(4), win(1), win(3)], &mut mru);
        // On garde l'ordre MRU (entrelacé), juste la fenêtre de tête (2) en index 0.
        assert_eq!(ids(&out), [2, 1, 3, 4]);
        assert_eq!(mru, [2, 1, 3, 4]);
    }

    #[test]
    fn alt_tab_deux_fois_revient_au_depart() {
        let mut mru = Vec::new();
        // Démarrage : A=1 au premier plan, B=2 ensuite.
        let out = order_by_mru(vec![win(1), win(2), win(3)], &mut mru);
        assert_eq!(ids(&out), [1, 2, 3]); // index 1 = B (2)

        // On bascule sur B : macOS le met au premier plan (z-order[0] = 2).
        let out = order_by_mru(vec![win(2), win(1), win(3)], &mut mru);
        assert_eq!(ids(&out), [2, 1, 3]); // index 1 = A (1) → un Tab nous y ramène

        // On bascule de nouveau sur A : retour à l'état de départ.
        let out = order_by_mru(vec![win(1), win(2), win(3)], &mut mru);
        assert_eq!(ids(&out), [1, 2, 3]);
    }

    #[test]
    fn nouvelles_fenetres_ajoutees_fermees_purgees() {
        let mut mru = vec![1, 2, 3];
        // 3 fermée, 4 ouverte ; 2 toujours au premier plan (z-order[0]).
        let out = order_by_mru(vec![win(2), win(1), win(4)], &mut mru);
        assert_eq!(ids(&out), [2, 1, 4]);
        assert_eq!(mru, [2, 1, 4]); // 3 purgée, 4 ajoutée
    }
}
