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
