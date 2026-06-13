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

use core::ffi::c_void;
use core::ptr::{self, NonNull};
use std::cell::RefCell;

use objc2_core_foundation::{kCFRunLoopCommonModes, CFMachPort, CFRetained, CFRunLoop};
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

/// Placeholder M1 : nombre de fenêtres simulées, pour visualiser le cycle dans
/// les logs. M2 le remplacera par l'énumération réelle des fenêtres.
const DEMO_WINDOW_COUNT: usize = 6;

/// État partagé entre la run loop et le callback (tout sur le thread principal).
struct TapState {
    switcher: Switcher,
    /// Conservé pour pouvoir réactiver le tap s'il est désactivé par le système.
    port: Option<CFRetained<CFMachPort>>,
}

thread_local! {
    static STATE: RefCell<TapState> = RefCell::new(TapState {
        switcher: Switcher::new(),
        port: None,
    });
}

/// Installe le tap clavier sur la run loop courante (à appeler depuis le thread
/// principal, avant `NSApplication::run`). Retourne `false` en cas d'échec
/// (typiquement permission d'Accessibilité manquante).
pub fn install() -> bool {
    STATE.with(|s| s.borrow_mut().switcher.set_count(DEMO_WINDOW_COUNT));

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
                dispatch(Input::Tab { shift: shift_held });
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

fn dispatch(input: Input) {
    let action = STATE.with(|s| s.borrow_mut().switcher.on_input(input));
    perform(action);
}

/// Exécute l'action décidée par la machine à états.
///
/// M1 se contente de tracer ; M3 (overlay) et M4 (activation de fenêtre)
/// brancheront ici l'UI et le focus réel.
fn perform(action: Action) {
    match action {
        Action::Show { selected } => {
            println!("[Tabs] ▸ ouverture du sélecteur — fenêtre {selected}")
        }
        Action::Select { selected } => println!("[Tabs]   sélection → fenêtre {selected}"),
        Action::Commit { selected } => println!("[Tabs] ✓ activation de la fenêtre {selected}"),
        Action::Cancel => println!("[Tabs] ✕ annulé"),
        Action::None => {}
    }
}
