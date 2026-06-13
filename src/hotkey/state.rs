//! Machine à états du sélecteur de fenêtres.
//!
//! Volontairement *pure* : aucune dépendance à macOS, afin d'être testable
//! unitairement. La couche [`super`] traduit les évènements clavier bruts
//! (CGEventTap) en [`Input`] et exécute les [`Action`] retournées.

/// Évènements d'entrée, déjà interprétés depuis le clavier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// Tab pressé pendant que Option est maintenu (Shift inverse le sens).
    Tab { shift: bool },
    /// La touche Option (Alt) vient d'être relâchée → validation.
    OptionReleased,
    /// Échap pressé → annulation.
    Escape,
}

/// Actions que la machine demande à l'hôte d'exécuter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Afficher l'overlay avec l'élément `selected` mis en évidence.
    Show { selected: usize },
    /// Déplacer la sélection sur `selected` (overlay déjà visible).
    Select { selected: usize },
    /// Valider la sélection `selected`, puis masquer l'overlay.
    Commit { selected: usize },
    /// Masquer l'overlay sans rien activer.
    Cancel,
    /// Rien à faire.
    None,
}

/// État du cycle commutateur de fenêtres : actif ou non, et index sélectionné parmi `count`
/// fenêtres.
#[derive(Debug)]
pub struct Switcher {
    active: bool,
    selected: usize,
    count: usize,
}

impl Switcher {
    pub fn new() -> Self {
        Self {
            active: false,
            selected: 0,
            count: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Index actuellement sélectionné.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Met à jour le nombre d'éléments cyclables. Ignoré pendant que le
    /// sélecteur est actif, pour ne pas perturber le cycle en cours.
    pub fn set_count(&mut self, count: usize) {
        if !self.active {
            self.count = count;
        }
    }

    /// Applique un évènement et retourne l'action à exécuter.
    pub fn on_input(&mut self, input: Input) -> Action {
        match input {
            Input::Tab { shift } => self.on_tab(shift),
            Input::OptionReleased => self.on_release(),
            Input::Escape => self.on_escape(),
        }
    }

    fn on_tab(&mut self, shift: bool) -> Action {
        if self.count == 0 {
            return Action::None;
        }
        if !self.active {
            self.active = true;
            // L'index 0 est la fenêtre courante ; à l'ouverture on saute donc
            // directement à la suivante (ou à la dernière avec Shift).
            self.selected = if shift {
                self.count - 1
            } else {
                1 % self.count
            };
            Action::Show {
                selected: self.selected,
            }
        } else {
            self.selected = if shift {
                (self.selected + self.count - 1) % self.count
            } else {
                (self.selected + 1) % self.count
            };
            Action::Select {
                selected: self.selected,
            }
        }
    }

    fn on_release(&mut self) -> Action {
        if !self.active {
            return Action::None;
        }
        self.active = false;
        Action::Commit {
            selected: self.selected,
        }
    }

    fn on_escape(&mut self) -> Action {
        if !self.active {
            return Action::None;
        }
        self.active = false;
        Action::Cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn switcher(count: usize) -> Switcher {
        let mut s = Switcher::new();
        s.set_count(count);
        s
    }

    #[test]
    fn tab_sans_fenetre_ne_fait_rien() {
        let mut s = switcher(0);
        assert_eq!(s.on_input(Input::Tab { shift: false }), Action::None);
        assert!(!s.is_active());
    }

    #[test]
    fn premier_tab_ouvre_sur_la_fenetre_suivante() {
        let mut s = switcher(3);
        assert_eq!(
            s.on_input(Input::Tab { shift: false }),
            Action::Show { selected: 1 }
        );
        assert!(s.is_active());
    }

    #[test]
    fn premier_tab_shift_ouvre_sur_la_derniere() {
        let mut s = switcher(3);
        assert_eq!(
            s.on_input(Input::Tab { shift: true }),
            Action::Show { selected: 2 }
        );
    }

    #[test]
    fn tabs_successifs_cyclent_et_bouclent() {
        let mut s = switcher(3);
        assert_eq!(
            s.on_input(Input::Tab { shift: false }),
            Action::Show { selected: 1 }
        );
        assert_eq!(
            s.on_input(Input::Tab { shift: false }),
            Action::Select { selected: 2 }
        );
        assert_eq!(
            s.on_input(Input::Tab { shift: false }),
            Action::Select { selected: 0 }
        );
    }

    #[test]
    fn shift_tab_revient_en_arriere() {
        let mut s = switcher(3);
        s.on_input(Input::Tab { shift: false }); // -> 1
        assert_eq!(
            s.on_input(Input::Tab { shift: true }),
            Action::Select { selected: 0 }
        );
        assert_eq!(
            s.on_input(Input::Tab { shift: true }),
            Action::Select { selected: 2 }
        );
    }

    #[test]
    fn relachement_valide_et_desactive() {
        let mut s = switcher(3);
        s.on_input(Input::Tab { shift: false }); // -> 1
        assert_eq!(
            s.on_input(Input::OptionReleased),
            Action::Commit { selected: 1 }
        );
        assert!(!s.is_active());
    }

    #[test]
    fn echap_annule_et_desactive() {
        let mut s = switcher(3);
        s.on_input(Input::Tab { shift: false });
        assert_eq!(s.on_input(Input::Escape), Action::Cancel);
        assert!(!s.is_active());
    }

    #[test]
    fn relachement_inactif_ne_fait_rien() {
        let mut s = switcher(3);
        assert_eq!(s.on_input(Input::OptionReleased), Action::None);
        assert_eq!(s.on_input(Input::Escape), Action::None);
    }

    #[test]
    fn set_count_ignore_pendant_cycle_actif() {
        let mut s = switcher(3);
        s.on_input(Input::Tab { shift: false }); // actif
        s.set_count(10); // ignoré
        // Le cycle reste borné par l'ancien count (3) : 1 -> 2 -> 0.
        s.on_input(Input::Tab { shift: false }); // -> 2
        assert_eq!(
            s.on_input(Input::Tab { shift: false }),
            Action::Select { selected: 0 }
        );
    }
}
