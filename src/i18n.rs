//! Internationalisation des libellés de l'interface (français / anglais).
//!
//! Toutes les chaînes visibles sont regroupées dans [`Strings`]. La langue
//! courante vient de [`crate::config::Settings::language`] ; on récupère le jeu
//! de libellés via [`strings`].

use crate::config::Language;

/// Tous les libellés visibles de l'interface, pour une langue donnée.
#[derive(Clone, Copy)]
pub struct Strings {
    pub prefs_title: &'static str,
    pub subtitle: &'static str,

    pub tab_general: &'static str,
    pub tab_appearance: &'static str,
    pub tab_shortcut: &'static str,
    pub tab_permissions: &'static str,
    pub tab_about: &'static str,

    pub visibility: &'static str,
    pub show_in_dock: &'static str,
    pub show_in_menu_bar: &'static str,
    pub launch_at_login: &'static str,
    pub language: &'static str,
    pub quit_tabs: &'static str,

    pub display_mode: &'static str,
    pub mode_thumbnails: &'static str,
    pub mode_appicons: &'static str,
    pub mode_titles: &'static str,

    pub size: &'static str,
    /// Libellés des 5 niveaux de taille (le niveau 3 est la taille de base).
    pub size_levels: [&'static str; 5],

    pub trigger: &'static str,
    pub hold_key: &'static str,
    pub disable_cmd_tab: &'static str,
    pub quit_with_q: &'static str,
    pub close_with_w: &'static str,
    pub overlay_hint: &'static str,

    pub permissions: &'static str,
    pub accessibility: &'static str,
    pub screen_recording: &'static str,
    pub permissions_note: &'static str,
    pub refresh: &'static str,
    pub relaunch_tabs: &'static str,
    pub authorize: &'static str,
    pub granted_suffix: &'static str,
    pub not_granted_suffix: &'static str,

    pub about_tagline: &'static str,
    pub about_free: &'static str,

    pub menu_preferences: &'static str,
}

/// Jeu de libellés pour la langue demandée.
pub fn strings(lang: Language) -> Strings {
    match lang {
        Language::Fr => FR,
        Language::En => EN,
    }
}

const FR: Strings = Strings {
    prefs_title: "Préférences Tabs",
    subtitle: "Commutateur de fenêtres",

    tab_general: "Général",
    tab_appearance: "Apparence",
    tab_shortcut: "Raccourci",
    tab_permissions: "Permissions",
    tab_about: "À propos",

    visibility: "Visibilité",
    show_in_dock: "Afficher dans le Dock",
    show_in_menu_bar: "Afficher dans la barre des menus",
    launch_at_login: "Lancer au démarrage",
    language: "Langue",
    quit_tabs: "Quitter Tabs",

    display_mode: "Mode d'affichage",
    mode_thumbnails: "Miniatures",
    mode_appicons: "Icônes d'app",
    mode_titles: "Titres",

    size: "Taille de l'overlay",
    size_levels: [
        "Très petite",
        "Petite",
        "Normale (défaut)",
        "Grande",
        "Très grande",
    ],

    trigger: "Déclencheur",
    hold_key: "Touche maintenue (puis Tab) :",
    disable_cmd_tab: "Désactiver le Cmd-Tab du système",
    quit_with_q: "Autoriser « q » à fermer l'app sélectionnée",
    close_with_w: "Autoriser « w » à fermer la fenêtre sélectionnée",
    overlay_hint: "Overlay : « m » mode · « w » ferme la fenêtre · « q » ferme l'app (si activé) · « , » réglages.",

    permissions: "Permissions",
    accessibility: "Accessibilité",
    screen_recording: "Enregistrement de l'écran",
    permissions_note: "L'Accessibilité est requise et s'active immédiatement. \
                       L'enregistrement d'écran active les miniatures après un relancement.",
    refresh: "Rafraîchir",
    relaunch_tabs: "Relancer Tabs",
    authorize: "Autoriser",
    granted_suffix: "accordée",
    not_granted_suffix: "non accordée",

    about_tagline: "Commutateur de fenêtres pour macOS",
    about_free: "Libre · GPL-3.0",

    menu_preferences: "Préférences…",
};

const EN: Strings = Strings {
    prefs_title: "Tabs Preferences",
    subtitle: "Window switcher",

    tab_general: "General",
    tab_appearance: "Appearance",
    tab_shortcut: "Shortcut",
    tab_permissions: "Permissions",
    tab_about: "About",

    visibility: "Visibility",
    show_in_dock: "Show in Dock",
    show_in_menu_bar: "Show in menu bar",
    launch_at_login: "Launch at login",
    language: "Language",
    quit_tabs: "Quit Tabs",

    display_mode: "Display mode",
    mode_thumbnails: "Thumbnails",
    mode_appicons: "App icons",
    mode_titles: "Titles",

    size: "Overlay size",
    size_levels: [
        "Smallest",
        "Small",
        "Default",
        "Large",
        "Largest",
    ],

    trigger: "Trigger",
    hold_key: "Hold key (then Tab):",
    disable_cmd_tab: "Disable the system Cmd-Tab",
    quit_with_q: "Allow \"q\" to close the selected app",
    close_with_w: "Allow \"w\" to close the selected window",
    overlay_hint: "Overlay: \"m\" mode · \"w\" closes the window · \"q\" closes the app (if enabled) · \",\" settings.",

    permissions: "Permissions",
    accessibility: "Accessibility",
    screen_recording: "Screen Recording",
    permissions_note: "Accessibility is required and takes effect immediately. \
                       Screen Recording enables thumbnails after a relaunch.",
    refresh: "Refresh",
    relaunch_tabs: "Relaunch Tabs",
    authorize: "Authorize",
    granted_suffix: "granted",
    not_granted_suffix: "not granted",

    about_tagline: "Window switcher for macOS",
    about_free: "Free · GPL-3.0",

    menu_preferences: "Preferences…",
};
