//! Disposition de l'overlay : calcul pur (sans AppKit) des tailles et frames,
//! donc testable unitairement.
//!
//! Convention de coordonnées AppKit : origine en bas à gauche, l'axe Y monte.
//! Dans chaque cellule, l'aperçu (miniature ou icône) est au-dessus du titre.

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
    /// Zone d'aperçu : miniature de la fenêtre, ou icône d'application en repli.
    pub image: Rect,
    pub title: Rect,
    /// Rectangle de surbrillance derrière la cellule sélectionnée.
    pub selection: Rect,
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
/// Largeur de la zone d'aperçu.
pub const THUMB_W: f64 = 150.0;
/// Hauteur de la zone d'aperçu.
pub const THUMB_H: f64 = 96.0;
/// Espace vertical entre l'aperçu et le titre.
pub const GAP: f64 = 6.0;
/// Hauteur de la zone de titre.
pub const TITLE_H: f64 = 16.0;
/// Largeur d'une cellule.
pub const CELL_W: f64 = 170.0;

/// Calcule la disposition pour `count` cellules disposées en une rangée
/// horizontale.
pub fn compute(count: usize) -> Layout {
    let inner_h = THUMB_H + GAP + TITLE_H;
    let height = inner_h + 2.0 * PAD;
    let width = (count as f64) * CELL_W + 2.0 * PAD;

    let mut cells = Vec::with_capacity(count);
    for i in 0..count {
        let cx = PAD + (i as f64) * CELL_W;
        let image = Rect {
            x: cx + (CELL_W - THUMB_W) / 2.0,
            y: PAD + TITLE_H + GAP,
            w: THUMB_W,
            h: THUMB_H,
        };
        let title = Rect {
            x: cx + 6.0,
            y: PAD,
            w: CELL_W - 12.0,
            h: TITLE_H,
        };
        let selection = Rect {
            x: cx + 4.0,
            y: PAD - 8.0,
            w: CELL_W - 8.0,
            h: inner_h + 14.0,
        };
        cells.push(CellFrame {
            image,
            title,
            selection,
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
        let l = compute(0);
        assert!(l.cells.is_empty());
        assert_eq!(l.width, 2.0 * PAD);
        assert_eq!(l.height, THUMB_H + GAP + TITLE_H + 2.0 * PAD);
    }

    #[test]
    fn largeur_proportionnelle_au_nombre() {
        let l = compute(3);
        assert_eq!(l.cells.len(), 3);
        assert_eq!(l.width, 3.0 * CELL_W + 2.0 * PAD);
    }

    #[test]
    fn cellules_alignees_horizontalement() {
        let l = compute(3);
        // Chaque cellule est décalée de CELL_W vers la droite.
        assert_eq!(l.cells[1].image.x - l.cells[0].image.x, CELL_W);
        assert_eq!(l.cells[2].image.x - l.cells[1].image.x, CELL_W);
    }

    #[test]
    fn apercu_au_dessus_du_titre() {
        let l = compute(1);
        let c = l.cells[0];
        assert!(c.image.y > c.title.y, "l'aperçu doit être au-dessus du titre");
    }

    #[test]
    fn frames_dans_les_limites_du_panneau() {
        let l = compute(4);
        for c in &l.cells {
            for r in [c.image, c.title, c.selection] {
                assert!(r.x >= 0.0 && r.y >= 0.0, "{r:?} hors limites (origine)");
                assert!(r.x + r.w <= l.width + 0.001, "{r:?} déborde en largeur");
                assert!(r.y + r.h <= l.height + 0.001, "{r:?} déborde en hauteur");
            }
        }
    }
}
