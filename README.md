# Tabs

> *Parce qu'on ne devrait pas avoir à payer pour une fonction qui devrait être native.*

Un commutateur de fenêtres pour macOS, écrit en **Rust** — libre, léger et rapide.

## Installation

### Télécharger (recommandé)

1. Récupère **`Tabs-arm64.dmg`** sur la page
   [Releases](https://github.com/sohakolan/Tabs/releases).
2. Ouvre le DMG et glisse **Tabs** dans **Applications**.
3. L'app n'est pas notarisée par Apple (ça nécessite un compte développeur payant), donc
   macOS la bloque au premier lancement. Lève la quarantaine puis ouvre-la :

   ```sh
   xattr -dr com.apple.quarantine /Applications/Tabs.app
   open /Applications/Tabs.app
   ```

> Compatibilité : build **Apple Silicon (arm64)**, **macOS 14+** (Sonoma).

### Construire depuis les sources

Prérequis : **Rust** stable et **macOS 14+**.

```sh
make run     # compile, assemble dist/Tabs.app et le lance
```

### Permissions

Au premier lancement, autorise « Tabs » dans **Réglages Système › Confidentialité et
sécurité** :

- **Accessibilité** (obligatoire) — observer le clavier et lister/activer les fenêtres ;
- **Enregistrement de l'écran** (optionnel) — affiche les miniatures (sinon repli sur les
  icônes d'application).

Dans le volet **Permissions** des préférences : « Rafraîchir » réévalue les statuts (et
active le raccourci dès que l'Accessibilité est accordée, sans relancer) ; « Relancer Tabs »
applique l'enregistrement d'écran (cette permission n'est prise en compte qu'au redémarrage).

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

## Développement

Pour contribuer ou itérer sur le code :

```sh
make signing-setup   # une seule fois : identité de signature stable
make bundle          # compile en release et assemble dist/Tabs.app
make dmg             # assemble dist/Tabs-<arch>.dmg (fichier d'installation pour une release)
cargo run            # lance le binaire nu (utile pour les logs ; l'app reste un agent)
```

> **Pourquoi `make signing-setup` ?** En signature ad-hoc, l'identité de code change à chaque
> build et macOS (TCC) réoublie les permissions, qu'il faut alors ré-accorder après chaque
> rebuild. Une identité de signature stable les fait persister — utile quand on recompile
> souvent. Un simple usage (un seul `make run`) n'en a pas besoin.

## Licence

[GPL-3.0-or-later](LICENSE).
