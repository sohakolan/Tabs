# Tabs

> *Parce qu'on ne devrait pas avoir à payer pour une fonction qui devrait être native.*

Un commutateur de fenêtres pour macOS, écrit en **Rust** — libre, léger et rapide.

## Installation

Prérequis : **Rust** stable et **macOS 14+** (Sonoma).

```sh
make signing-setup   # une seule fois : identité de signature stable (permissions persistantes)
make bundle          # compile en release et assemble dist/Tabs.app
make run             # build + bundle + lance dist/Tabs.app
```

Au premier lancement, autorise « Tabs » dans **Réglages Système › Confidentialité et
sécurité** :

- **Accessibilité** (obligatoire) — observer le clavier et lister/activer les fenêtres ;
- **Enregistrement de l'écran** (optionnel) — affiche les miniatures (sinon repli sur les
  icônes d'application).

Dans le volet **Permissions** des préférences : « Rafraîchir » réévalue les statuts (et
active le raccourci dès que l'Accessibilité est accordée, sans relancer) ; « Relancer Tabs »
applique l'enregistrement d'écran (cette permission n'est prise en compte qu'au redémarrage).

> **Pourquoi `make signing-setup` ?** En signature ad-hoc, l'identité de code change à chaque
> build et macOS (TCC) réoublie les permissions. Une identité de signature stable les fait
> persister entre les rebuilds — à exécuter une seule fois.

Pour itérer pendant le développement :

```sh
cargo run     # lance le binaire nu (utile pour les logs ; l'app reste un agent)
```

## Pourquoi

macOS n'offre pas de véritable commutateur de fenêtres. **Tabs** comble ce manque —
gratuitement et sous licence **GPL-3.0** — avec pour objectifs un démarrage à froid quasi
instantané, une faible empreinte mémoire (pas de runtime Swift) et des miniatures
*zéro-copie* via ScreenCaptureKit.

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

## Feuille de route

| Jalon | Contenu | État |
|-------|---------|------|
| **M0** | Scaffolding : app agent (`NSApplication` `.accessory`), permission Accessibilité, bundle `.app` | ✅ |
| **M1** | Déclencheur Option-Tab (`CGEventTap`, cycle, commit au relâchement, annulation Échap) | ✅ |
| **M2** | Énumération : fenêtres visibles (`CGWindowList`, apps du Dock, hors soi) **+ fenêtres minimisées** (via Accessibilité) · **titres réels** (onglet actif, piste Spotify…) | ✅ |
| **M3** | Overlay : `NSPanel` non-activant, rangée icône+titre, surbrillance/navigation clavier | ✅ |
| **M4** | Activation de la fenêtre sélectionnée → **MVP utilisable** | ✅ |
| **M5** | Miniatures de fenêtres (`CGWindowListCreateImage`, repli icône) | ✅ |
| **Modes** | Affichage commutable Thumbnails/AppIcons/Titles (touche `m`) · **souris** (survol/clic) | ✅ |
| **Réglages** | `q` quitte l'app sélectionnée · `,` préférences · Cmd-Q / menu pour quitter Tabs · préférences ouvertes au lancement et au clic Dock | ✅ |
| **Préférences** | Vraie fenêtre **à onglets** (Général · Apparence · Raccourci · Permissions · À propos) : en-tête logo, **tuiles d'aperçu** des 3 modes, **modificateur** (⌥/⌘/⌃), visibilité Dock/menu, **lancement au démarrage**, **statut des permissions** (auto-rafraîchi) | ✅ |
| **Identité** | Logo/icône d'app (`assets/icon.svg` → `AppIcon.icns`) + aperçus de modes, intégrés au bundle | ✅ |
| Post-MVP | Migration ScreenCaptureKit · Spaces · multi-écran · recherche · actions traffic-light · gestes · focus renforcé (SLPS) · a11y · i18n | — |

## Licence

[GPL-3.0-or-later](LICENSE).
