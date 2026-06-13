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

use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::MainThreadMarker;

fn main() {
    let mtm = MainThreadMarker::new().expect("Tabs doit être lancé depuis le thread principal");

    let app = NSApplication::sharedApplication(mtm);
    // Application « agent » : ni icône dans le Dock, ni menu — juste l'overlay.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    if permissions::ensure_accessibility() {
        println!("[Tabs] Permission d'Accessibilité accordée.");
    } else {
        eprintln!(
            "[Tabs] Permission d'Accessibilité manquante.\n\
             Autorise « Tabs » dans Réglages Système › Confidentialité et sécurité › \
             Accessibilité, puis relance l'application."
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
