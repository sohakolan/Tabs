//! Overlay du sélecteur : un `NSPanel` non-activant, flottant au-dessus de
//! toutes les applications et de tous les Spaces, qui affiche une rangée de
//! cellules (icône d'application + titre) avec surbrillance de la sélection.
//!
//! Le panneau est *non-activant* : l'afficher ne vole pas le focus à
//! l'application en cours, ce qui est essentiel pour un commutateur de fenêtres.
//!
//! Les vues sont reconstruites à chaque ouverture (`show`) ; pendant le cycle,
//! `select` se contente de déplacer la surbrillance.

use core::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSBox, NSBoxType, NSColor, NSEvent, NSFont, NSImage, NSImageScaling,
    NSImageView, NSPanel, NSPopUpMenuWindowLevel, NSRunningApplication, NSScreen, NSTextAlignment,
    NSTextField, NSTitlePosition, NSTrackingArea, NSTrackingAreaOptions, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSPoint, NSRect, NSSize, NSString};

use super::layout::{self, DisplayMode, Rect};
use crate::windows::{self, Window};

/// Données portées par la vue de contenu de l'overlay.
struct OverlayViewIvars {
    /// Rectangles cliquables/survolables par index (coordonnées de la vue).
    cells: RefCell<Vec<NSRect>>,
    /// Dernier index survolé (-1 = aucun), pour ne notifier qu'aux changements.
    last_hover: Cell<isize>,
    /// Rappel au survol d'une cellule (index).
    on_hover: Box<dyn Fn(usize)>,
    /// Rappel au clic sur une cellule (index).
    on_click: Box<dyn Fn(usize)>,
}

define_class!(
    // SAFETY:
    // - NSView n'impose pas de contrainte de sous-classe particulière.
    // - OverlayView n'implémente pas Drop.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = OverlayViewIvars]
    struct OverlayView;

    impl OverlayView {
        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.handle_hover(event);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.handle_hover(event);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if let Some(i) = self.cell_at(event) {
                (self.ivars().on_click)(i);
            }
        }

        // Permet de recevoir le premier clic même si le panneau n'est pas actif.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }
    }
);

impl OverlayView {
    fn new(
        mtm: MainThreadMarker,
        on_hover: Box<dyn Fn(usize)>,
        on_click: Box<dyn Fn(usize)>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(OverlayViewIvars {
            cells: RefCell::new(Vec::new()),
            last_hover: Cell::new(-1),
            on_hover,
            on_click,
        });
        // SAFETY: init de NSView/NSObject.
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };

        // Zone de suivi de la souris couvrant toute la vue visible.
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                NSRect::ZERO,
                NSTrackingAreaOptions::MouseMoved
                    | NSTrackingAreaOptions::ActiveAlways
                    | NSTrackingAreaOptions::InVisibleRect,
                Some(&this),
                None,
            )
        };
        this.addTrackingArea(&area);
        this
    }

    /// Met à jour les zones cliquables (appelé à chaque ouverture).
    fn set_cells(&self, rects: Vec<NSRect>) {
        *self.ivars().cells.borrow_mut() = rects;
        self.ivars().last_hover.set(-1);
    }

    /// Index de la cellule sous le curseur pour un évènement, le cas échéant.
    fn cell_at(&self, event: &NSEvent) -> Option<usize> {
        let window_point = event.locationInWindow();
        let p = self.convertPoint_fromView(window_point, None);
        self.ivars()
            .cells
            .borrow()
            .iter()
            .position(|r| point_in(*r, p))
    }

    /// Notifie le survol si la cellule sous le curseur a changé.
    fn handle_hover(&self, event: &NSEvent) {
        if let Some(i) = self.cell_at(event) {
            if self.ivars().last_hover.get() != i as isize {
                self.ivars().last_hover.set(i as isize);
                (self.ivars().on_hover)(i);
            }
        }
    }
}

fn point_in(r: NSRect, p: NSPoint) -> bool {
    p.x >= r.origin.x
        && p.x < r.origin.x + r.size.width
        && p.y >= r.origin.y
        && p.y < r.origin.y + r.size.height
}

pub struct Overlay {
    mtm: MainThreadMarker,
    panel: Retained<NSPanel>,
    /// Vue de contenu personnalisée (reçoit les évènements souris).
    view: Retained<OverlayView>,
    /// Boîte de surbrillance, recréée à chaque `show`.
    selection: Option<Retained<NSBox>>,
    /// Rectangles de surbrillance par index, pour `select`.
    sel_frames: Vec<Rect>,
}

impl Overlay {
    /// Crée et configure le panneau (sans l'afficher). `on_hover`/`on_click`
    /// sont appelés (avec l'index de cellule) au survol et au clic souris.
    pub fn new(
        mtm: MainThreadMarker,
        on_hover: Box<dyn Fn(usize)>,
        on_click: Box<dyn Fn(usize)>,
    ) -> Self {
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
        // Nécessaire pour recevoir les évènements `mouseMoved`.
        panel.setAcceptsMouseMovedEvents(true);

        // Vue de contenu personnalisée qui capte la souris.
        let view = OverlayView::new(mtm, on_hover, on_click);
        panel.setContentView(Some(&view));

        Self {
            mtm,
            panel,
            view,
            selection: None,
            sel_frames: Vec::new(),
        }
    }

    /// Affiche l'overlay pour `windows`, avec l'élément `selected` en évidence,
    /// dans le mode d'affichage demandé.
    pub fn show(&mut self, windows: &[Window], selected: usize, mode: DisplayMode) {
        let mtm = self.mtm;
        let lay = layout::compute(windows.len(), mode);

        // Dimensionne et centre le panneau sur l'écran principal.
        self.panel
            .setContentSize(NSSize::new(lay.width, lay.height));
        if let Some(screen) = NSScreen::mainScreen(mtm) {
            let f = screen.frame();
            let x = f.origin.x + (f.size.width - lay.width) / 2.0;
            let y = f.origin.y + (f.size.height - lay.height) / 2.0;
            self.panel.setFrameOrigin(NSPoint::new(x, y));
        }

        let content = self.view.clone();

        // Renseigne les zones cliquables/survolables pour la souris.
        self.view
            .set_cells(lay.cells.iter().map(|c| to_nsrect(c.hit)).collect());

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

        // Cellules : aperçu (selon le mode) + titre.
        for (i, w) in windows.iter().enumerate() {
            let cf = lay.cells[i];

            let image = match mode {
                DisplayMode::Titles => None,
                DisplayMode::AppIcons => app_icon(w),
                DisplayMode::Thumbnails => thumbnail_or_icon(w),
            };
            if let Some(image) = image {
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
    app_icon(w)
}

/// Icône de l'application propriétaire de la fenêtre.
fn app_icon(w: &Window) -> Option<Retained<NSImage>> {
    NSRunningApplication::runningApplicationWithProcessIdentifier(w.pid)?.icon()
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
