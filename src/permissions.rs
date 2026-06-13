//! Vérification et demande des permissions système nécessaires à Tabs.
//!
//! - **Accessibilité** (obligatoire) : permet d'observer les événements clavier
//!   globaux et de lister/activer les fenêtres des autres applications.
//! - **Enregistrement de l'écran** : nécessaire pour capturer les miniatures de
//!   fenêtres (et pour lire leurs titres).

use objc2_application_services::{
    kAXTrustedCheckOptionPrompt, AXIsProcessTrusted, AXIsProcessTrustedWithOptions,
};
use objc2_core_foundation::{kCFBooleanTrue, CFBoolean, CFDictionary, CFString};
use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};

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

/// Indique si la permission d'Accessibilité est accordée (sans déclencher de
/// prompt).
pub fn is_accessibility_granted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Indique si l'accès à l'enregistrement de l'écran est accordé (sans prompt).
pub fn is_screen_recording_granted() -> bool {
    CGPreflightScreenCaptureAccess()
}

/// Vérifie l'accès à l'enregistrement de l'écran et, s'il manque, déclenche le
/// prompt système. Sans cet accès, les miniatures et titres de fenêtres ne sont
/// pas disponibles (on retombe alors sur les icônes d'application).
///
/// Retourne `true` si l'accès est déjà accordé.
pub fn ensure_screen_recording() -> bool {
    if CGPreflightScreenCaptureAccess() {
        return true;
    }
    // Affiche le prompt ; l'autorisation ne prend effet qu'au prochain lancement.
    CGRequestScreenCaptureAccess()
}
