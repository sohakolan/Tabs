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

/// Direction de navigation au clavier (flèches).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Index de la cellule voisine de `from` dans la direction `dir`, ou `None` s'il
/// n'y en a pas (bord de la grille). Purement géométrique sur les rectangles de
/// cellule : fonctionne pour la grille horizontale comme pour la liste verticale,
/// quel que soit l'ordre de remplissage, et borne aux bords (pas d'enroulement).
///
/// On ne retient que les cellules réellement situées dans la direction demandée,
/// puis on choisit la plus proche en pénalisant le décalage sur l'axe transverse
/// (pour rester dans la même rangée/colonne quand c'est possible).
pub fn neighbor(rects: &[Rect], from: usize, dir: Direction) -> Option<usize> {
    let cur = rects.get(from)?;
    let cx = cur.x + cur.w / 2.0;
    let cy = cur.y + cur.h / 2.0;

    // Poids du décalage transverse : assez grand pour préférer l'alignement.
    const CROSS_WEIGHT: f64 = 3.0;
    // Seuil minimal de progression dans la direction (anti-bruit).
    const EPS: f64 = 0.5;

    let mut best: Option<usize> = None;
    let mut best_score = f64::INFINITY;
    for (i, r) in rects.iter().enumerate() {
        if i == from {
            continue;
        }
        let rx = r.x + r.w / 2.0;
        let ry = r.y + r.h / 2.0;
        let dx = rx - cx;
        let dy = ry - cy;
        // `primary` = avancée dans la direction voulue (doit être > 0) ;
        // `cross` = écart sur l'axe perpendiculaire (en valeur absolue).
        // Note : en coordonnées AppKit, l'axe Y monte (haut = y plus grand).
        let (primary, cross) = match dir {
            Direction::Left => (-dx, dy.abs()),
            Direction::Right => (dx, dy.abs()),
            Direction::Up => (dy, dx.abs()),
            Direction::Down => (-dy, dx.abs()),
        };
        if primary <= EPS {
            continue;
        }
        let score = primary + cross * CROSS_WEIGHT;
        if score < best_score {
            best_score = score;
            best = Some(i);
        }
    }
    best
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

/// Marge extérieure du panneau (à l'échelle 1.0).
const PAD: f64 = 18.0;
/// Espace vertical entre l'aperçu et le titre (à l'échelle 1.0).
const GAP: f64 = 6.0;
/// Hauteur de la zone de titre (à l'échelle 1.0).
const TITLE_H: f64 = 16.0;

/// Métriques dépendant du mode (déjà mises à l'échelle).
struct Metrics {
    image_w: f64,
    image_h: f64,
    cell_w: f64,
}

fn metrics(mode: DisplayMode, scale: f64) -> Metrics {
    let m = match mode {
        DisplayMode::Thumbnails => (150.0, 96.0, 170.0),
        DisplayMode::AppIcons => (72.0, 72.0, 104.0),
        DisplayMode::Titles => (0.0, 0.0, 240.0),
    };
    Metrics {
        image_w: m.0 * scale,
        image_h: m.1 * scale,
        cell_w: m.2 * scale,
    }
}

/// Constantes du mode Titres (liste verticale), à l'échelle 1.0.
const TITLE_ROW_H: f64 = 40.0;
const TITLE_ICON: f64 = 28.0;
const TITLES_W: f64 = 480.0;

/// Calcule la disposition selon le mode : grille horizontale (Miniatures,
/// Icônes) ou liste verticale (Titres). `scale` est le facteur d'échelle ;
/// `max_w`/`max_h` bornent la zone disponible (l'écran visible) pour replier
/// les éléments sur plusieurs rangées/colonnes et ne jamais déborder.
pub fn compute(count: usize, mode: DisplayMode, scale: f64, max_w: f64, max_h: f64) -> Layout {
    if matches!(mode, DisplayMode::Titles) {
        return compute_titles(count, scale, max_h);
    }
    compute_row(count, mode, scale, max_w)
}

/// Liste verticale : icône + titre par ligne, repliée sur plusieurs colonnes
/// si la hauteur dépasse `max_h`.
fn compute_titles(count: usize, scale: f64, max_h: f64) -> Layout {
    let pad = PAD * scale;
    let row_h = TITLE_ROW_H * scale;
    let icon = TITLE_ICON * scale;
    let col_w = TITLES_W * scale;

    // Nombre de lignes qui tiennent dans une colonne, puis nombre de colonnes.
    let avail_h = (max_h - 2.0 * pad).max(row_h);
    let rows_per_col = ((avail_h / row_h).floor() as usize).max(1);
    let cols = count.div_ceil(rows_per_col).max(1);
    let used_rows = count.min(rows_per_col);

    let height = (used_rows.max(1) as f64) * row_h + 2.0 * pad;
    let width = if count == 0 {
        2.0 * pad
    } else {
        (cols as f64) * col_w
    };

    let mut cells = Vec::with_capacity(count);
    for i in 0..count {
        let col = i / rows_per_col;
        let r = i % rows_per_col;
        let col_x = (col as f64) * col_w;
        // Premier élément en haut : y décroît avec la ligne.
        let ry = height - pad - (r as f64 + 1.0) * row_h;
        let image = Rect {
            x: col_x + pad + 6.0 * scale,
            y: ry + (row_h - icon) / 2.0,
            w: icon,
            h: icon,
        };
        let tx = pad + 6.0 * scale + icon + 10.0 * scale;
        let title = Rect {
            x: col_x + tx,
            y: ry + (row_h - 22.0 * scale) / 2.0,
            w: col_w - tx - pad,
            h: 22.0 * scale,
        };
        let selection = Rect {
            x: col_x + 6.0 * scale,
            y: ry + 1.0 * scale,
            w: col_w - 12.0 * scale,
            h: row_h - 2.0 * scale,
        };
        let hit = Rect {
            x: col_x,
            y: ry,
            w: col_w,
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
        width,
        height,
        cells,
    }
}

/// Grille horizontale (Miniatures / Icônes), repliée sur plusieurs rangées si
/// la largeur dépasse `max_w`.
fn compute_row(count: usize, mode: DisplayMode, scale: f64, max_w: f64) -> Layout {
    let m = metrics(mode, scale);
    let shows_image = mode.shows_image();
    let pad = PAD * scale;
    let gap = GAP * scale;
    let title_h = TITLE_H * scale;

    let inner_h = if shows_image {
        m.image_h + gap + title_h
    } else {
        title_h
    };
    // Espace vertical entre deux rangées.
    let row_gap = gap;

    // Colonnes qui tiennent dans la largeur disponible (au moins 1).
    let avail_w = (max_w - 2.0 * pad).max(m.cell_w);
    let max_cols = ((avail_w / m.cell_w).floor() as usize).max(1);
    let cols = if count == 0 { 0 } else { count.min(max_cols) };
    let rows = if cols == 0 { 0 } else { count.div_ceil(cols) };

    let width = (cols as f64) * m.cell_w + 2.0 * pad;
    let height = if rows == 0 {
        inner_h + 2.0 * pad
    } else {
        (rows as f64) * inner_h + ((rows - 1) as f64) * row_gap + 2.0 * pad
    };

    let mut cells = Vec::with_capacity(count);
    for i in 0..count {
        let col = i % cols;
        let row = i / cols;
        let cx = pad + (col as f64) * m.cell_w;
        // Bas de la rangée `row` (la rangée 0 est en haut).
        let ry = height - pad - (row as f64 + 1.0) * inner_h - (row as f64) * row_gap;

        let image = if shows_image {
            Rect {
                x: cx + (m.cell_w - m.image_w) / 2.0,
                y: ry + title_h + gap,
                w: m.image_w,
                h: m.image_h,
            }
        } else {
            Rect {
                x: cx,
                y: ry,
                w: 0.0,
                h: 0.0,
            }
        };
        let title = Rect {
            x: cx + 6.0 * scale,
            y: ry,
            w: m.cell_w - 12.0 * scale,
            h: if shows_image { title_h } else { inner_h },
        };
        let selection = Rect {
            x: cx + 4.0 * scale,
            y: ry - 8.0 * scale,
            w: m.cell_w - 8.0 * scale,
            h: inner_h + 14.0 * scale,
        };
        let hit = Rect {
            x: cx,
            y: ry - row_gap / 2.0,
            w: m.cell_w,
            h: inner_h + row_gap,
        };
        // Pastille d'icône d'app, dans le bas-centre de la miniature (Thumbnails).
        let badge = if matches!(mode, DisplayMode::Thumbnails) {
            let bs = 42.0 * scale;
            Rect {
                x: image.x + (image.w - bs) / 2.0,
                y: image.y + 6.0 * scale,
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

    /// Écran « infini » : pas de repli, comportement d'une seule rangée/colonne.
    const BIG_W: f64 = 100_000.0;
    const BIG_H: f64 = 100_000.0;

    fn compute1(count: usize, mode: DisplayMode) -> Layout {
        compute(count, mode, 1.0, BIG_W, BIG_H)
    }

    #[test]
    fn panneau_vide() {
        let l = compute1(0, DisplayMode::Thumbnails);
        assert!(l.cells.is_empty());
        assert_eq!(l.width, 2.0 * PAD);
    }

    #[test]
    fn largeur_proportionnelle_au_nombre() {
        let l = compute1(3, DisplayMode::Thumbnails);
        assert_eq!(l.cells.len(), 3);
        let cell_w = (l.width - 2.0 * PAD) / 3.0;
        assert_eq!(l.cells[1].hit.x - l.cells[0].hit.x, cell_w);
    }

    #[test]
    fn apercu_au_dessus_du_titre_en_thumbnails() {
        let l = compute1(1, DisplayMode::Thumbnails);
        let c = l.cells[0];
        assert!(c.image.y > c.title.y, "l'aperçu doit être au-dessus du titre");
    }

    #[test]
    fn titles_liste_verticale() {
        let l = compute1(3, DisplayMode::Titles);
        // Largeur fixe, hauteur qui croît avec le nombre d'éléments.
        assert_eq!(l.width, compute1(5, DisplayMode::Titles).width);
        assert!(compute1(5, DisplayMode::Titles).height > l.height);
        // Empilées de haut en bas : le premier élément est plus haut (y plus grand).
        assert!(l.cells[0].hit.y > l.cells[1].hit.y);
        assert!(l.cells[1].hit.y > l.cells[2].hit.y);
        // Chaque ligne a une petite icône à gauche.
        assert!(l.cells[0].image.w > 0.0);
    }

    #[test]
    fn appicons_plus_compact_que_thumbnails() {
        let icons = compute1(3, DisplayMode::AppIcons);
        let thumbs = compute1(3, DisplayMode::Thumbnails);
        assert!(icons.width < thumbs.width);
    }

    #[test]
    fn cycle_des_modes() {
        assert_eq!(DisplayMode::Thumbnails.next(), DisplayMode::AppIcons);
        assert_eq!(DisplayMode::AppIcons.next(), DisplayMode::Titles);
        assert_eq!(DisplayMode::Titles.next(), DisplayMode::Thumbnails);
    }

    #[test]
    fn echelle_agrandit_tout() {
        let base = compute(3, DisplayMode::Thumbnails, 1.0, BIG_W, BIG_H);
        let grand = compute(3, DisplayMode::Thumbnails, 1.45, BIG_W, BIG_H);
        assert!(grand.width > base.width);
        assert!(grand.height > base.height);
        assert!(grand.cells[0].image.w > base.cells[0].image.w);
    }

    #[test]
    fn repli_en_grille_quand_la_largeur_deborde() {
        // 8 miniatures mais une largeur ne tenant que ~3 colonnes → plusieurs
        // rangées, sans jamais dépasser la largeur disponible.
        let max_w = 3.5 * 170.0 + 2.0 * PAD;
        let l = compute(8, DisplayMode::Thumbnails, 1.0, max_w, BIG_H);
        assert!(l.width <= max_w + 0.001, "la grille déborde : {}", l.width);
        // Plus haute qu'une seule rangée (donc bien repliée).
        let single = compute1(8, DisplayMode::Thumbnails);
        assert!(l.height > single.height);
    }

    #[test]
    fn repli_en_colonnes_quand_la_hauteur_deborde_en_titres() {
        // Beaucoup de titres, hauteur réduite → plusieurs colonnes.
        let max_h = 5.5 * 40.0 + 2.0 * PAD;
        let l = compute(12, DisplayMode::Titles, 1.0, BIG_W, max_h);
        assert!(l.height <= max_h + 0.001, "la liste déborde : {}", l.height);
        assert!(l.width > compute1(12, DisplayMode::Titles).width);
    }

    /// Rectangles de sélection d'une disposition (les cibles de navigation).
    fn sel_rects(l: &Layout) -> Vec<Rect> {
        l.cells.iter().map(|c| c.selection).collect()
    }

    #[test]
    fn fleches_dans_une_grille_2x3() {
        // 6 miniatures, largeur limitée à ~3 colonnes → 2 rangées de 3.
        // Indices : rangée 0 = 0,1,2 (haut) ; rangée 1 = 3,4,5 (bas).
        let l = compute(6, DisplayMode::Thumbnails, 1.0, 600.0, BIG_H);
        let r = sel_rects(&l);
        assert_eq!(neighbor(&r, 0, Direction::Right), Some(1));
        assert_eq!(neighbor(&r, 1, Direction::Left), Some(0));
        assert_eq!(neighbor(&r, 0, Direction::Down), Some(3));
        assert_eq!(neighbor(&r, 4, Direction::Up), Some(1));
        // Bords : pas d'enroulement.
        assert_eq!(neighbor(&r, 0, Direction::Left), None);
        assert_eq!(neighbor(&r, 0, Direction::Up), None);
        assert_eq!(neighbor(&r, 2, Direction::Right), None);
        assert_eq!(neighbor(&r, 5, Direction::Down), None);
    }

    #[test]
    fn fleches_dans_une_liste_de_titres() {
        // Liste verticale d'une seule colonne : Bas/Haut naviguent, pas Gauche/Droite.
        let l = compute(4, DisplayMode::Titles, 1.0, BIG_W, BIG_H);
        let r = sel_rects(&l);
        assert_eq!(neighbor(&r, 0, Direction::Down), Some(1));
        assert_eq!(neighbor(&r, 2, Direction::Up), Some(1));
        assert_eq!(neighbor(&r, 0, Direction::Up), None);
        assert_eq!(neighbor(&r, 3, Direction::Down), None);
        assert_eq!(neighbor(&r, 0, Direction::Right), None);
    }

    #[test]
    fn fleches_index_hors_borne() {
        let l = compute(3, DisplayMode::AppIcons, 1.0, BIG_W, BIG_H);
        let r = sel_rects(&l);
        assert_eq!(neighbor(&r, 9, Direction::Right), None);
    }

    #[test]
    fn frames_dans_les_limites_du_panneau() {
        for scale in [0.72, 1.0, 1.45] {
            for mode in [
                DisplayMode::Thumbnails,
                DisplayMode::AppIcons,
                DisplayMode::Titles,
            ] {
                // Largeur/hauteur volontairement serrées pour forcer le repli.
                let l = compute(9, mode, scale, 600.0, 500.0);
                for c in &l.cells {
                    for r in [c.image, c.badge, c.title, c.selection, c.hit] {
                        assert!(r.x >= -0.001 && r.y >= -0.001, "{r:?} hors limites (origine)");
                        assert!(r.x + r.w <= l.width + 0.001, "{r:?} déborde en largeur");
                        assert!(r.y + r.h <= l.height + 0.001, "{r:?} déborde en hauteur");
                    }
                }
            }
        }
    }
}
