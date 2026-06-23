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
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSBackingStoreType, NSBox,
    NSColor, NSEvent, NSFont, NSImage, NSImageScaling,
    NSImageView, NSPanel, NSPopUpMenuWindowLevel, NSScreen, NSTextAlignment,
    NSTextField, NSTrackingArea, NSTrackingAreaOptions, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_graphics::CGImage;
use objc2_foundation::{MainThreadMarker, NSArray, NSPoint, NSRect, NSSize, NSString};

use super::layout::{self, Direction, DisplayMode, Rect};
use crate::windows::{self, Window, WindowId};

/// Déplacement souris (en points) requis avant que le survol reprenne la main
/// après l'ouverture de l'overlay. Évite qu'un curseur immobile sous l'overlay
/// ne détourne la sélection clavier, tout en absorbant le jitter du trackpad.
const HOVER_MOVE_THRESHOLD: f64 = 6.0;

/// Données portées par la vue de contenu de l'overlay.
struct OverlayViewIvars {
    /// Rectangles cliquables/survolables par index (coordonnées de la vue).
    cells: RefCell<Vec<NSRect>>,
    /// Dernier index survolé (-1 = aucun), pour ne notifier qu'aux changements.
    last_hover: Cell<isize>,
    /// Position écran de la souris au moment de l'ouverture. Le survol n'est
    /// honoré qu'après un déplacement réel depuis cet ancrage, sinon l'overlay
    /// apparaissant sous un curseur immobile détournerait la sélection clavier.
    anchor: Cell<NSPoint>,
    /// `true` une fois la souris déplacée depuis l'ouverture (survol actif).
    armed: Cell<bool>,
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
            anchor: Cell::new(NSPoint::ZERO),
            armed: Cell::new(false),
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
        // Désarme le survol et ancre la position courante de la souris : il
        // faudra un déplacement réel pour que le survol reprenne la main.
        self.ivars().armed.set(false);
        self.ivars().anchor.set(NSEvent::mouseLocation());
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

    /// Notifie le survol si la cellule sous le curseur a changé. Ignoré tant que
    /// la souris n'a pas réellement bougé depuis l'ouverture de l'overlay.
    fn handle_hover(&self, event: &NSEvent) {
        if !self.armed_after_move() {
            return;
        }
        if let Some(i) = self.cell_at(event) {
            if self.ivars().last_hover.get() != i as isize {
                self.ivars().last_hover.set(i as isize);
                (self.ivars().on_hover)(i);
            }
        }
    }

    /// `true` une fois que la souris s'est éloignée de l'ancrage capturé à
    /// l'ouverture (seuil anti-jitter). Le survol reste alors actif.
    fn armed_after_move(&self) -> bool {
        if self.ivars().armed.get() {
            return true;
        }
        let now = NSEvent::mouseLocation();
        let anchor = self.ivars().anchor.get();
        let moved = (now.x - anchor.x).hypot(now.y - anchor.y) > HOVER_MOVE_THRESHOLD;
        if moved {
            self.ivars().armed.set(true);
        }
        moved
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
    /// Fond en verre dépoli, persistant entre les ouvertures (sa recréation à
    /// chaque `show` était coûteuse et provoquait une latence visible).
    glass: Retained<NSVisualEffectView>,
    /// Boîte de surbrillance, persistante : on ne fait que la repositionner.
    selection: Retained<NSBox>,
    /// Conteneur des cellules, persistant : seules ses sous-vues sont
    /// reconstruites à chaque `show` (le verre et la surbrillance, eux, restent).
    cells: Retained<NSView>,
    /// Rectangles de surbrillance par index, pour `select`.
    sel_frames: Vec<Rect>,
    /// Génération de l'affichage, incrémentée à chaque `show`/`hide`. Une
    /// miniature capturée en arrière-plan n'est posée que si la génération n'a
    /// pas changé depuis le lancement de sa capture (sinon elle est périmée).
    generation: Cell<u64>,
    /// Vues d'image des cellules en mode Miniatures, indexées par id de fenêtre :
    /// cibles du remplissage progressif (placeholder initial = icône d'app).
    thumb_views: RefCell<Vec<(WindowId, Retained<NSImageView>)>>,
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
        // Apparence sombre : contraste propre sur le verre dépoli.
        let dark = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua });
        panel.setAppearance(dark.as_deref());

        // Vue de contenu personnalisée qui capte la souris.
        let view = OverlayView::new(mtm, on_hover, on_click);
        panel.setContentView(Some(&view));

        // Pile persistante : verre (au fond), surbrillance, puis conteneur des
        // cellules (au-dessus). Recréer ces vues à chaque ouverture coûtait cher
        // (le NSVisualEffectView surtout) ; on les garde et on les réutilise.
        let glass = make_glass(mtm, NSRect::ZERO);
        view.addSubview(&glass);

        let selection = super::make_box(mtm, NSRect::ZERO, 10.0);
        selection.setFillColor(&NSColor::controlAccentColor().colorWithAlphaComponent(0.30));
        selection.setHidden(true);
        view.addSubview(&selection);

        let cells = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
        view.addSubview(&cells);

        Self {
            mtm,
            panel,
            view,
            glass,
            selection,
            cells,
            sel_frames: Vec::new(),
            generation: Cell::new(0),
            thumb_views: RefCell::new(Vec::new()),
        }
    }

    /// Affiche l'overlay pour `windows`, avec l'élément `selected` en évidence,
    /// dans le mode d'affichage demandé et à l'échelle `scale` (1.0 = base). Les
    /// éléments sont repliés sur plusieurs rangées/colonnes pour ne jamais
    /// déborder de l'écran visible.
    pub fn show(&mut self, windows: &[Window], selected: usize, mode: DisplayMode, scale: f64) {
        let mtm = self.mtm;

        // Écran principal résolu une seule fois : sert à la fois à borner le repli
        // (zone visible) et à centrer le panneau (cadre complet).
        let screen = NSScreen::mainScreen(mtm);

        // Zone disponible (hors barre des menus et Dock) : borne le repli pour
        // que l'overlay reste entièrement à l'écran, même très agrandi.
        let (max_w, max_h) = match &screen {
            Some(s) => {
                let vf = s.visibleFrame();
                (vf.size.width * 0.96, vf.size.height * 0.92)
            }
            None => (1280.0, 800.0),
        };
        let lay = layout::compute(windows.len(), mode, scale, max_w, max_h);

        // Dimensionne et centre le panneau sur l'écran principal.
        self.panel
            .setContentSize(NSSize::new(lay.width, lay.height));
        if let Some(s) = &screen {
            let f = s.frame();
            let x = f.origin.x + (f.size.width - lay.width) / 2.0;
            let y = f.origin.y + (f.size.height - lay.height) / 2.0;
            self.panel.setFrameOrigin(NSPoint::new(x, y));
        }

        let full = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(lay.width, lay.height));

        // Renseigne les zones cliquables/survolables pour la souris.
        self.view
            .set_cells(lay.cells.iter().map(|c| to_nsrect(c.hit)).collect());

        // Réutilise le verre persistant : on le redimensionne et on met son
        // arrondi à l'échelle, sans le recréer (gain de latence à l'ouverture).
        self.glass.setFrame(full);
        if let Some(layer) = self.glass.layer() {
            layer.setCornerRadius(18.0 * scale);
        }

        // Surbrillance de la sélection (persistante, placée derrière les cellules).
        self.sel_frames = lay.cells.iter().map(|c| c.selection).collect();
        self.selection.setCornerRadius(10.0 * scale);
        match lay.cells.get(selected) {
            Some(c) => {
                self.selection.setHidden(false);
                self.selection.setFrame(to_nsrect(c.selection));
            }
            None => self.selection.setHidden(true),
        }

        // Nouvelle génération d'affichage : invalide les miniatures encore en vol
        // pour l'affichage précédent, et réinitialise les cibles de remplissage.
        self.generation.set(self.generation.get().wrapping_add(1));
        let mut thumb_targets: Vec<(WindowId, Retained<NSImageView>)> = Vec::new();
        let thumbnails = matches!(mode, DisplayMode::Thumbnails);

        // Reconstruit uniquement le conteneur des cellules.
        self.cells.setFrame(full);
        self.cells.setSubviews(&NSArray::new());
        let content = self.cells.clone();

        // Icônes d'application mémoïsées par pid pour la durée de ce `show` :
        // plusieurs fenêtres d'une même app partagent une seule résolution, et le
        // mode Miniatures réutilise la même icône pour l'aperçu et la pastille.
        let mut icon_cache: Vec<(i32, Option<Retained<NSImage>>)> = Vec::new();
        let mut icon_for = |pid: i32| -> Option<Retained<NSImage>> {
            if let Some((_, icon)) = icon_cache.iter().find(|(p, _)| *p == pid) {
                return icon.clone();
            }
            let icon = app_icon_for_pid(pid);
            icon_cache.push((pid, icon.clone()));
            icon
        };

        // Cellules : aperçu + titre. L'aperçu est posé tout de suite avec l'icône
        // d'application (rendu instantané) ; en mode Miniatures, la vraie miniature
        // remplacera cette icône dès qu'elle est capturée en arrière-plan
        // (cf. `set_thumbnail` et `hotkey::kick_thumbnails`).
        for (i, w) in windows.iter().enumerate() {
            let cf = lay.cells[i];

            if let Some(image) = icon_for(w.pid) {
                let view = NSImageView::imageViewWithImage(&image, mtm);
                view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
                view.setFrame(to_nsrect(cf.image));
                content.addSubview(&view);
                // Cellule susceptible de recevoir une miniature : on garde sa vue.
                if thumbnails {
                    thumb_targets.push((w.id, view));
                }
            }

            // Pastille d'icône d'app posée sur la miniature (mode Thumbnails).
            if cf.badge.w > 0.0 {
                if let Some(icon) = icon_for(w.pid) {
                    let badge = NSImageView::imageViewWithImage(&icon, mtm);
                    badge.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
                    badge.setFrame(to_nsrect(cf.badge));
                    content.addSubview(&badge);
                }
            }

            let base = if w.title.is_empty() {
                w.app_name.as_str()
            } else {
                w.title.as_str()
            };
            // Préfixe « replié » pour les fenêtres minimisées ; on n'alloue de
            // chaîne que dans ce cas (le cas courant passe le `&str` directement).
            let label = if w.minimized {
                NSTextField::labelWithString(&NSString::from_str(&format!("⤓ {base}")), mtm)
            } else {
                NSTextField::labelWithString(&NSString::from_str(base), mtm)
            };
            label.setAlignment(if matches!(mode, DisplayMode::Titles) {
                NSTextAlignment::Left
            } else {
                NSTextAlignment::Center
            });
            label.setTextColor(Some(&NSColor::labelColor()));
            let font_size = if matches!(mode, DisplayMode::Titles) {
                14.0 * scale
            } else {
                11.0 * scale
            };
            label.setFont(Some(&NSFont::systemFontOfSize(font_size)));
            label.setFrame(to_nsrect(cf.title));
            content.addSubview(&label);
        }

        *self.thumb_views.borrow_mut() = thumb_targets;

        // Affiche sans voler le focus à l'application active.
        self.panel.orderFrontRegardless();
    }

    /// Génération d'affichage courante (cf. [`Overlay::set_thumbnail`]).
    pub fn generation(&self) -> u64 {
        self.generation.get()
    }

    /// Remplace l'icône-placeholder d'une cellule par la miniature `image` de la
    /// fenêtre `id`, si l'affichage est toujours de la génération `generation`
    /// (sinon la capture est périmée et on l'ignore). Appelé sur le thread
    /// principal depuis la livraison des captures faites en arrière-plan.
    pub fn set_thumbnail(&self, generation: u64, id: WindowId, image: &CGImage) {
        if generation != self.generation.get() {
            return;
        }
        let targets = self.thumb_views.borrow();
        if let Some((_, view)) = targets.iter().find(|(wid, _)| *wid == id) {
            // NSSize(0,0) → l'image garde sa taille en pixels ; l'NSImageView la
            // met à l'échelle dans son cadre.
            let ns = NSImage::initWithCGImage_size(NSImage::alloc(), image, NSSize::new(0.0, 0.0));
            view.setImage(Some(&ns));
        }
    }

    /// Index de la cellule voisine de `from` dans la direction `dir` (navigation
    /// aux flèches), ou `None` au bord de la grille.
    pub fn neighbor(&self, from: usize, dir: Direction) -> Option<usize> {
        layout::neighbor(&self.sel_frames, from, dir)
    }

    /// Déplace la surbrillance sur l'élément `selected` (overlay déjà visible).
    pub fn select(&mut self, selected: usize) {
        if let Some(frame) = self.sel_frames.get(selected) {
            self.selection.setHidden(false);
            self.selection.setFrame(to_nsrect(*frame));
        }
    }

    /// Masque l'overlay. Incrémente la génération pour qu'aucune miniature encore
    /// en vol ne vienne se poser sur un affichage refermé.
    pub fn hide(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        self.thumb_views.borrow_mut().clear();
        self.panel.orderOut(None);
    }

    /// « Préchauffe » le panneau : force, hors écran, la création de la surface
    /// fenêtre et le premier rendu du verre dépoli, afin que la toute première
    /// ouverture réelle soit instantanée (sinon le premier affichage accuse un
    /// délai notable, le temps que le serveur de fenêtres alloue la surface).
    pub fn prewarm(&self) {
        self.glass
            .setFrame(NSRect::new(NSPoint::ZERO, NSSize::new(200.0, 120.0)));
        self.panel.setFrameOrigin(NSPoint::new(-10_000.0, -10_000.0));
        self.panel.orderFrontRegardless();
        self.panel.orderOut(None);
    }
}

/// Icône de l'application de PID `pid`, le cas échéant.
fn app_icon_for_pid(pid: i32) -> Option<Retained<NSImage>> {
    windows::focus::running_app(pid)?.icon()
}

/// Convertit un [`Rect`] de disposition en `NSRect`.
fn to_nsrect(r: Rect) -> NSRect {
    NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h))
}

/// Crée la vue de fond en verre dépoli (flou translucide) à coins arrondis.
fn make_glass(mtm: MainThreadMarker, frame: NSRect) -> Retained<NSVisualEffectView> {
    // SAFETY: initWithFrame: est l'initialiseur correct d'une NSView.
    let view: Retained<NSVisualEffectView> =
        unsafe { msg_send![NSVisualEffectView::alloc(mtm), initWithFrame: frame] };
    view.setMaterial(NSVisualEffectMaterial::HUDWindow);
    view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    view.setState(NSVisualEffectState::Active);
    view.setWantsLayer(true);
    if let Some(layer) = view.layer() {
        layer.setCornerRadius(18.0);
        layer.setMasksToBounds(true);
    }
    view
}
