//! Vérification et demande des permissions système nécessaires à Tabs.
//!
//! - **Accessibilité** (obligatoire) : permet d'observer les événements clavier
//!   globaux et de lister/activer les fenêtres des autres applications.
//! - **Enregistrement de l'écran** (à venir, M5) : nécessaire pour capturer les
//!   miniatures de fenêtres via ScreenCaptureKit.

use objc2_application_services::{kAXTrustedCheckOptionPrompt, AXIsProcessTrustedWithOptions};
use objc2_core_foundation::{kCFBooleanTrue, CFBoolean, CFDictionary, CFString};

/// Vérifie si Tabs est un client d'accessibilité de confiance.
///
/// Si la permission n'est pas encore accordée, déclenche le prompt système
/// invitant l'utilisateur à l'autoriser. Le prompt est asynchrone et n'affecte
/// pas la valeur de retour.
///
/// Retourne `true` si la permission est déjà accordée au moment de l'appel.
pub fn ensure_accessibility() -> bool {
    // Dictionnaire d'options { kAXTrustedCheckOptionPrompt: true } afin que le
    // système affiche le prompt quand la permission n'est pas encore accordée.
    let key: &CFString = unsafe { kAXTrustedCheckOptionPrompt };
    let value: &CFBoolean = unsafe { kCFBooleanTrue }.expect("kCFBooleanTrue est NULL");
    let options = CFDictionary::from_slices(&[key], &[value]);

    // L'API AX attend un dictionnaire CoreFoundation opaque ; notre dictionnaire
    // typé `CFDictionary<CFString, CFBoolean>` lui est structurellement
    // identique (tous deux ne sont qu'un `__CFDictionary`).
    let typed: &CFDictionary<CFString, CFBoolean> = &options;
    let opaque: &CFDictionary = unsafe { &*(typed as *const _ as *const CFDictionary) };

    unsafe { AXIsProcessTrustedWithOptions(Some(opaque)) }
}
