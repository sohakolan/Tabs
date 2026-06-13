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

mod app_ui;
mod config;
mod hotkey;
mod login;
mod permissions;
mod system;
mod ui;
mod windows;

use objc2::runtime::ProtocolObject;
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
            let tag = if w.minimized { " [réduit]" } else { "" };
            println!("  {i:>2}. {} — {} [id {}]{tag}", w.app_name, title, w.id);
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
        let mut overlay = ui::Overlay::new(mtm, Box::new(|_| {}), Box::new(|_| {}));
        let wins = windows::list_windows();
        let selected = if wins.len() > 1 { 1 } else { 0 };
        println!("[Tabs] --demo : {} fenêtre(s), sélection {selected}", wins.len());
        overlay.show(&wins, selected, ui::DisplayMode::Thumbnails);
        // L'overlay doit rester en vie pendant la boucle d'évènements.
        std::mem::forget(overlay);
        app.run();
        return;
    }

    // Charge les réglages et applique la visibilité (Dock / barre des menus).
    // Le contrôleur reste vivant toute la session (cible des actions de menu).
    let first_run = !config::exists();
    let settings = config::load();
    let controller = app_ui::AppController::new(mtm, settings.clone());
    controller.apply_initial();

    // On se contente de vérifier l'état des permissions au démarrage, sans
    // jamais déclencher de prompt : la demande se fait uniquement via les
    // boutons « Autoriser » de la fenêtre de préférences.
    if !permissions::is_accessibility_granted() {
        eprintln!(
            "[Tabs] Accessibilité non accordée — ouvre les préférences (touche « , » \
             pendant l'overlay, ou l'icône de la barre des menus) pour l'autoriser."
        );
    }
    if !permissions::is_screen_recording_granted() {
        eprintln!(
            "[Tabs] Enregistrement de l'écran non accordé — miniatures indisponibles \
             (repli sur les icônes d'application)."
        );
    }

    // Installe le déclencheur clavier (Option-Tab). La touche `,` ouvre les
    // préférences via le contrôleur.
    let prefs_controller = controller.clone();
    hotkey::install(
        settings.mode,
        Box::new(move || prefs_controller.show_preferences()),
    );
    // Applique le modificateur de déclenchement, l'état du Cmd-Tab système et le
    // lancement au démarrage.
    hotkey::set_trigger_modifier(settings.trigger);
    hotkey::set_disable_native_cmd_tab(settings.disable_native_cmd_tab);
    login::set_launch_at_login(settings.launch_at_login);

    // Matérialise le fichier de réglages au premier lancement.
    if first_run {
        config::save(&settings);
    }

    // Délégué de l'application : un clic sur l'icône du Dock rouvre les
    // préférences.
    app.setDelegate(Some(ProtocolObject::from_ref(&*controller)));
    // Ouvre les préférences au lancement manuel, mais pas lors d'un démarrage
    // automatique à l'ouverture de session (le LaunchAgent passe « --login »).
    let launched_at_login = std::env::args().any(|a| a == "--login");
    if !launched_at_login {
        controller.show_preferences();
    }

    // Garde le contrôleur en vie pendant toute la durée de la boucle.
    let _controller = controller;

    // Boucle d'événements AppKit.
    app.run();
}
