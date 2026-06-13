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

use crate::ui::Overlay;
use crate::windows::{self, Window};

use core::ffi::c_void;
use core::ptr::{self, NonNull};
use std::cell::RefCell;

use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRetained, CFRunLoop};
use objc2_foundation::MainThreadMarker;
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType,
};

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
    /// L'overlay affiché à l'écran (créé à l'installation, sur le thread
    /// principal).
    overlay: Option<Overlay>,
    /// Conservé pour pouvoir réactiver le tap s'il est désactivé par le système.
    port: Option<CFRetained<CFMachPort>>,
}

thread_local! {
    static STATE: RefCell<TapState> = RefCell::new(TapState {
        switcher: Switcher::new(),
        windows: Vec::new(),
        overlay: None,
        port: None,
    });
}

/// Installe le tap clavier sur la run loop courante (à appeler depuis le thread
/// principal, avant `NSApplication::run`). Retourne `false` en cas d'échec
/// (typiquement permission d'Accessibilité manquante).
pub fn install() -> bool {
    // L'overlay vit sur le thread principal (objets AppKit non-Send).
    let mtm = MainThreadMarker::new().expect("install doit s'exécuter sur le thread principal");
    let overlay = Overlay::new(mtm);
    STATE.with(|s| s.borrow_mut().overlay = Some(overlay));

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

    let Some(port) = port else {
        eprintln!("[Tabs] Échec de création du tap clavier (permission d'Accessibilité ?).");
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
    let option_held = flags.contains(CGEventFlags::MaskAlternate);
    let shift_held = flags.contains(CGEventFlags::MaskShift);

    match event_type {
        CGEventType::KeyDown => {
            let keycode = keycode(ev);
            if keycode == KEYCODE_TAB && option_held {
                on_tab(shift_held);
                return swallow;
            }
            if keycode == KEYCODE_ESCAPE && is_active() {
                dispatch(Input::Escape);
                return swallow;
            }
            passthrough
        }
        CGEventType::KeyUp => {
            let keycode = keycode(ev);
            // Tant que le sélecteur est actif, on retient les relâchements de
            // Tab/Échap pour qu'ils n'atteignent pas l'application active.
            if is_active() && (keycode == KEYCODE_TAB || keycode == KEYCODE_ESCAPE) {
                return swallow;
            }
            passthrough
        }
        CGEventType::FlagsChanged => {
            if is_active() && !option_held {
                dispatch(Input::OptionReleased);
            }
            passthrough
        }
        _ => passthrough,
    }
}

fn keycode(ev: &CGEvent) -> i64 {
    CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode)
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
            let windows = windows::list_windows();
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
            Action::Show { selected } => overlay.show(&st.windows, selected),
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
