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

/// Réglages de l'utilisateur.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Mode d'affichage des cellules.
    pub mode: DisplayMode,
    /// Afficher l'icône de l'application dans le Dock.
    pub show_in_dock: bool,
    /// Afficher l'icône de l'application dans la barre des menus.
    pub show_in_menu_bar: bool,
    /// Remplacer le Cmd-Tab natif de macOS par Tabs (déclencheur = Cmd-Tab et
    /// désactivation du commutateur système).
    pub replace_cmd_tab: bool,
}

impl Default for Settings {
    fn default() -> Self {
        // Tout masqué par défaut : l'app ne se voit nulle part.
        Self {
            mode: DisplayMode::Thumbnails,
            show_in_dock: false,
            show_in_menu_bar: false,
            replace_cmd_tab: false,
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
