//! Simple mode: the forest as a handful of choices (theme, tree types, how much of each, snow, light)
//! instead of the tabs of sliders. Everything here sets the same project settings the Advanced tabs show.

use super::TreeApp;
use eframe::egui;
use forest_core::treegen_sprite::{tree_type_label, TREE_TYPES};
use forest_core::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Slot {
    Main,
    Mix,
    Shrub,
}

impl Slot {
    const ALL: [Slot; 3] = [Slot::Main, Slot::Mix, Slot::Shrub];

    fn index(self, g: &GenParams) -> usize {
        match self {
            Slot::Main => g.main_species as usize,
            Slot::Mix => g.mix_species as usize,
            Slot::Shrub => g.shrub_species as usize,
        }
    }

    fn set(self, g: &mut GenParams, i: usize) {
        let i = i as u8;
        match self {
            Slot::Main => g.main_species = i,
            Slot::Mix => g.mix_species = i,
            Slot::Shrub => g.shrub_species = i,
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Slot::Main => "main",
            Slot::Mix => "mix",
            Slot::Shrub => "shrubs",
        }
    }
}

impl TreeApp {
    /// The species a slot uses, copied first if another slot shares it, so changing one leaves the others alone.
    fn own_slot(&mut self, slot: Slot) -> usize {
        let g = &self.project.gen;
        let i = slot.index(g).min(self.project.species.len().saturating_sub(1));
        let shared = Slot::ALL.iter().any(|&o| o != slot && o.index(g) == i);
        if !shared {
            return i;
        }
        let mut c = self.project.species[i].clone();
        c.name = format!("{} ({})", c.name, slot.suffix());
        let j = match self.project.species.iter().position(|s| s.name == c.name) {
            Some(j) => {
                self.project.species[j] = c;
                j
            }
            None if self.project.species.len() < 255 => {
                self.project.species.push(c);
                self.project.species.len() - 1
            }
            None => return i,
        };
        slot.set(&mut self.project.gen, j);
        j
    }

    pub(super) fn tree_type_picker(&mut self, ui: &mut egui::Ui, slot: Slot, label: &str) {
        let i = slot.index(&self.project.gen);
        let Some(sp) = self.project.species.get(i) else { return };
        let current = sp.tree_type().map(str::to_string);
        let text = current.as_deref().map(|t| tree_type_label(t).to_string()).unwrap_or_else(|| "None".into());
        let mut pick: Option<&str> = None;
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(egui::Vec2::new(super::tuning::LABEL_W, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_size(egui::Vec2::new(super::tuning::LABEL_W, 20.0));
                ui.label(format!("  {label}"));
            });
            egui::ComboBox::from_id_salt(("simple_type", slot.suffix())).width((ui.available_width() - 40.0).clamp(120.0, 260.0)).selected_text(text).show_ui(ui, |ui| {
                for (key, name) in TREE_TYPES.iter().filter(|t| !forest_core::mushrooms::is_mushroom(t.0)) {
                    if ui.selectable_label(current.as_deref() == Some(*key), *name).clicked() {
                        pick = Some(*key);
                    }
                }
            });
        });
        if let Some(p) = pick {
            let j = self.own_slot(slot);
            let sp = &mut self.project.species[j];
            sp.procedural = true;
            sp.preset = p.to_string();
        }
    }
}
