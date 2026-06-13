//! Overlay du sélecteur : un `NSPanel` non-activant, flottant au-dessus de
//! toutes les applications et de tous les Spaces, qui affiche une rangée de
//! cellules (icône d'application + titre) avec surbrillance de la sélection.
//!
//! Le panneau est *non-activant* : l'afficher ne vole pas le focus à
//! l'application en cours, ce qui est essentiel pour un commutateur de fenêtres.
//!
//! Les vues sont reconstruites à chaque ouverture (`show`) ; pendant le cycle,
//! `select` se contente de déplacer la surbrillance.

use objc2::rc::Retained;
use objc2::{msg_send, AllocAnyThread, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSBox, NSBoxType, NSColor, NSFont, NSImage, NSImageScaling, NSImageView,
    NSPanel, NSPopUpMenuWindowLevel, NSRunningApplication, NSScreen, NSTextAlignment, NSTextField,
    NSTitlePosition, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSPoint, NSRect, NSSize, NSString};

use super::layout::{self, Rect};
use crate::windows::{self, Window};

pub struct Overlay {
    mtm: MainThreadMarker,
    panel: Retained<NSPanel>,
    /// Boîte de surbrillance, recréée à chaque `show`.
    selection: Option<Retained<NSBox>>,
    /// Rectangles de surbrillance par index, pour `select`.
    sel_frames: Vec<Rect>,
}

impl Overlay {
    /// Crée et configure le panneau (sans l'afficher).
    pub fn new(mtm: MainThreadMarker) -> Self {
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(200.0, 120.0)),
            NSWindowStyleMask::NonactivatingPanel | NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        );

        // SAFETY: on ne ferme jamais le panneau (on l'ordonne/désordonne) ;
        // désactiver l'auto-release est requis hors window controller.
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setOpaque(false);
        let clear = NSColor::clearColor();
        panel.setBackgroundColor(Some(&clear));
        panel.setHasShadow(true);
        panel.setLevel(NSPopUpMenuWindowLevel);
        panel.setFloatingPanel(true);
        // Visible sur tous les Spaces, y compris au-dessus des apps en plein
        // écran, et sans suivre les changements de Space.
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );

        Self {
            mtm,
            panel,
            selection: None,
            sel_frames: Vec::new(),
        }
    }

    /// Affiche l'overlay pour `windows`, avec l'élément `selected` en évidence.
    pub fn show(&mut self, windows: &[Window], selected: usize) {
        let mtm = self.mtm;
        let lay = layout::compute(windows.len());

        // Dimensionne et centre le panneau sur l'écran principal.
        self.panel
            .setContentSize(NSSize::new(lay.width, lay.height));
        if let Some(screen) = NSScreen::mainScreen(mtm) {
            let f = screen.frame();
            let x = f.origin.x + (f.size.width - lay.width) / 2.0;
            let y = f.origin.y + (f.size.height - lay.height) / 2.0;
            self.panel.setFrameOrigin(NSPoint::new(x, y));
        }

        let content = self.panel.contentView().expect("le panneau a une vue");

        // Repart d'une vue vide.
        content.setSubviews(&NSArray::new());

        // Fond translucide arrondi.
        let bg_fill = NSColor::colorWithCalibratedWhite_alpha(0.14, 0.92);
        let bg = make_box(
            mtm,
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(lay.width, lay.height)),
            &bg_fill,
            16.0,
        );
        content.addSubview(&bg);

        // Surbrillance de la sélection (placée derrière les cellules).
        self.sel_frames = lay.cells.iter().map(|c| c.selection).collect();
        let sel_fill = NSColor::selectedContentBackgroundColor();
        let sel = make_box(mtm, NSRect::ZERO, &sel_fill, 10.0);
        match lay.cells.get(selected) {
            Some(c) => sel.setFrame(to_nsrect(c.selection)),
            None => sel.setHidden(true),
        }
        content.addSubview(&sel);
        self.selection = Some(sel);

        // Cellules : aperçu (miniature, ou icône d'app en repli) + titre.
        for (i, w) in windows.iter().enumerate() {
            let cf = lay.cells[i];

            if let Some(image) = thumbnail_or_icon(w) {
                let view = NSImageView::imageViewWithImage(&image, mtm);
                view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
                view.setFrame(to_nsrect(cf.image));
                content.addSubview(&view);
            }

            let text = if w.title.is_empty() {
                w.app_name.as_str()
            } else {
                w.title.as_str()
            };
            let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
            label.setAlignment(NSTextAlignment::Center);
            label.setTextColor(Some(&NSColor::labelColor()));
            label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
            label.setFrame(to_nsrect(cf.title));
            content.addSubview(&label);
        }

        // Affiche sans voler le focus à l'application active.
        self.panel.orderFrontRegardless();
    }

    /// Déplace la surbrillance sur l'élément `selected` (overlay déjà visible).
    pub fn select(&mut self, selected: usize) {
        if let (Some(sel), Some(frame)) = (self.selection.as_ref(), self.sel_frames.get(selected)) {
            sel.setHidden(false);
            sel.setFrame(to_nsrect(*frame));
        }
    }

    /// Masque l'overlay.
    pub fn hide(&self) {
        self.panel.orderOut(None);
    }
}

/// Renvoie l'aperçu d'une fenêtre : sa miniature si la capture est possible,
/// sinon l'icône de son application.
fn thumbnail_or_icon(w: &Window) -> Option<Retained<NSImage>> {
    if let Some(cg) = windows::capture::capture(w.id) {
        // NSSize(0,0) → l'image conserve sa taille en pixels, l'NSImageView
        // la met ensuite à l'échelle dans son cadre.
        return Some(NSImage::initWithCGImage_size(
            NSImage::alloc(),
            &cg,
            NSSize::new(0.0, 0.0),
        ));
    }
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(w.pid)?;
    app.icon()
}

/// Convertit un [`Rect`] de disposition en `NSRect`.
fn to_nsrect(r: Rect) -> NSRect {
    NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h))
}

/// Crée une `NSBox` sans titre ni bordure, au fond plein et aux coins arrondis.
fn make_box(
    mtm: MainThreadMarker,
    frame: NSRect,
    fill: &NSColor,
    corner_radius: f64,
) -> Retained<NSBox> {
    // SAFETY: `init` est la méthode d'initialisation correcte de NSBox.
    let boxed: Retained<NSBox> = unsafe { msg_send![NSBox::alloc(mtm), init] };
    boxed.setBoxType(NSBoxType::Custom);
    boxed.setTitlePosition(NSTitlePosition::NoTitle);
    boxed.setBorderWidth(0.0);
    boxed.setFillColor(fill);
    boxed.setCornerRadius(corner_radius);
    boxed.setFrame(frame);
    boxed
}
