//! Tabs — un window switcher façon commutateur de fenêtres pour macOS, écrit en Rust.
//!
//! Objectif du projet : réimplémenter les fonctionnalités d'commutateur de fenêtres en restant
//! libre (GPL-3.0) et performant. Voir le README pour la feuille de route.
//!
//! Jalon M0 : l'application démarre comme un agent (pas d'icône dans le Dock,
//! pas de menu) et s'assure d'avoir la permission d'Accessibilité, qui est
//! indispensable pour intercepter le raccourci global et manipuler les fenêtres
//! des autres applications. Les jalons suivants y branchent le tap clavier,
//! l'énumération des fenêtres et l'overlay.

mod hotkey;
mod permissions;
mod ui;
mod windows;

use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::MainThreadMarker;

fn main() {
    // Mode diagnostic : `tabs --list` imprime les fenêtres détectées et quitte,
    // sans installer le tap ni démarrer la boucle d'évènements. Pratique pour
    // vérifier l'énumération.
    if std::env::args().any(|a| a == "--list") {
        let windows = windows::list_windows();
        println!("[Tabs] {} fenêtre(s) détectée(s) (de l'avant vers l'arrière) :", windows.len());
        for (i, w) in windows.iter().enumerate() {
            let title = if w.title.is_empty() {
                "(titre indisponible)"
            } else {
                w.title.as_str()
            };
            println!("  {i:>2}. {} — {} [id {}]", w.app_name, title, w.id);
        }
        return;
    }

    let demo = std::env::args().any(|a| a == "--demo");

    let mtm = MainThreadMarker::new().expect("Tabs doit être lancé depuis le thread principal");

    let app = NSApplication::sharedApplication(mtm);
    // Application « agent » : ni icône dans le Dock, ni menu — juste l'overlay.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // Mode diagnostic : `tabs --demo` affiche l'overlay (sans installer le tap)
    // pour inspecter le rendu, puis laisse la boucle tourner.
    if demo {
        let mut overlay = ui::Overlay::new(mtm);
        let wins = windows::list_windows();
        let selected = if wins.len() > 1 { 1 } else { 0 };
        println!("[Tabs] --demo : {} fenêtre(s), sélection {selected}", wins.len());
        overlay.show(&wins, selected, ui::DisplayMode::Thumbnails);
        // L'overlay doit rester en vie pendant la boucle d'évènements.
        std::mem::forget(overlay);
        app.run();
        return;
    }

    if permissions::ensure_accessibility() {
        println!("[Tabs] Permission d'Accessibilité accordée.");
    } else {
        eprintln!(
            "[Tabs] Permission d'Accessibilité manquante.\n\
             Autorise « Tabs » dans Réglages Système › Confidentialité et sécurité › \
             Accessibilité, puis relance l'application."
        );
    }

    // Demande l'accès à l'enregistrement de l'écran (miniatures + titres). Sans
    // lui, Tabs retombe sur les icônes d'application.
    if !permissions::ensure_screen_recording() {
        eprintln!(
            "[Tabs] Enregistrement de l'écran non accordé : miniatures et titres \
             indisponibles (repli sur les icônes). Autorise « Tabs » dans Réglages \
             Système › Confidentialité et sécurité › Enregistrement de l'écran."
        );
    }

    // Installe le déclencheur clavier (Option-Tab). Sans permission
    // d'Accessibilité, le tap ne peut pas être créé ; l'application continue
    // néanmoins de tourner pour laisser l'utilisateur accorder l'accès.
    hotkey::install();

    // Boucle d'événements AppKit. Le tap clavier y est greffé ; l'overlay (M3)
    // viendra s'y ajouter.
    app.run();
}
