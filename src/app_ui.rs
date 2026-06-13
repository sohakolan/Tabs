//! Contrôleur d'application : visibilité (Dock / barre des menus), menu de la
//! barre d'état et fenêtre de préférences.
//!
//! Par défaut l'application est invisible (cf. [`crate::config`]). La fenêtre de
//! préférences est joignable par le raccourci `,` (pendant l'overlay) et, si
//! activée, par l'icône de la barre des menus.

use core::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType, NSBox,
    NSBoxType, NSButton, NSCellImagePosition, NSColor, NSControlStateValueOn, NSFont, NSImage,
    NSImageScaling, NSImageView, NSMenu, NSMenuItem, NSPopUpButton, NSStatusBar, NSStatusItem,
    NSTextField, NSTitlePosition, NSVariableStatusItemLength, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::config::{self, Settings, TriggerModifier};
use crate::ui::DisplayMode;
use crate::{hotkey, permissions};

const WIN_W: f64 = 460.0;
const WIN_H: f64 = 700.0;

pub(crate) struct Ivars {
    mtm: MainThreadMarker,
    settings: RefCell<Settings>,
    prefs_window: RefCell<Option<Retained<NSWindow>>>,
    status_item: RefCell<Option<Retained<NSStatusItem>>>,
    /// Boîtes de surbrillance des tuiles de mode (pour mettre à jour la
    /// sélection), reconstruites à chaque ouverture des préférences.
    tiles: RefCell<Vec<(DisplayMode, Retained<NSBox>)>>,
}

define_class!(
    // SAFETY: superclasse NSObject sans contrainte ; pas de Drop.
    #[unsafe(super = objc2_foundation::NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    pub(crate) struct AppController;

    impl AppController {
        #[unsafe(method(showPreferences:))]
        fn action_show_preferences(&self, _sender: Option<&AnyObject>) {
            self.show_preferences();
        }

        #[unsafe(method(quitApp:))]
        fn action_quit(&self, _sender: Option<&AnyObject>) {
            // Restaure le commutateur natif puis quitte de façon garantie.
            crate::system::set_native_cmd_tab_enabled(true);
            std::process::exit(0);
        }

        #[unsafe(method(selectThumbnails:))]
        fn action_mode_thumbnails(&self, _sender: Option<&AnyObject>) {
            self.select_mode(DisplayMode::Thumbnails);
        }

        #[unsafe(method(selectAppIcons:))]
        fn action_mode_appicons(&self, _sender: Option<&AnyObject>) {
            self.select_mode(DisplayMode::AppIcons);
        }

        #[unsafe(method(selectTitles:))]
        fn action_mode_titles(&self, _sender: Option<&AnyObject>) {
            self.select_mode(DisplayMode::Titles);
        }

        #[unsafe(method(triggerChanged:))]
        fn action_trigger_changed(&self, sender: Option<&AnyObject>) {
            let idx = sender
                .and_then(|s| s.downcast_ref::<NSPopUpButton>())
                .map(|p| p.indexOfSelectedItem())
                .unwrap_or(0);
            let modifier = match idx {
                1 => TriggerModifier::Command,
                2 => TriggerModifier::Control,
                _ => TriggerModifier::Option,
            };
            self.ivars().settings.borrow_mut().trigger = modifier;
            self.save();
            hotkey::set_trigger_modifier(modifier);
        }

        #[unsafe(method(toggleDisableCmdTab:))]
        fn action_toggle_disable_cmd_tab(&self, sender: Option<&AnyObject>) {
            let on = checkbox_is_on(sender);
            self.ivars().settings.borrow_mut().disable_native_cmd_tab = on;
            self.save();
            hotkey::set_disable_native_cmd_tab(on);
        }

        #[unsafe(method(toggleDock:))]
        fn action_toggle_dock(&self, sender: Option<&AnyObject>) {
            let on = checkbox_is_on(sender);
            self.ivars().settings.borrow_mut().show_in_dock = on;
            self.save();
            self.apply_dock_visibility();
        }

        #[unsafe(method(toggleMenuBar:))]
        fn action_toggle_menu_bar(&self, sender: Option<&AnyObject>) {
            let on = checkbox_is_on(sender);
            self.ivars().settings.borrow_mut().show_in_menu_bar = on;
            self.save();
            self.apply_menu_bar_visibility();
        }

        #[unsafe(method(toggleLaunchAtLogin:))]
        fn action_toggle_launch_at_login(&self, sender: Option<&AnyObject>) {
            let on = checkbox_is_on(sender);
            self.ivars().settings.borrow_mut().launch_at_login = on;
            self.save();
            crate::login::set_launch_at_login(on);
        }

        #[unsafe(method(grantAccessibility:))]
        fn action_grant_accessibility(&self, _sender: Option<&AnyObject>) {
            permissions::ensure_accessibility();
        }

        #[unsafe(method(grantScreenRecording:))]
        fn action_grant_screen_recording(&self, _sender: Option<&AnyObject>) {
            permissions::ensure_screen_recording();
        }
    }

    // SAFETY: NSObjectProtocol n'a pas d'exigence de sûreté.
    unsafe impl NSObjectProtocol for AppController {}

    // SAFETY: NSApplicationDelegate n'a pas de méthode requise.
    unsafe impl NSApplicationDelegate for AppController {
        // Clic sur l'icône du Dock (sans fenêtre visible) → rouvre les préférences.
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn application_should_handle_reopen(
            &self,
            _app: &NSApplication,
            _has_visible_windows: bool,
        ) -> bool {
            self.show_preferences();
            true
        }

        // Au retour au premier plan (ex. après avoir accordé une permission dans
        // Réglages Système), rafraîchit les statuts si les préférences sont ouvertes.
        #[unsafe(method(applicationDidBecomeActive:))]
        fn application_did_become_active(&self, _notification: &NSNotification) {
            self.refresh_preferences_if_open();
        }
    }
);

impl AppController {
    pub fn new(mtm: MainThreadMarker, settings: Settings) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            mtm,
            settings: RefCell::new(settings),
            prefs_window: RefCell::new(None),
            status_item: RefCell::new(None),
            tiles: RefCell::new(Vec::new()),
        });
        // SAFETY: init de NSObject.
        unsafe { msg_send![super(this), init] }
    }

    /// Applique les réglages au démarrage (Dock + barre des menus).
    pub fn apply_initial(&self) {
        self.apply_dock_visibility();
        self.apply_menu_bar_visibility();
    }

    fn save(&self) {
        config::save(&self.ivars().settings.borrow());
    }

    fn select_mode(&self, mode: DisplayMode) {
        self.ivars().settings.borrow_mut().mode = mode;
        self.save();
        hotkey::set_mode(mode);
        for (m, box_) in self.ivars().tiles.borrow().iter() {
            apply_tile_border(box_, *m == mode);
        }
    }

    fn apply_dock_visibility(&self) {
        let policy = if self.ivars().settings.borrow().show_in_dock {
            NSApplicationActivationPolicy::Regular
        } else {
            NSApplicationActivationPolicy::Accessory
        };
        NSApplication::sharedApplication(self.ivars().mtm).setActivationPolicy(policy);
    }

    fn apply_menu_bar_visibility(&self) {
        let show = self.ivars().settings.borrow().show_in_menu_bar;
        let mtm = self.ivars().mtm;
        let mut slot = self.ivars().status_item.borrow_mut();
        match (show, slot.is_some()) {
            (true, false) => {
                let item =
                    NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
                if let Some(button) = item.button(mtm) {
                    button.setTitle(&NSString::from_str("⇥"));
                }
                item.setMenu(Some(&self.build_menu()));
                *slot = Some(item);
            }
            (false, true) => {
                if let Some(item) = slot.take() {
                    NSStatusBar::systemStatusBar().removeStatusItem(&item);
                }
            }
            _ => {}
        }
    }

    fn build_menu(&self) -> Retained<NSMenu> {
        let mtm = self.ivars().mtm;
        let menu = NSMenu::new(mtm);
        menu.addItem(&menu_item(mtm, "Préférences…", sel!(showPreferences:), self));
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&menu_item(mtm, "Quitter Tabs", sel!(quitApp:), self));
        menu
    }

    /// Installe un menu principal minimal (menu application) pour que Cmd-Q
    /// quitte Tabs quand l'application est active (fenêtre de préférences).
    pub fn install_main_menu(&self) {
        let mtm = self.ivars().mtm;
        let main = NSMenu::new(mtm);
        let app_item = NSMenuItem::new(mtm);
        main.addItem(&app_item);

        let submenu = NSMenu::new(mtm);
        let quit = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str("Quitter Tabs"),
                Some(sel!(quitApp:)),
                &NSString::from_str("q"),
            )
        };
        let target: &AnyObject = self;
        unsafe { quit.setTarget(Some(target)) };
        submenu.addItem(&quit);
        app_item.setSubmenu(Some(&submenu));

        NSApplication::sharedApplication(mtm).setMainMenu(Some(&main));
    }

    /// Affiche la fenêtre de préférences (reconstruite à chaque ouverture pour
    /// refléter l'état courant : sélection, statuts de permissions).
    pub fn show_preferences(&self) {
        NSApplication::sharedApplication(self.ivars().mtm).activate();
        self.present_preferences();
    }

    /// Reconstruit et réaffiche la fenêtre de préférences si elle est déjà
    /// ouverte (sans réactiver l'app) — pour rafraîchir les statuts.
    fn refresh_preferences_if_open(&self) {
        let visible = self
            .ivars()
            .prefs_window
            .borrow()
            .as_ref()
            .map(|w| w.isVisible())
            .unwrap_or(false);
        if visible {
            self.present_preferences();
        }
    }

    /// Construit une fenêtre de préférences fraîche et l'affiche (en fermant la
    /// précédente).
    fn present_preferences(&self) {
        if let Some(old) = self.ivars().prefs_window.borrow_mut().take() {
            old.orderOut(None);
        }
        let window = self.build_preferences_window();
        window.makeKeyAndOrderFront(None);
        *self.ivars().prefs_window.borrow_mut() = Some(window);
    }

    fn build_preferences_window(&self) -> Retained<NSWindow> {
        let mtm = self.ivars().mtm;
        let settings = self.ivars().settings.borrow().clone();
        self.ivars().tiles.borrow_mut().clear();

        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIN_W, WIN_H)),
                NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&NSString::from_str("Préférences Tabs"));
        window.center();
        let content = window.contentView().expect("la fenêtre a une vue");

        // En-tête : logo + titre.
        if let Some(icon) = NSApplication::sharedApplication(mtm).applicationIconImage() {
            let view = NSImageView::imageViewWithImage(&icon, mtm);
            view.setFrame(rect(24.0, WIN_H - 82.0, 56.0, 56.0));
            content.addSubview(&view);
        }
        let title = label(mtm, "Tabs", rect(92.0, WIN_H - 60.0, 300.0, 28.0));
        title.setFont(Some(&NSFont::boldSystemFontOfSize(22.0)));
        content.addSubview(&title);
        content.addSubview(&label(
            mtm,
            "Commutateur de fenêtres",
            rect(92.0, WIN_H - 80.0, 300.0, 18.0),
        ));

        // Trait de séparation sous l'en-tête.
        let separator = make_box(mtm, rect(24.0, WIN_H - 96.0, WIN_W - 48.0, 1.0), 0.0);
        separator.setFillColor(&NSColor::colorWithCalibratedWhite_alpha(0.5, 0.28));
        content.addSubview(&separator);

        // Section « Aperçu des onglets ».
        let mut y = WIN_H - 132.0;
        content.addSubview(&section(mtm, "Aperçu des onglets", rect(24.0, y, 400.0, 18.0)));
        y -= 122.0;
        let tile_w = 132.0;
        let modes = [
            (DisplayMode::Thumbnails, "preview_thumbnails", "Miniatures", sel!(selectThumbnails:)),
            (DisplayMode::AppIcons, "preview_appicons", "Icônes d'app", sel!(selectAppIcons:)),
            (DisplayMode::Titles, "preview_titles", "Titres", sel!(selectTitles:)),
        ];
        for (i, (mode, image, label_text, action)) in modes.iter().enumerate() {
            let x = 24.0 + (i as f64) * (tile_w + 11.0);
            self.add_tile(
                &content,
                *mode,
                image,
                label_text,
                *action,
                rect(x, y, tile_w, 110.0),
                *mode == settings.mode,
            );
        }

        // Section « Déclencheur ».
        y -= 44.0;
        content.addSubview(&section(mtm, "Déclencheur", rect(24.0, y, 400.0, 18.0)));
        y -= 34.0;
        content.addSubview(&label(
            mtm,
            "Touche maintenue (puis Tab) :",
            rect(24.0, y, 220.0, 22.0),
        ));
        let trigger_idx = match settings.trigger {
            TriggerModifier::Option => 0,
            TriggerModifier::Command => 1,
            TriggerModifier::Control => 2,
        };
        let trigger_popup = popup(
            mtm,
            &["⌥ Option", "⌘ Command", "⌃ Control"],
            trigger_idx,
            sel!(triggerChanged:),
            self,
            rect(250.0, y - 2.0, 180.0, 26.0),
        );
        content.addSubview(&trigger_popup);

        y -= 36.0;
        content.addSubview(&checkbox(
            mtm,
            "Désactiver le Cmd-Tab du système",
            sel!(toggleDisableCmdTab:),
            self,
            settings.disable_native_cmd_tab,
            rect(24.0, y, 410.0, 22.0),
        ));

        // Section « Apparence dans le système ».
        y -= 44.0;
        content.addSubview(&section(mtm, "Apparence dans le système", rect(24.0, y, 400.0, 18.0)));
        y -= 32.0;
        content.addSubview(&checkbox(
            mtm,
            "Afficher dans le Dock",
            sel!(toggleDock:),
            self,
            settings.show_in_dock,
            rect(24.0, y, 410.0, 22.0),
        ));
        y -= 30.0;
        content.addSubview(&checkbox(
            mtm,
            "Afficher dans la barre des menus",
            sel!(toggleMenuBar:),
            self,
            settings.show_in_menu_bar,
            rect(24.0, y, 410.0, 22.0),
        ));
        y -= 30.0;
        content.addSubview(&checkbox(
            mtm,
            "Lancer au démarrage",
            sel!(toggleLaunchAtLogin:),
            self,
            settings.launch_at_login,
            rect(24.0, y, 410.0, 22.0),
        ));

        // Section « Permissions ».
        y -= 44.0;
        content.addSubview(&section(mtm, "Permissions", rect(24.0, y, 400.0, 18.0)));
        y -= 30.0;
        self.add_permission_row(
            &content,
            "Accessibilité",
            permissions::is_accessibility_granted(),
            sel!(grantAccessibility:),
            y,
        );
        y -= 30.0;
        self.add_permission_row(
            &content,
            "Enregistrement de l'écran",
            permissions::is_screen_recording_granted(),
            sel!(grantScreenRecording:),
            y,
        );

        // Pied : astuce + quitter.
        let hint = label(
            mtm,
            "Maintiens le modificateur + Tab · « m » mode · « q » quitter l'app · « , » réglages",
            rect(24.0, 62.0, WIN_W - 48.0, 16.0),
        );
        hint.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        hint.setTextColor(Some(&NSColor::secondaryLabelColor()));
        content.addSubview(&hint);
        content.addSubview(&button(
            mtm,
            "Quitter Tabs",
            sel!(quitApp:),
            self,
            rect(24.0, 20.0, WIN_W - 48.0, 30.0),
        ));

        window
    }

    /// Ajoute une tuile d'aperçu sélectionnable (image + libellé) et enregistre
    /// sa boîte de surbrillance.
    fn add_tile(
        &self,
        content: &NSView,
        mode: DisplayMode,
        image_name: &str,
        title: &str,
        action: Sel,
        frame: NSRect,
        selected: bool,
    ) {
        let mtm = self.ivars().mtm;

        // Carte derrière la tuile : fond léger + contour (accent si sélectionné).
        let box_ = make_box(mtm, frame, 14.0);
        box_.setFillColor(&NSColor::colorWithCalibratedWhite_alpha(0.5, 0.08));
        apply_tile_border(&box_, selected);
        content.addSubview(&box_);

        // Bouton-image cliquable (l'aperçu).
        let target: &AnyObject = self;
        let button = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(""),
                Some(target),
                Some(action),
                mtm,
            )
        };
        button.setBordered(false);
        button.setImagePosition(NSCellImagePosition::ImageOnly);
        button.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        if let Some(image) = NSImage::imageNamed(&NSString::from_str(image_name)) {
            button.setImage(Some(&image));
        }
        button.setFrame(rect(
            frame.origin.x + 8.0,
            frame.origin.y + 26.0,
            frame.size.width - 16.0,
            76.0,
        ));
        content.addSubview(&button);

        let lbl = label(
            mtm,
            title,
            rect(frame.origin.x, frame.origin.y + 4.0, frame.size.width, 18.0),
        );
        lbl.setAlignment(objc2_app_kit::NSTextAlignment::Center);
        content.addSubview(&lbl);

        self.ivars().tiles.borrow_mut().push((mode, box_));
    }

    /// Ajoute une ligne de permission : nom, statut, et bouton « Autoriser »
    /// uniquement si la permission n'est pas déjà accordée.
    fn add_permission_row(
        &self,
        content: &NSView,
        name: &str,
        granted: bool,
        action: Sel,
        y: f64,
    ) {
        let mtm = self.ivars().mtm;
        let status = if granted {
            format!("✅ {name} — accordée")
        } else {
            format!("⚠️ {name} — non accordée")
        };
        content.addSubview(&label(mtm, &status, rect(24.0, y, 300.0, 22.0)));
        if !granted {
            content.addSubview(&button(mtm, "Autoriser", action, self, rect(330.0, y - 4.0, 104.0, 26.0)));
        }
    }
}

// ----- Helpers (création de vues / valeurs) -------------------------------

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

/// Applique le contour d'une tuile selon qu'elle est sélectionnée (accent épais)
/// ou non (contour discret).
fn apply_tile_border(box_: &NSBox, selected: bool) {
    if selected {
        box_.setBorderColor(&NSColor::controlAccentColor());
        box_.setBorderWidth(3.0);
    } else {
        box_.setBorderColor(&NSColor::colorWithCalibratedWhite_alpha(0.5, 0.30));
        box_.setBorderWidth(1.0);
    }
}

fn checkbox_is_on(sender: Option<&AnyObject>) -> bool {
    sender
        .and_then(|s| s.downcast_ref::<NSButton>())
        .map(|b| b.state() == NSControlStateValueOn)
        .unwrap_or(false)
}

fn label(mtm: MainThreadMarker, text: &str, frame: NSRect) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFrame(frame);
    label
}

fn section(mtm: MainThreadMarker, text: &str, frame: NSRect) -> Retained<NSTextField> {
    let label = label(mtm, text, frame);
    label.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    label
}

fn make_box(mtm: MainThreadMarker, frame: NSRect, corner_radius: f64) -> Retained<NSBox> {
    // SAFETY: init de NSBox.
    let boxed: Retained<NSBox> = unsafe { msg_send![NSBox::alloc(mtm), init] };
    boxed.setBoxType(NSBoxType::Custom);
    boxed.setTitlePosition(NSTitlePosition::NoTitle);
    boxed.setBorderWidth(0.0);
    boxed.setFillColor(&NSColor::clearColor());
    boxed.setCornerRadius(corner_radius);
    boxed.setFrame(frame);
    boxed
}

fn button(
    mtm: MainThreadMarker,
    title: &str,
    action: Sel,
    target: &AppController,
    frame: NSRect,
) -> Retained<NSButton> {
    let target_obj: &AnyObject = target;
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(title),
            Some(target_obj),
            Some(action),
            mtm,
        )
    };
    button.setFrame(frame);
    button
}

fn checkbox(
    mtm: MainThreadMarker,
    title: &str,
    action: Sel,
    target: &AppController,
    on: bool,
    frame: NSRect,
) -> Retained<NSButton> {
    let target_obj: &AnyObject = target;
    let button = unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str(title),
            Some(target_obj),
            Some(action),
            mtm,
        )
    };
    if on {
        button.setState(NSControlStateValueOn);
    }
    button.setFrame(frame);
    button
}

fn popup(
    mtm: MainThreadMarker,
    items: &[&str],
    selected: isize,
    action: Sel,
    target: &AppController,
    frame: NSRect,
) -> Retained<NSPopUpButton> {
    let popup = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), frame, false);
    for item in items {
        popup.addItemWithTitle(&NSString::from_str(item));
    }
    popup.selectItemAtIndex(selected);
    let target_obj: &AnyObject = target;
    unsafe {
        popup.setTarget(Some(target_obj));
        popup.setAction(Some(action));
    }
    popup
}

fn menu_item(
    mtm: MainThreadMarker,
    title: &str,
    action: Sel,
    target: &AppController,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            &NSString::from_str(""),
        )
    };
    let target_obj: &AnyObject = target;
    unsafe { item.setTarget(Some(target_obj)) };
    item
}
