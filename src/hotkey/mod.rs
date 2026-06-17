//! Tap d'évènements clavier (CGEventTap) déclenchant le sélecteur.
//!
//! On installe un tap au niveau de la session qui observe les pressions de
//! touches et les changements de modificateurs. Les évènements pertinents sont
//! traduits en [`Input`] et passés à la [`Switcher`] ; les touches consommées
//! par le sélecteur (Tab, Échap) sont « avalées » pour ne pas atteindre
//! l'application active.
//!
//! Le tap requiert la permission d'Accessibilité (cf. [`crate::permissions`]).

mod state;

pub use state::{Action, Input, Switcher};

use crate::config::TriggerModifier;
use crate::ui::{DisplayMode, Overlay};
use crate::windows::{self, Window, WindowId};

use core::ffi::c_void;
use core::ptr::{self, NonNull};
use std::cell::RefCell;

use objc2_app_kit::NSEvent;
use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRetained, CFRunLoop};
use objc2_foundation::MainThreadMarker;
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType,
};

// Tab et Échap se repèrent par leur keycode physique (identique sur tous les
// agencements de clavier). En revanche les lettres (q, m) et la virgule
// dépendent de l'agencement (AZERTY ≠ QWERTY) : on les repère par le caractère
// réellement tapé, pas par la position physique de la touche.
/// Code de touche virtuelle macOS pour Tab.
const KEYCODE_TAB: i64 = 0x30;
/// Code de touche virtuelle macOS pour Échap.
const KEYCODE_ESCAPE: i64 = 0x35;

/// Masque des évènements écoutés : keyDown (10) | keyUp (11) | flagsChanged (12).
const EVENT_MASK: u64 = (1 << 10) | (1 << 11) | (1 << 12);

/// État partagé entre la run loop et le callback (tout sur le thread principal).
struct TapState {
    switcher: Switcher,
    /// Instantané des fenêtres pris à l'ouverture du sélecteur ; l'index
    /// sélectionné par la [`Switcher`] désigne une entrée de ce vecteur.
    windows: Vec<Window>,
    /// Ordre d'usage récent des fenêtres (id, plus récent d'abord). macOS
    /// regroupe l'ordre z par application dès qu'on active une fenêtre ; on
    /// maintient donc notre propre historique pour conserver un vrai ordre
    /// MRU par fenêtre (cf. [`windows::order_by_mru`]).
    mru: Vec<WindowId>,
    /// Mode d'affichage courant des cellules.
    mode: DisplayMode,
    /// Modificateur qui déclenche/maintient le cycle (Option par défaut, ou
    /// Command si le remplacement de Cmd-Tab est activé).
    trigger_flag: CGEventFlags,
    /// L'overlay affiché à l'écran (créé à l'installation, sur le thread
    /// principal).
    overlay: Option<Overlay>,
    /// Autorise la touche « q » à fermer l'app sélectionnée (désactivé par
    /// défaut).
    quit_with_q: bool,
    /// Autorise la touche « w » à fermer la fenêtre sélectionnée (activé par
    /// défaut).
    close_with_w: bool,
    /// Rappel pour ouvrir la fenêtre de préférences (touche `,`).
    on_open_prefs: Box<dyn Fn()>,
    /// Conservé pour pouvoir réactiver le tap s'il est désactivé par le système.
    port: Option<CFRetained<CFMachPort>>,
}

thread_local! {
    static STATE: RefCell<TapState> = RefCell::new(TapState {
        switcher: Switcher::new(),
        windows: Vec::new(),
        mru: Vec::new(),
        mode: DisplayMode::Thumbnails,
        trigger_flag: CGEventFlags::MaskAlternate,
        quit_with_q: false,
        close_with_w: true,
        overlay: None,
        on_open_prefs: Box::new(|| {}),
        port: None,
    });
}

/// Prépare le sélecteur (overlay + rappels) puis tente d'installer le tap
/// clavier. À appeler une fois depuis le thread principal, avant
/// `NSApplication::run`. Retourne `false` si le tap n'a pas pu être créé
/// (typiquement permission d'Accessibilité manquante) — dans ce cas l'overlay
/// est tout de même prêt et [`ensure_tap_installed`] activera le tap dès que la
/// permission sera accordée, sans relancer l'application.
pub fn install(initial_mode: DisplayMode, on_open_prefs: Box<dyn Fn()>) -> bool {
    // L'overlay vit sur le thread principal (objets AppKit non-Send). On lui
    // confie les rappels souris, branchés sur la même machine à états.
    let mtm = MainThreadMarker::new().expect("install doit s'exécuter sur le thread principal");
    let overlay = Overlay::new(mtm, Box::new(mouse_hover), Box::new(mouse_click));
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.overlay = Some(overlay);
        st.mode = initial_mode;
        st.on_open_prefs = on_open_prefs;
    });

    if ensure_tap_installed() {
        true
    } else {
        eprintln!(
            "[Tabs] Tap clavier non installé (permission d'Accessibilité manquante). \
             Il s'activera automatiquement dès qu'elle sera accordée."
        );
        false
    }
}

/// (Re)crée le tap clavier sur la run loop courante si nécessaire.
///
/// Idempotent : si le tap est déjà actif, retourne `true` sans rien faire.
/// `CGEventTapCreate` échoue tant que l'Accessibilité n'est pas accordée ; on
/// peut donc rappeler cette fonction après l'octroi de la permission (p. ex. au
/// retour des Réglages Système) pour activer Option-Tab **sans relancer** Tabs.
/// Retourne `true` si le tap est actif à la sortie.
pub fn ensure_tap_installed() -> bool {
    if STATE.with(|s| s.borrow().port.is_some()) {
        return true;
    }

    // SAFETY: signature conforme à CGEventTapCallBack ; `user_info` non utilisé
    // (l'état est dans un thread_local). `CGEventTapCreate` est déprécié au
    // profit d'une méthode non encore exposée par objc2 : on l'utilise donc tel
    // quel, en confinant l'avertissement.
    #[allow(deprecated)]
    let port = unsafe {
        objc2_core_graphics::CGEventTapCreate(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            EVENT_MASK,
            Some(tap_callback),
            ptr::null_mut(),
        )
    };

    // Échec silencieux : l'Accessibilité n'est pas (encore) accordée. On
    // réessaiera plus tard ; inutile de polluer les logs à chaque tentative.
    let Some(port) = port else {
        return false;
    };

    let Some(source) = CFMachPort::new_run_loop_source(None, Some(&port), 0) else {
        eprintln!("[Tabs] Échec de création de la run loop source du tap.");
        return false;
    };

    let Some(run_loop) = CFRunLoop::current() else {
        eprintln!("[Tabs] Run loop courante introuvable.");
        return false;
    };
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });

    CGEvent::tap_enable(&port, true);

    STATE.with(|s| s.borrow_mut().port = Some(port));
    // La source doit rester vivante aussi longtemps que la run loop tourne,
    // c'est-à-dire toute la durée de vie de l'application.
    core::mem::forget(source);

    println!("[Tabs] Tap clavier installé. Maintiens Option et appuie sur Tab.");
    true
}

/// Callback C invoqué par la run loop pour chaque évènement capté.
unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: NonNull<CGEvent>,
    _user_info: *mut c_void,
) -> *mut CGEvent {
    let passthrough = event.as_ptr();
    let swallow = ptr::null_mut();
    // SAFETY: la run loop nous fournit un évènement valide.
    let ev = unsafe { event.as_ref() };

    // Le système peut désactiver le tap (timeout/saisie utilisateur) : on le
    // réactive immédiatement.
    if event_type == CGEventType::TapDisabledByTimeout
        || event_type == CGEventType::TapDisabledByUserInput
    {
        STATE.with(|s| {
            if let Some(port) = &s.borrow().port {
                CGEvent::tap_enable(port, true);
            }
        });
        return passthrough;
    }

    let flags = CGEvent::flags(Some(ev));
    let trigger_flag = STATE.with(|s| s.borrow().trigger_flag);
    let trigger_held = flags.contains(trigger_flag);
    let shift_held = flags.contains(CGEventFlags::MaskShift);

    match event_type {
        CGEventType::KeyDown => {
            // Aucun raccourci possible hors de ces deux états : on évite le
            // coût de `classify` (round-trip NSEvent) sur le flux ambiant.
            if !trigger_held && !is_active() {
                return passthrough;
            }
            match classify(ev) {
                Some(Shortcut::Tab) if trigger_held => on_tab(shift_held),
                Some(Shortcut::Escape) if is_active() => dispatch(Input::Escape),
                Some(Shortcut::CycleMode) if is_active() => cycle_mode(),
                Some(Shortcut::Quit) if is_active() => quit_selected_app(),
                Some(Shortcut::CloseWindow) if is_active() => close_selected_window(),
                Some(Shortcut::Prefs) if is_active() => open_prefs(),
                _ => return passthrough,
            }
            swallow
        }
        CGEventType::KeyUp => {
            // Tant que le sélecteur est actif, on retient les relâchements des
            // touches qu'il consomme pour qu'elles n'atteignent pas l'app active.
            if is_active() && classify(ev).is_some() {
                return swallow;
            }
            passthrough
        }
        CGEventType::FlagsChanged => {
            // Le relâchement du modificateur de déclenchement valide la sélection.
            if is_active() && !trigger_held {
                dispatch(Input::OptionReleased);
            }
            passthrough
        }
        _ => passthrough,
    }
}

/// Raccourci clavier reconnu par le sélecteur.
enum Shortcut {
    Tab,
    Escape,
    CycleMode,
    Quit,
    CloseWindow,
    Prefs,
}

/// Identifie le raccourci correspondant à un évènement, source de vérité
/// partagée par les chemins keyDown et keyUp. Tab et Échap sont repérés par
/// keycode physique (identique sur tous les agencements) ; les lettres et la
/// virgule par le caractère tapé, donc indépendamment de l'agencement.
fn classify(ev: &CGEvent) -> Option<Shortcut> {
    match keycode(ev) {
        KEYCODE_TAB => Some(Shortcut::Tab),
        KEYCODE_ESCAPE => Some(Shortcut::Escape),
        _ => match typed_char(ev)? {
            'm' => Some(Shortcut::CycleMode),
            'q' => Some(Shortcut::Quit),
            'w' => Some(Shortcut::CloseWindow),
            ',' => Some(Shortcut::Prefs),
            _ => None,
        },
    }
}

fn keycode(ev: &CGEvent) -> i64 {
    CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode)
}

/// Caractère tapé (sans modificateurs), en minuscule. Permet de repérer les
/// raccourcis lettres/virgule indépendamment de l'agencement (AZERTY, QWERTY…).
fn typed_char(ev: &CGEvent) -> Option<char> {
    let event = NSEvent::eventWithCGEvent(ev)?;
    let string = event.charactersIgnoringModifiers()?;
    string.to_string().chars().next().map(|c| c.to_ascii_lowercase())
}

fn is_active() -> bool {
    STATE.with(|s| s.borrow().switcher.is_active())
}

/// Traite une pression sur Tab. À l'ouverture du cycle, on prend un instantané
/// frais des fenêtres et on en informe la machine à états avant de l'activer.
fn on_tab(shift: bool) {
    let action = STATE.with(|s| {
        let mut st = s.borrow_mut();
        if !st.switcher.is_active() {
            let st = &mut *st;
            let windows = windows::order_by_mru(windows::list_windows(), &mut st.mru);
            st.switcher.set_count(windows.len());
            st.windows = windows;
        }
        st.switcher.on_input(Input::Tab { shift })
    });
    perform(action);
}

fn dispatch(input: Input) {
    let action = STATE.with(|s| s.borrow_mut().switcher.on_input(input));
    perform(action);
}

/// Quitte l'application de la fenêtre sélectionnée (touche « q ») et met à jour
/// l'overlay : les fenêtres de cette app sont retirées tout de suite.
fn quit_selected_app() {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        if !st.switcher.is_active() || !st.quit_with_q {
            return;
        }
        let selected = st.switcher.selected();
        let Some((pid, id)) = st.windows.get(selected).map(|w| (w.pid, w.id)) else {
            return;
        };

        // Finder : on ferme seulement la fenêtre sélectionnée (le quitter le
        // tuerait et il se relancerait). Les autres apps quittent normalement.
        let is_finder = windows::focus::bundle_id(pid).as_deref() == Some("com.apple.finder");
        if is_finder {
            windows::ax::close_window(pid, id);
            // Retrait optimiste de cette seule fenêtre.
            st.windows.retain(|w| w.id != id);
        } else {
            windows::focus::quit_app(pid);
            // Retrait optimiste des fenêtres de l'app fermée.
            st.windows.retain(|w| w.pid != pid);
        }
        refresh_after_removal(&mut st);
    });
}

/// Ferme la seule fenêtre sélectionnée (touche « w ») sans quitter
/// l'application, pour toutes les apps, et met à jour l'overlay.
fn close_selected_window() {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        if !st.switcher.is_active() || !st.close_with_w {
            return;
        }
        let selected = st.switcher.selected();
        let Some((pid, id)) = st.windows.get(selected).map(|w| (w.pid, w.id)) else {
            return;
        };
        windows::ax::close_window(pid, id);
        // Retrait optimiste de cette seule fenêtre.
        st.windows.retain(|w| w.id != id);
        refresh_after_removal(&mut st);
    });
}

/// Après retrait de fenêtres : recalcule la sélection et rafraîchit l'overlay
/// (ou le masque s'il ne reste rien).
fn refresh_after_removal(st: &mut TapState) {
    let count = st.windows.len();
    st.switcher.refresh(count);

    let selected = st.switcher.selected();
    let active = st.switcher.is_active();
    if let Some(overlay) = st.overlay.as_mut() {
        if active && count > 0 {
            overlay.show(&st.windows, selected, st.mode);
        } else {
            overlay.hide();
        }
    }
}

/// Définit le modificateur de déclenchement (maintenu pendant le cycle).
pub fn set_trigger_modifier(modifier: TriggerModifier) {
    STATE.with(|s| s.borrow_mut().trigger_flag = modifier_flag(modifier));
}

/// (Dés)active le commutateur d'applications natif de macOS.
pub fn set_disable_native_cmd_tab(disable: bool) {
    crate::system::set_native_cmd_tab_enabled(!disable);
}

/// Autorise ou non la touche « q » à fermer l'app sélectionnée.
pub fn set_quit_with_q(enabled: bool) {
    STATE.with(|s| s.borrow_mut().quit_with_q = enabled);
}

/// Autorise ou non la touche « w » à fermer la fenêtre sélectionnée.
pub fn set_close_with_w(enabled: bool) {
    STATE.with(|s| s.borrow_mut().close_with_w = enabled);
}

fn modifier_flag(modifier: TriggerModifier) -> CGEventFlags {
    match modifier {
        TriggerModifier::Option => CGEventFlags::MaskAlternate,
        TriggerModifier::Command => CGEventFlags::MaskCommand,
        TriggerModifier::Control => CGEventFlags::MaskControl,
    }
}

/// Ouvre les préférences (touche `,`) : ferme d'abord l'overlay.
fn open_prefs() {
    dispatch(Input::Escape);
    STATE.with(|s| (s.borrow().on_open_prefs)());
}

/// Change le mode d'affichage depuis l'extérieur (préférences) et redessine
/// l'overlay s'il est ouvert.
pub fn set_mode(mode: DisplayMode) {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.mode = mode;
        if st.switcher.is_active() {
            let selected = st.switcher.selected();
            let st = &mut *st;
            if let Some(overlay) = st.overlay.as_mut() {
                overlay.show(&st.windows, selected, mode);
            }
        }
    });
}

/// Survol souris d'une cellule : déplace la sélection.
fn mouse_hover(index: usize) {
    dispatch(Input::Point { index });
}

/// Clic souris sur une cellule : valide la sélection (active la fenêtre).
fn mouse_click(index: usize) {
    dispatch(Input::Click { index });
}

/// Passe au mode d'affichage suivant et redessine l'overlay (sans changer la
/// sélection courante).
fn cycle_mode() {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        if !st.switcher.is_active() {
            return;
        }
        st.mode = st.mode.next();
        let selected = st.switcher.selected();
        let st = &mut *st;
        if let Some(overlay) = st.overlay.as_mut() {
            overlay.show(&st.windows, selected, st.mode);
        }
    });
}

/// Exécute l'action décidée par la machine à états en pilotant l'overlay.
///
/// M3 affiche/masque l'overlay et déplace la surbrillance ; M4 ajoutera
/// l'activation réelle de la fenêtre validée.
fn perform(action: Action) {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        let st = &mut *st;
        let Some(overlay) = st.overlay.as_mut() else {
            return;
        };
        match action {
            Action::Show { selected } => overlay.show(&st.windows, selected, st.mode),
            Action::Select { selected } => overlay.select(selected),
            Action::Commit { selected } => {
                overlay.hide();
                if let Some(w) = st.windows.get(selected) {
                    let raised = windows::focus::activate(w);
                    let how = if raised { "fenêtre levée" } else { "app activée" };
                    println!("[Tabs] ✓ {} [id {}] ({how})", w.app_name, w.id);
                }
            }
            Action::Cancel => overlay.hide(),
            Action::None => {}
        }
    });
}
