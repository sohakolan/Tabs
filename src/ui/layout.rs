//! Disposition de l'overlay : calcul pur (sans AppKit) des tailles et frames,
//! donc testable unitairement. Paramétré par le [`DisplayMode`].
//!
//! Convention de coordonnées AppKit : origine en bas à gauche, l'axe Y monte.
//! Dans chaque cellule, l'aperçu (miniature ou icône) est au-dessus du titre.

/// Mode d'affichage des cellules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DisplayMode {
    /// Miniatures des fenêtres (capture) + titre.
    Thumbnails,
    /// Icônes d'application (façon Dock) + titre.
    AppIcons,
    /// Liste compacte, titre uniquement.
    Titles,
}

impl DisplayMode {
    /// Mode suivant, en cycle.
    pub fn next(self) -> Self {
        match self {
            DisplayMode::Thumbnails => DisplayMode::AppIcons,
            DisplayMode::AppIcons => DisplayMode::Titles,
            DisplayMode::Titles => DisplayMode::Thumbnails,
        }
    }

    /// L'aperçu visuel est-il affiché dans ce mode ?
    pub fn shows_image(self) -> bool {
        !matches!(self, DisplayMode::Titles)
    }
}

/// Rectangle simple en points (coordonnées AppKit, origine bas-gauche).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Frames calculés pour une cellule (une fenêtre).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellFrame {
    /// Zone d'aperçu (miniature ou icône). De taille nulle en mode `Titles`.
    pub image: Rect,
    /// Pastille d'icône d'application posée sur la miniature (mode Thumbnails
    /// uniquement) ; de taille nulle sinon.
    pub badge: Rect,
    pub title: Rect,
    /// Rectangle de surbrillance derrière la cellule sélectionnée.
    pub selection: Rect,
    /// Zone cliquable/survolable couvrant toute la cellule (pour la souris).
    pub hit: Rect,
}

/// Résultat complet du calcul de disposition.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    pub cells: Vec<CellFrame>,
}

/// Marge extérieure du panneau.
pub const PAD: f64 = 18.0;
/// Espace vertical entre l'aperçu et le titre.
pub const GAP: f64 = 6.0;
/// Hauteur de la zone de titre.
pub const TITLE_H: f64 = 16.0;

/// Métriques dépendant du mode.
struct Metrics {
    image_w: f64,
    image_h: f64,
    cell_w: f64,
}

fn metrics(mode: DisplayMode) -> Metrics {
    match mode {
        DisplayMode::Thumbnails => Metrics {
            image_w: 150.0,
            image_h: 96.0,
            cell_w: 170.0,
        },
        DisplayMode::AppIcons => Metrics {
            image_w: 72.0,
            image_h: 72.0,
            cell_w: 104.0,
        },
        DisplayMode::Titles => Metrics {
            image_w: 0.0,
            image_h: 0.0,
            cell_w: 240.0,
        },
    }
}

/// Constantes du mode Titres (liste verticale).
const TITLE_ROW_H: f64 = 40.0;
const TITLE_ICON: f64 = 28.0;
const TITLES_W: f64 = 480.0;

/// Calcule la disposition selon le mode : rangée horizontale (Miniatures,
/// Icônes) ou liste verticale de haut en bas (Titres).
pub fn compute(count: usize, mode: DisplayMode) -> Layout {
    if matches!(mode, DisplayMode::Titles) {
        return compute_titles(count);
    }
    compute_row(count, mode)
}

/// Liste verticale : un élément par ligne (de haut en bas), icône + titre.
fn compute_titles(count: usize) -> Layout {
    let row_h = TITLE_ROW_H;
    let height = (count as f64) * row_h + 2.0 * PAD;
    let mut cells = Vec::with_capacity(count);
    for i in 0..count {
        // Premier élément en haut : y décroît avec l'index.
        let ry = height - PAD - (i as f64 + 1.0) * row_h;
        let image = Rect {
            x: PAD + 6.0,
            y: ry + (row_h - TITLE_ICON) / 2.0,
            w: TITLE_ICON,
            h: TITLE_ICON,
        };
        let tx = PAD + 6.0 + TITLE_ICON + 10.0;
        let title = Rect {
            x: tx,
            y: ry + (row_h - 22.0) / 2.0,
            w: TITLES_W - tx - PAD,
            h: 22.0,
        };
        let selection = Rect {
            x: 6.0,
            y: ry + 1.0,
            w: TITLES_W - 12.0,
            h: row_h - 2.0,
        };
        let hit = Rect {
            x: 0.0,
            y: ry,
            w: TITLES_W,
            h: row_h,
        };
        let badge = Rect {
            x: image.x,
            y: image.y,
            w: 0.0,
            h: 0.0,
        };
        cells.push(CellFrame {
            image,
            badge,
            title,
            selection,
            hit,
        });
    }
    Layout {
        width: TITLES_W,
        height,
        cells,
    }
}

/// Rangée horizontale (Miniatures / Icônes).
fn compute_row(count: usize, mode: DisplayMode) -> Layout {
    let m = metrics(mode);
    let shows_image = mode.shows_image();

    let inner_h = if shows_image {
        m.image_h + GAP + TITLE_H
    } else {
        TITLE_H
    };
    let height = inner_h + 2.0 * PAD;
    let width = (count as f64) * m.cell_w + 2.0 * PAD;

    let mut cells = Vec::with_capacity(count);
    for i in 0..count {
        let cx = PAD + (i as f64) * m.cell_w;

        let image = if shows_image {
            Rect {
                x: cx + (m.cell_w - m.image_w) / 2.0,
                y: PAD + TITLE_H + GAP,
                w: m.image_w,
                h: m.image_h,
            }
        } else {
            Rect {
                x: cx,
                y: PAD,
                w: 0.0,
                h: 0.0,
            }
        };
        let title = Rect {
            x: cx + 6.0,
            y: PAD,
            w: m.cell_w - 12.0,
            h: if shows_image { TITLE_H } else { inner_h },
        };
        let selection = Rect {
            x: cx + 4.0,
            y: PAD - 8.0,
            w: m.cell_w - 8.0,
            h: inner_h + 14.0,
        };
        let hit = Rect {
            x: cx,
            y: 0.0,
            w: m.cell_w,
            h: height,
        };
        // Pastille d'icône d'app, dans le bas-centre de la miniature (Thumbnails).
        let badge = if matches!(mode, DisplayMode::Thumbnails) {
            let bs = 42.0;
            Rect {
                x: image.x + (image.w - bs) / 2.0,
                y: image.y + 6.0,
                w: bs,
                h: bs,
            }
        } else {
            Rect {
                x: image.x,
                y: image.y,
                w: 0.0,
                h: 0.0,
            }
        };
        cells.push(CellFrame {
            image,
            badge,
            title,
            selection,
            hit,
        });
    }

    Layout {
        width,
        height,
        cells,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panneau_vide() {
        let l = compute(0, DisplayMode::Thumbnails);
        assert!(l.cells.is_empty());
        assert_eq!(l.width, 2.0 * PAD);
    }

    #[test]
    fn largeur_proportionnelle_au_nombre() {
        let l = compute(3, DisplayMode::Thumbnails);
        assert_eq!(l.cells.len(), 3);
        let cell_w = (l.width - 2.0 * PAD) / 3.0;
        assert_eq!(l.cells[1].hit.x - l.cells[0].hit.x, cell_w);
    }

    #[test]
    fn apercu_au_dessus_du_titre_en_thumbnails() {
        let l = compute(1, DisplayMode::Thumbnails);
        let c = l.cells[0];
        assert!(c.image.y > c.title.y, "l'aperçu doit être au-dessus du titre");
    }

    #[test]
    fn titles_liste_verticale() {
        let l = compute(3, DisplayMode::Titles);
        // Largeur fixe, hauteur qui croît avec le nombre d'éléments.
        assert_eq!(l.width, compute(5, DisplayMode::Titles).width);
        assert!(compute(5, DisplayMode::Titles).height > l.height);
        // Empilées de haut en bas : le premier élément est plus haut (y plus grand).
        assert!(l.cells[0].hit.y > l.cells[1].hit.y);
        assert!(l.cells[1].hit.y > l.cells[2].hit.y);
        // Chaque ligne a une petite icône à gauche.
        assert!(l.cells[0].image.w > 0.0);
    }

    #[test]
    fn appicons_plus_compact_que_thumbnails() {
        let icons = compute(3, DisplayMode::AppIcons);
        let thumbs = compute(3, DisplayMode::Thumbnails);
        assert!(icons.width < thumbs.width);
    }

    #[test]
    fn cycle_des_modes() {
        assert_eq!(DisplayMode::Thumbnails.next(), DisplayMode::AppIcons);
        assert_eq!(DisplayMode::AppIcons.next(), DisplayMode::Titles);
        assert_eq!(DisplayMode::Titles.next(), DisplayMode::Thumbnails);
    }

    #[test]
    fn frames_dans_les_limites_du_panneau() {
        for mode in [
            DisplayMode::Thumbnails,
            DisplayMode::AppIcons,
            DisplayMode::Titles,
        ] {
            let l = compute(4, mode);
            for c in &l.cells {
                for r in [c.image, c.badge, c.title, c.selection, c.hit] {
                    assert!(r.x >= 0.0 && r.y >= 0.0, "{r:?} hors limites (origine)");
                    assert!(r.x + r.w <= l.width + 0.001, "{r:?} déborde en largeur");
                    assert!(r.y + r.h <= l.height + 0.001, "{r:?} déborde en hauteur");
                }
            }
        }
    }
}
