//! Réglages système via API privée SkyLight/CGS.
//!
//! Sert à activer/désactiver le commutateur d'applications natif de macOS
//! (Cmd-Tab et Cmd-Shift-Tab) lorsque l'utilisateur veut que Tabs le remplace.

// API privée : active/désactive un « symbolic hotkey » système.
// Renvoie un CGError (0 = succès).
unsafe extern "C" {
    fn CGSSetSymbolicHotKeyEnabled(hot_key: i32, enabled: bool) -> i32;
}

// Identifiants des raccourcis symboliques du commutateur d'applications.
const HOTKEY_CMD_TAB: i32 = 1;
const HOTKEY_CMD_SHIFT_TAB: i32 = 2;

/// Active ou désactive le commutateur d'applications natif (Cmd-Tab et
/// Cmd-Shift-Tab). Désactivé quand Tabs prend le relais ; réactivé sinon.
pub fn set_native_cmd_tab_enabled(enabled: bool) {
    unsafe {
        CGSSetSymbolicHotKeyEnabled(HOTKEY_CMD_TAB, enabled);
        CGSSetSymbolicHotKeyEnabled(HOTKEY_CMD_SHIFT_TAB, enabled);
    }
}
