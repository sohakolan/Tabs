# Tabs

> *Because you shouldn't have to pay for a feature that should be native.*

A lightweight, fast window switcher for macOS — written in **Rust**, free and open source.

## Installation

### Homebrew

```sh
brew tap sohakolan/tabs https://github.com/sohakolan/Tabs
brew install --cask sohakolan/tabs/tabs
xattr -dr com.apple.quarantine /Applications/Tabs.app   # Tabs isn't notarized by Apple
```

### Download

1. Grab **`Tabs-arm64.dmg`** from the [Releases](https://github.com/sohakolan/Tabs/releases)
   page.
2. Open the DMG and drag **Tabs** into **Applications**.
3. Tabs isn't notarized by Apple (that needs a paid developer account), so macOS blocks it on
   first launch. Clear the quarantine flag, then open it:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Tabs.app
   open /Applications/Tabs.app
   ```

> Built for **Apple Silicon (arm64)**, **macOS 14+** (Sonoma).

### Build from source

Requirements: **Rust** (stable) and **macOS 14+**.

```sh
make run     # compiles, assembles dist/Tabs.app and launches it
```

### Permissions

On first launch, allow "Tabs" in **System Settings › Privacy & Security**:

- **Accessibility** (required) — observe the keyboard and list/activate windows;
- **Screen Recording** (optional) — shows window thumbnails (otherwise falls back to app
  icons).

In the preferences **Permissions** tab, "Refresh" re-checks the statuses (and enables the
shortcut as soon as Accessibility is granted, without relaunching); "Relaunch Tabs" applies
Screen Recording (this permission only takes effect after a restart).

## Why

macOS has no real window switcher. **Tabs** fills that gap — for free and under the
**GPL-3.0** license — aiming for a near-instant cold start, a small memory footprint (no Swift
runtime) and *zero-copy* thumbnails via ScreenCaptureKit.

> Rendering necessarily goes through Core Animation / the system compositor: Tabs doesn't try
> to "beat AppKit" on raw rendering, but to minimize everything else (startup, memory, window
> enumeration, on-demand thumbnail capture).

## Tech stack

- **UI**: native AppKit via [`objc2`](https://github.com/madsmtm/objc2) — non-activating
  `NSPanel` + Core Animation.
- **Windows**: Accessibility API (`AXUIElement`) for enumeration and activation; `CGWindowList`
  for z-order and titles.
- **Trigger**: `CGEventTap` (modifier tracking, commit on release).
- **Thumbnails**: ScreenCaptureKit (macOS 14+), *zero-copy* rendering via IOSurface.
- **Spaces / reliable focus**: private SkyLight/CGS APIs (`_AXUIElementGetWindow`,
  `_SLPSSetFrontProcessWithOptions`, …).

## Development

To contribute or iterate on the code:

```sh
make signing-setup   # once: stable signing identity
make bundle          # release build + assembles dist/Tabs.app
make dmg             # assembles dist/Tabs-<arch>.dmg (installer for a release)
cargo run            # runs the bare binary (handy for logs; the app stays an agent)
```

> **Why `make signing-setup`?** With ad-hoc signing, the code identity changes on every build
> and macOS (TCC) forgets the granted permissions, forcing you to re-grant them after each
> rebuild. A stable signing identity keeps them — useful when recompiling often. A plain
> install (a single `make run`) doesn't need it.

## License

[GPL-3.0-or-later](LICENSE).
