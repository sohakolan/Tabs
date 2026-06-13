# Tabs

Un *window switcher* façon **commutateur de fenêtres** pour macOS, écrit en **Rust** — libre, léger
et rapide.

## Pourquoi

[commutateur de fenêtres](https://github.com/sohakolan/Tabs) est l'excellente référence du genre
sur macOS, mais une partie de ses fonctionnalités passe désormais derrière une version
**Pro** payante. **Tabs** vise à reconstruire l'ensemble des fonctionnalités, gratuitement
et sous licence **GPL-3.0**, avec pour objectifs un démarrage à froid quasi instantané, une
faible empreinte mémoire (pas de runtime Swift) et des miniatures *zéro-copie* via
ScreenCaptureKit.

> Le rendu passe nécessairement par Core Animation / le compositeur du système : Tabs ne
> cherche pas à « battre AppKit » sur le rendu pur, mais à minimiser tout le reste (démarrage,
> mémoire, énumération des fenêtres, capture des miniatures à la demande).

## Pile technique

- **UI** : AppKit natif via [`objc2`](https://github.com/madsmtm/objc2) — `NSPanel`
  non-activant + Core Animation.
- **Fenêtres** : API d'Accessibilité (`AXUIElement`) pour l'énumération et l'activation ;
  `CGWindowList` pour l'ordre z et les titres.
- **Déclencheur** : `CGEventTap` (suivi des modificateurs, commit au relâchement).
- **Miniatures** : ScreenCaptureKit (macOS 14+), rendu *zéro-copie* via IOSurface.
- **Spaces / focus fiable** : API privées SkyLight/CGS (`_AXUIElementGetWindow`,
  `_SLPSSetFrontProcessWithOptions`, …).

**Cible : macOS 14+ (Sonoma).** Permissions requises : **Accessibilité** (obligatoire) et
**Enregistrement de l'écran** (pour les miniatures).

## Feuille de route

| Jalon | Contenu | État |
|-------|---------|------|
| **M0** | Scaffolding : app agent (`NSApplication` `.accessory`), permission Accessibilité, bundle `.app` | ✅ |
| **M1** | Déclencheur Option-Tab (`CGEventTap`, cycle, commit au relâchement, annulation Échap) | ⏳ |
| **M2** | Énumération des fenêtres du Space courant (AX + `CGWindowList`) | ⏳ |
| **M3** | Overlay : `NSPanel` non-activant, grille icône+titre, navigation clavier | ⏳ |
| **M4** | Activation de la fenêtre sélectionnée → **MVP utilisable** | ⏳ |
| **M5** | Miniatures live (ScreenCaptureKit, IOSurface) | ⏳ |
| Post-MVP | Spaces, multi-écran, recherche, styles, actions traffic-light, gestes, préférences, a11y, i18n | — |

## Build & lancement

Prérequis : Rust stable et macOS 14+.

```sh
make bundle   # compile en release et assemble dist/Tabs.app
make run      # build + bundle + lance dist/Tabs.app
```

Au premier lancement, autorise « Tabs » dans **Réglages Système › Confidentialité et
sécurité › Accessibilité**, puis relance.

Pour itérer rapidement pendant le développement :

```sh
cargo run     # lance le binaire nu (utile pour les logs ; l'app reste un agent)
```

## Licence

[GPL-3.0-or-later](LICENSE). Ce projet s'inspire du fonctionnement d'commutateur de fenêtres (également
GPL-3.0), notamment pour les parties reposant sur des API privées de macOS.
