//! Contrôleur d'application : visibilité (Dock / barre des menus), menu de la
//! barre d'état et fenêtre de préférences.
//!
//! Par défaut l'application est invisible (cf. [`crate::config`]). La fenêtre de
//! préférences est joignable par le raccourci `,` (pendant l'overlay) et, si
//! activée, par l'icône de la barre des menus.

use core::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSButton,
    NSControlStateValueOn, NSFont, NSImageView, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
    NSTextField, NSVariableStatusItemLength, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

use crate::config::{self, Settings};
use crate::{hotkey, permissions};

pub(crate) struct Ivars {
    mtm: MainThreadMarker,
    settings: RefCell<Settings>,
    prefs_window: RefCell<Option<Retained<NSWindow>>>,
    status_item: RefCell<Option<Retained<NSStatusItem>>>,
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
            // Restaure le commutateur natif avant de quitter.
            crate::system::set_native_cmd_tab_enabled(true);
            NSApplication::sharedApplication(self.ivars().mtm).terminate(None);
        }

        #[unsafe(method(cycleMode:))]
        fn action_cycle_mode(&self, sender: Option<&AnyObject>) {
            let next = {
                let mut s = self.ivars().settings.borrow_mut();
                s.mode = s.mode.next();
                s.mode
            };
            self.save();
            hotkey::set_mode(next);
            if let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) {
                button.setTitle(&NSString::from_str(mode_label(next)));
            }
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

        #[unsafe(method(toggleReplaceCmdTab:))]
        fn action_toggle_replace_cmd_tab(&self, sender: Option<&AnyObject>) {
            let on = checkbox_is_on(sender);
            self.ivars().settings.borrow_mut().replace_cmd_tab = on;
            self.save();
            hotkey::set_replace_cmd_tab(on);
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
);

impl AppController {
    pub fn new(mtm: MainThreadMarker, settings: Settings) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            mtm,
            settings: RefCell::new(settings),
            prefs_window: RefCell::new(None),
            status_item: RefCell::new(None),
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
                let item = NSStatusBar::systemStatusBar()
                    .statusItemWithLength(NSVariableStatusItemLength);
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

        let prefs = menu_item(mtm, "Préférences…", sel!(showPreferences:), self);
        menu.addItem(&prefs);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let quit = menu_item(mtm, "Quitter Tabs", sel!(quitApp:), self);
        menu.addItem(&quit);
        menu
    }

    /// Affiche la fenêtre de préférences (la construit à la première demande).
    pub fn show_preferences(&self) {
        if self.ivars().prefs_window.borrow().is_none() {
            let window = self.build_preferences_window();
            *self.ivars().prefs_window.borrow_mut() = Some(window);
        }
        let app = NSApplication::sharedApplication(self.ivars().mtm);
        app.activate();
        if let Some(window) = self.ivars().prefs_window.borrow().as_ref() {
            window.makeKeyAndOrderFront(None);
        }
    }

    fn build_preferences_window(&self) -> Retained<NSWindow> {
        let mtm = self.ivars().mtm;
        let settings = self.ivars().settings.borrow().clone();

        let width = 360.0;
        let height = 400.0;
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height)),
                NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(&NSString::from_str("Préférences Tabs"));
        window.center();

        let content = window.contentView().expect("la fenêtre a une vue");

        // En-tête : icône de l'application + titre.
        if let Some(icon) = NSApplication::sharedApplication(mtm).applicationIconImage() {
            let icon_view = NSImageView::imageViewWithImage(&icon, mtm);
            icon_view.setFrame(NSRect::new(
                NSPoint::new(24.0, height - 96.0),
                NSSize::new(64.0, 64.0),
            ));
            content.addSubview(&icon_view);
        }
        let title = make_label(mtm, "Tabs", NSRect::new(
            NSPoint::new(100.0, height - 74.0),
            NSSize::new(220.0, 28.0),
        ));
        title.setFont(Some(&NSFont::boldSystemFontOfSize(22.0)));
        content.addSubview(&title);
        let subtitle = make_label(mtm, "Préférences", NSRect::new(
            NSPoint::new(100.0, height - 96.0),
            NSSize::new(220.0, 20.0),
        ));
        content.addSubview(&subtitle);

        // Empilement vertical (origine bas-gauche).
        let mut y = height - 150.0;
        let label = make_label(mtm, "Mode d'affichage :", NSRect::new(
            NSPoint::new(20.0, y),
            NSSize::new(150.0, 20.0),
        ));
        content.addSubview(&label);
        let mode_btn = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(mode_label(settings.mode)),
                Some(self),
                Some(sel!(cycleMode:)),
                mtm,
            )
        };
        mode_btn.setFrame(NSRect::new(NSPoint::new(180.0, y - 4.0), NSSize::new(160.0, 28.0)));
        content.addSubview(&mode_btn);

        y -= 48.0;
        let dock = make_checkbox(mtm, "Afficher dans le Dock", sel!(toggleDock:), self,
            settings.show_in_dock, NSRect::new(NSPoint::new(20.0, y), NSSize::new(320.0, 22.0)));
        content.addSubview(&dock);

        y -= 32.0;
        let menubar = make_checkbox(mtm, "Afficher dans la barre des menus", sel!(toggleMenuBar:),
            self, settings.show_in_menu_bar,
            NSRect::new(NSPoint::new(20.0, y), NSSize::new(320.0, 22.0)));
        content.addSubview(&menubar);

        y -= 32.0;
        let cmd_tab = make_checkbox(mtm, "Remplacer le Cmd-Tab du système",
            sel!(toggleReplaceCmdTab:), self, settings.replace_cmd_tab,
            NSRect::new(NSPoint::new(20.0, y), NSSize::new(320.0, 22.0)));
        content.addSubview(&cmd_tab);

        y -= 48.0;
        let ax = make_button(mtm, "Autoriser l'Accessibilité", sel!(grantAccessibility:), self,
            NSRect::new(NSPoint::new(20.0, y), NSSize::new(320.0, 28.0)));
        content.addSubview(&ax);

        y -= 36.0;
        let screen = make_button(mtm, "Autoriser l'Enregistrement de l'écran",
            sel!(grantScreenRecording:), self,
            NSRect::new(NSPoint::new(20.0, y), NSSize::new(320.0, 28.0)));
        content.addSubview(&screen);

        y -= 44.0;
        let quit = make_button(mtm, "Quitter Tabs", sel!(quitApp:), self,
            NSRect::new(NSPoint::new(20.0, y), NSSize::new(320.0, 28.0)));
        content.addSubview(&quit);

        window
    }
}

/// Libellé lisible d'un mode d'affichage.
fn mode_label(mode: crate::ui::DisplayMode) -> &'static str {
    use crate::ui::DisplayMode::*;
    match mode {
        Thumbnails => "Miniatures",
        AppIcons => "Icônes d'app",
        Titles => "Titres",
    }
}

fn checkbox_is_on(sender: Option<&AnyObject>) -> bool {
    sender
        .and_then(|s| s.downcast_ref::<NSButton>())
        .map(|b| b.state() == NSControlStateValueOn)
        .unwrap_or(false)
}

fn menu_item(
    mtm: MainThreadMarker,
    title: &str,
    action: objc2::runtime::Sel,
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
    let target: &AnyObject = target;
    unsafe { item.setTarget(Some(target)) };
    item
}

fn make_label(mtm: MainThreadMarker, text: &str, frame: NSRect) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFrame(frame);
    label
}

fn make_checkbox(
    mtm: MainThreadMarker,
    title: &str,
    action: objc2::runtime::Sel,
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

fn make_button(
    mtm: MainThreadMarker,
    title: &str,
    action: objc2::runtime::Sel,
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
