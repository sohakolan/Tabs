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

    // Fenêtres visibles d'abord (ordre z) : leur ensemble d'identifiants borne la
    // lecture des titres AX, qu'on ne fait que pour les fenêtres réellement
    // affichées (visibles ou minimisées).
    let onscreen = onscreen_entries(&regular_pids, self_pid);
    let onscreen_ids: HashSet<WindowId> = onscreen.iter().map(|(id, _, _)| *id).collect();

    // Fenêtres vues par l'Accessibilité (titre + état minimisé) par application.
    let mut ax_windows: HashMap<i32, Vec<ax::AxWindow>> = HashMap::with_capacity(apps.len());
    for (pid, _) in &apps {
        ax_windows.insert(*pid, ax::windows_for_pid(*pid, &onscreen_ids));
    }

    let mut out = Vec::with_capacity(onscreen.len() + apps.len());
    let mut seen: HashSet<WindowId> = HashSet::with_capacity(onscreen.len());
    // Pids ayant déjà produit une fenêtre (phases 1 et 2) : sert à ne pas
    // re-lister une app en entrée « sans fenêtre » à la phase 3.
    let mut seen_pids: HashSet<i32> = HashSet::with_capacity(apps.len());

    // 1. Fenêtres visibles, dans l'ordre z de CGWindowList.
    for (id, pid, app_name) in onscreen {
        let title = ax_windows
            .get(&pid)
            .and_then(|v| v.iter().find(|w| w.id == id))
            .map(|w| w.title.clone())
            .unwrap_or_default();
        seen.insert(id);
        seen_pids.insert(pid);
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
                    seen_pids.insert(*pid);
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
    for (pid, app_name) in &apps {
        if seen_pids.contains(pid) {
            continue;
        }
        // Finder est toujours lancé mais le plus souvent sans fenêtre : on ne le
        // montre dans le sélecteur que s'il a une vraie fenêtre (sinon il
        // l'encombre en permanence).
        if focus::is_finder(*pid) {
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
pub(crate) fn app_only_id(pid: i32) -> WindowId {
    0xF000_0000 | (pid as u32 & 0x0FFF_FFFF)
}

/// Réordonne `windows` selon l'historique d'usage `mru` (plus récent d'abord).
///
/// macOS regroupe l'ordre z par application dès qu'une fenêtre est activée, ce
/// qui détruit l'ordre MRU par fenêtre. On conserve donc notre propre ordre :
/// 1. les fenêtres connues du `mru`, dans cet ordre ;
/// 2. les nouvelles fenêtres, dans leur ordre z (telles que listées) ;
/// 3. la **fenêtre courante** forcée à l'index 0, car c'est elle que l'on quitte
///    au prochain Tab.
///
/// La fenêtre courante est, par défaut, la tête de l'ordre z (`windows[0]`) :
/// cela reflète un changement de fenêtre fait **hors de Tabs** (clic souris,
/// Cmd-Tab natif…). Mais cet ordre z est parfois en retard ou imprécis juste
/// après une validation : quand `committed` désigne la fenêtre qu'on vient
/// d'activer et que la tête de l'ordre z n'est qu'une **autre fenêtre de la même
/// application** (artefact d'`AXRaise`/d'activation d'app), c'est `committed`
/// qui fait foi. Un vrai changement vers une autre application (pid différent)
/// reste, lui, honoré. Sans ce garde-fou, l'ordre se « retriait » de travers dès
/// que l'instantané z ne reflétait pas la fenêtre réellement validée.
///
/// `mru` est mis à jour pour refléter le nouvel ordre (et purgé des fenêtres
/// fermées).
pub fn order_by_mru(
    windows: Vec<Window>,
    mru: &mut Vec<WindowId>,
    committed: Option<WindowId>,
) -> Vec<Window> {
    let present: HashSet<WindowId> = windows.iter().map(|w| w.id).collect();

    // Fenêtre à forcer en index 0 (la « courante »). On suit la tête de l'ordre
    // z, sauf si elle n'est qu'une sœur (même app) de la fenêtre tout juste
    // validée : dans ce cas l'activation a bien visé `committed`, mais
    // l'instantané z fait remonter une autre fenêtre de l'app — `committed` prime.
    let z_front = windows.first();
    let front = match committed {
        Some(c)
            if present.contains(&c)
                && z_front.is_some_and(|f| f.id != c && Some(f.pid) == pid_of(&windows, c)) =>
        {
            Some(c)
        }
        _ => z_front.map(|w| w.id),
    };

    // 1. MRU connu, restreint aux fenêtres encore présentes.
    let mut order: Vec<WindowId> = mru.iter().copied().filter(|id| present.contains(id)).collect();
    // 2. Nouvelles fenêtres, dans leur ordre z.
    let known: HashSet<WindowId> = order.iter().copied().collect();
    order.extend(windows.iter().map(|w| w.id).filter(|id| !known.contains(id)));
    // 3. Fenêtre courante en tête.
    if let Some(front) = front {
        if let Some(pos) = order.iter().position(|&id| id == front) {
            let id = order.remove(pos);
            order.insert(0, id);
        }
    }

    // Réindexe les fenêtres selon `order` (en l'empruntant), puis transfère
    // `order` dans `mru` sans clone superflu.
    let mut by_id: HashMap<WindowId, Window> = windows.into_iter().map(|w| (w.id, w)).collect();
    let ordered = order.iter().filter_map(|id| by_id.remove(id)).collect();
    *mru = order;
    ordered
}

/// Pid de la fenêtre `id` dans `windows`, le cas échéant.
fn pid_of(windows: &[Window], id: WindowId) -> Option<i32> {
    windows.iter().find(|w| w.id == id).map(|w| w.pid)
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
        let out = order_by_mru(vec![win(10), win(20), win(30)], &mut mru, None);
        assert_eq!(ids(&out), [10, 20, 30]);
        assert_eq!(mru, [10, 20, 30]);
    }

    #[test]
    fn front_force_a_lindex_0_sans_regrouper() {
        // MRU entrelacé (kitty, firefox, kitty, firefox), puis macOS regroupe
        // l'ordre z par app et met firefox au premier plan.
        let mut mru = vec![1, 2, 3, 4];
        // list_windows renverrait l'ordre z groupé : [firefox 2, firefox 4, kitty 1, kitty 3].
        let out = order_by_mru(vec![win(2), win(4), win(1), win(3)], &mut mru, None);
        // On garde l'ordre MRU (entrelacé), juste la fenêtre de tête (2) en index 0.
        assert_eq!(ids(&out), [2, 1, 3, 4]);
        assert_eq!(mru, [2, 1, 3, 4]);
    }

    #[test]
    fn alt_tab_deux_fois_revient_au_depart() {
        let mut mru = Vec::new();
        // Démarrage : A=1 au premier plan, B=2 ensuite.
        let out = order_by_mru(vec![win(1), win(2), win(3)], &mut mru, None);
        assert_eq!(ids(&out), [1, 2, 3]); // index 1 = B (2)

        // On bascule sur B : macOS le met au premier plan (z-order[0] = 2).
        let out = order_by_mru(vec![win(2), win(1), win(3)], &mut mru, Some(2));
        assert_eq!(ids(&out), [2, 1, 3]); // index 1 = A (1) → un Tab nous y ramène

        // On bascule de nouveau sur A : retour à l'état de départ.
        let out = order_by_mru(vec![win(1), win(2), win(3)], &mut mru, Some(1));
        assert_eq!(ids(&out), [1, 2, 3]);
    }

    #[test]
    fn nouvelles_fenetres_ajoutees_fermees_purgees() {
        let mut mru = vec![1, 2, 3];
        // 3 fermée, 4 ouverte ; 2 toujours au premier plan (z-order[0]).
        let out = order_by_mru(vec![win(2), win(1), win(4)], &mut mru, Some(2));
        assert_eq!(ids(&out), [2, 1, 4]);
        assert_eq!(mru, [2, 1, 4]); // 3 purgée, 4 ajoutée
    }

    // ---- Simulation du gestionnaire de fenêtres macOS -----------------------
    //
    // L'ordre affiché par `order_by_mru` dépend entièrement de l'ordre z que
    // renvoie `CGWindowListCopyWindowInfo`. Pour valider la logique de tri sans
    // macOS, on modélise le comportement réel du système :
    //   - un ordre z (avant → arrière) des fenêtres visibles ;
    //   - à l'activation d'une fenêtre, macOS **regroupe son application** :
    //     la fenêtre activée passe en tête, ses sœurs (même app) la suivent dans
    //     leur ordre relatif, puis le reste.
    // L'identifiant encode l'app dans ses dizaines : 11,12 → app 1 ; 21,22 → app 2.

    /// Modèle minimal du gestionnaire de fenêtres macOS.
    struct Macos {
        /// Ordre z des fenêtres visibles, de l'avant vers l'arrière.
        z: Vec<WindowId>,
    }

    impl Macos {
        fn new(z: &[WindowId]) -> Self {
            Self { z: z.to_vec() }
        }

        /// Application propriétaire d'une fenêtre (encodée dans les dizaines).
        fn app(id: WindowId) -> WindowId {
            id / 10
        }

        /// Active `w` en levant **la fenêtre précise** (cas nominal : `AXRaise`
        /// réussit). `w` passe en tête ; ses sœurs suivent dans leur ordre
        /// relatif antérieur ; le reste passe en dessous.
        fn raise_window(&mut self, w: WindowId) {
            let app = Self::app(w);
            let mut siblings = Vec::new();
            let mut others = Vec::new();
            for &id in &self.z {
                if id == w {
                    continue;
                }
                if Self::app(id) == app {
                    siblings.push(id);
                } else {
                    others.push(id);
                }
            }
            let mut z = Vec::with_capacity(self.z.len());
            z.push(w);
            z.extend(siblings);
            z.extend(others);
            self.z = z;
        }

        /// Active `w` en n'amenant que **l'application** au premier plan, sans
        /// lever la fenêtre précise (cas dégradé : `AXRaise` échoue / fenêtre sur
        /// un autre Space / instantané z périmé). Les fenêtres de l'app gardent
        /// leur ordre interne : c'est celle **déjà** en tête de l'app qui reste
        /// devant, pas `w`.
        fn activate_app_only(&mut self, w: WindowId) {
            let app = Self::app(w);
            let mut app_wins = Vec::new();
            let mut others = Vec::new();
            for &id in &self.z {
                if Self::app(id) == app {
                    app_wins.push(id);
                } else {
                    others.push(id);
                }
            }
            app_wins.extend(others);
            self.z = app_wins;
        }

        /// Ce que verrait `list_windows` : les fenêtres dans l'ordre z, chaque
        /// fenêtre portant le pid de son application (regroupement par app).
        fn list(&self) -> Vec<Window> {
            self.z
                .iter()
                .map(|&id| Window {
                    id,
                    pid: Self::app(id) as i32,
                    app_name: String::new(),
                    title: String::new(),
                    minimized: false,
                })
                .collect()
        }
    }

    /// Référence : pile MRU idéale (déplace `w` en tête, dédupliqué).
    fn move_to_front(stack: &mut Vec<WindowId>, w: WindowId) {
        stack.retain(|&id| id != w);
        stack.insert(0, w);
    }

    /// Générateur pseudo-aléatoire déterministe (pas de dépendance externe).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn pick(&mut self, n: usize) -> usize {
            (self.next() as usize) % n
        }
    }

    /// Cas nominal : tant que macOS lève bien la fenêtre activée, `order_by_mru`
    /// maintient une pile MRU **parfaite** malgré le regroupement par app —
    /// vérifié sur des milliers de séquences d'activation aléatoires, avec
    /// plusieurs apps multi-fenêtres.
    #[test]
    fn simulation_mru_coherent_malgre_le_regroupement() {
        let initial = [11, 12, 21, 22, 23, 31, 41, 42];
        let mut macos = Macos::new(&initial);
        let mut mru = Vec::new();

        // Premier open : établit l'ordre de référence.
        let display = order_by_mru(macos.list(), &mut mru, None);
        let mut expected = ids(&display);
        let mut rng = Lcg(0xC0FF_EE12_3456_789A);

        for step in 0..5000 {
            // L'utilisateur valide une fenêtre quelconque parmi celles affichées.
            let target = expected[rng.pick(expected.len())];
            macos.raise_window(target);
            move_to_front(&mut expected, target);

            // Réouverture : Tabs recalcule l'ordre depuis l'ordre z courant, en
            // signalant la fenêtre validée.
            let display = ids(&order_by_mru(macos.list(), &mut mru, Some(target)));
            assert_eq!(display, expected, "étape {step}: ordre MRU incohérent");
            // La fenêtre validée doit être en tête (index 0 = fenêtre courante).
            assert_eq!(display[0], target, "étape {step}: tête != fenêtre activée");
            // L'historique persisté reflète l'ordre affiché.
            assert_eq!(mru, expected, "étape {step}: mru désynchronisé");
        }
    }

    /// Bascule immédiate (un seul Tab) répétée deux fois : on doit revenir à la
    /// fenêtre de départ **et** au même ordre, y compris avec des apps
    /// multi-fenêtres regroupées par macOS.
    #[test]
    fn bascule_immediate_est_un_aller_retour_meme_avec_regroupement() {
        let initial = [11, 12, 21, 31, 32];
        let mut macos = Macos::new(&initial);
        let mut mru = Vec::new();
        let depart = ids(&order_by_mru(macos.list(), &mut mru, None));

        // Premier aller : on bascule vers la fenêtre à l'index 1 (un Tab + relâche).
        let suivant = depart[1];
        macos.raise_window(suivant);
        let apres = ids(&order_by_mru(macos.list(), &mut mru, Some(suivant)));
        assert_eq!(apres[0], suivant);
        assert_eq!(apres[1], depart[0]); // l'ancienne fenêtre courante en index 1

        // Retour : on rebascule vers l'index 1 → on doit retrouver l'état initial.
        let retour = apres[1];
        macos.raise_window(retour);
        let final_ = ids(&order_by_mru(macos.list(), &mut mru, Some(retour)));
        assert_eq!(final_, depart, "la bascule immédiate n'est pas un aller-retour");
    }

    /// Non-régression du bug « parfois il se retrie pas correctement ».
    ///
    /// Quand l'activation ne lève pas la fenêtre précise (app multi-fenêtres dont
    /// `AXRaise` échoue, fenêtre sur un autre Space, ou instantané z lu avant que
    /// le serveur de fenêtres se stabilise), l'ordre z suivant a **une autre
    /// fenêtre de la même app** en tête. Avant le correctif, `order_by_mru`
    /// promouvait cette mauvaise fenêtre. Désormais, la fenêtre réellement validée
    /// (`committed`) fait foi.
    #[test]
    fn fenetre_validee_prime_si_le_zorder_remonte_une_soeur() {
        // app 2 a trois fenêtres ; 21 est déjà en tête de son app.
        let mut macos = Macos::new(&[11, 21, 22, 23, 31]);
        let mut mru = Vec::new();
        order_by_mru(macos.list(), &mut mru, None);

        // L'utilisateur valide 23 (fenêtre d'arrière-plan de l'app 2), mais
        // l'activation n'amène que l'application — 21 reste en tête de l'app.
        let valide = 23;
        macos.activate_app_only(valide);

        let display = ids(&order_by_mru(macos.list(), &mut mru, Some(valide)));

        // La fenêtre réellement validée est en tête, malgré l'ordre z trompeur.
        assert_eq!(
            display[0], valide,
            "la fenêtre validée doit primer sur une sœur remontée par l'ordre z"
        );
    }

    /// Garde-fou inverse : un **vrai** changement vers une autre application
    /// (faite hors de Tabs, p. ex. au clic) doit rester honoré — la fenêtre
    /// validée précédemment ne doit pas masquer la nouvelle fenêtre courante.
    #[test]
    fn changement_externe_vers_une_autre_app_reste_honore() {
        // On a validé 21 (app 2) au cycle précédent ; mru en témoigne.
        let mut macos = Macos::new(&[21, 11, 31]);
        let mut mru = Vec::new();
        order_by_mru(macos.list(), &mut mru, Some(21));

        // L'utilisateur clique (hors Tabs) sur 11 (app 1) : l'ordre z le remonte.
        macos.raise_window(11);

        // On rouvre en signalant encore 21 comme dernière validation Tabs : comme
        // la tête de l'ordre z (11) appartient à une AUTRE app, elle l'emporte.
        let display = ids(&order_by_mru(macos.list(), &mut mru, Some(21)));
        assert_eq!(
            display[0], 11,
            "un changement vers une autre app doit rester reflété en tête"
        );
    }
}
