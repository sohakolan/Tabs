//! Réglages persistants de Tabs.
//!
//! Sérialisés en JSON dans `~/Library/Application Support/Tabs/settings.json`.
//! Par défaut, l'application est **invisible** : ni icône dans le Dock, ni dans
//! la barre des menus. La fenêtre de préférences reste joignable par le
//! raccourci dédié (touche `,` pendant l'overlay).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ui::DisplayMode;

/// Modificateur maintenu pour déclencher et parcourir le sélecteur (+ Tab).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerModifier {
    Option,
    Command,
    Control,
}

impl Default for TriggerModifier {
    fn default() -> Self {
        Self::Option
    }
}

/// Langue de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    Fr,
    En,
}

impl Default for Language {
    fn default() -> Self {
        Self::Fr
    }
}

/// Réglages de l'utilisateur.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Mode d'affichage des cellules.
    pub mode: DisplayMode,
    /// Modificateur de déclenchement (maintenu pendant le cycle).
    pub trigger: TriggerModifier,
    /// Désactiver le commutateur d'applications natif (Cmd-Tab) de macOS.
    pub disable_native_cmd_tab: bool,
    /// Afficher l'icône de l'application dans le Dock.
    pub show_in_dock: bool,
    /// Afficher l'icône de l'application dans la barre des menus.
    pub show_in_menu_bar: bool,
    /// Lancer Tabs automatiquement à l'ouverture de session.
    pub launch_at_login: bool,
    /// Autoriser la touche « q » de l'overlay à fermer l'app sélectionnée
    /// (désactivé par défaut, pour éviter les fermetures accidentelles).
    pub quit_with_q: bool,
    /// Autoriser la touche « w » de l'overlay à fermer la fenêtre sélectionnée
    /// (activé par défaut ; non destructif, l'app reste ouverte).
    pub close_with_w: bool,
    /// Langue de l'interface (français par défaut).
    pub language: Language,
    /// Taille de l'overlay : niveau 1 (compact) à 5 (grand). Le niveau 3 est la
    /// taille de base (facteur 1.0).
    pub scale: u8,
}

/// Facteur d'échelle appliqué à l'overlay pour un niveau `1..=5`. Le niveau 3
/// (par défaut) vaut 1.0 ; les autres niveaux réduisent ou agrandissent tout.
pub fn scale_factor(level: u8) -> f64 {
    match level.clamp(1, 5) {
        1 => 0.72,
        2 => 0.85,
        3 => 1.0,
        4 => 1.2,
        _ => 1.45,
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: DisplayMode::Thumbnails,
            trigger: TriggerModifier::Option,
            disable_native_cmd_tab: false,
            // Visible dans la barre des menus (repère discret), pas dans le Dock.
            show_in_dock: false,
            show_in_menu_bar: true,
            launch_at_login: false,
            quit_with_q: false,
            close_with_w: true,
            language: Language::Fr,
            // Niveau 3 = taille de base.
            scale: 3,
        }
    }
}

/// Indique si le fichier de réglages existe déjà (faux au tout premier
/// lancement).
pub fn exists() -> bool {
    file_path().map(|p| p.exists()).unwrap_or(false)
}

/// Charge les réglages, ou les valeurs par défaut si le fichier est absent ou
/// illisible.
pub fn load() -> Settings {
    let Some(path) = file_path() else {
        return Settings::default();
    };
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// Enregistre les réglages sur disque (échec silencieux : best-effort).
pub fn save(settings: &Settings) {
    let Some(path) = file_path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(&path, json);
    }
}

/// Chemin du fichier de réglages, dérivé de `$HOME`.
fn file_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut path = PathBuf::from(home);
    path.push("Library/Application Support/Tabs/settings.json");
    Some(path)
}
