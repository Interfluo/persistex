//! Per-tone editor: a table of frequency / period / amplitude, plus generators.

use crate::theme::*;
use crate::{SpecRow, ToneRow};
use egui::Vec2;
use persistex_core::design::{generate_tones, Generator, Shape, Spacing};
use std::collections::HashSet;

/// Outcome of showing the editor, applied by the caller.
pub struct Outcome {
    pub changed: bool,
    pub close: bool,
}

fn num_field(ui: &mut egui::Ui, value: &mut String, width: f32) -> bool {
    ui.add_sized(
        [width, 20.0],
        egui::TextEdit::singleline(value).horizontal_align(egui::Align::RIGHT),
    )
    .changed()
}

/// The window for one input. `taken` holds bins used by the *other* inputs, so a
/// generated set steps around them and the inputs stay orthogonal.
pub fn show(
    ctx: &egui::Context,
    row: &mut SpecRow,
    index: usize,
    f0: f64,
    taken: &HashSet<usize>,
) -> Outcome {
    let mut changed = false;
    let mut open = true;
    let mut close = false;

    egui::Window::new(format!("Tones — {}", row.name))
        .id(egui::Id::new(("tone_editor", index)))
        .open(&mut open)
        .default_width(430.0)
        .collapsible(false)
        .show(ctx, |ui| {
            // ---------------------------------------------------------- generate
            egui::Frame::new()
                .fill(PANEL)
                .stroke(egui::Stroke::new(1.0_f32, FRAME))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Generate a set")
                            .size(SIZE_SMALL)
                            .color(MUTED),
                    );
                    ui.add_space(4.0);
                    let g = &mut row.generator;
                    egui::Grid::new(("gen", index))
                        .num_columns(4)
                        .spacing([6.0, 5.0])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("f min").size(SIZE_SMALL));
                            num_field(ui, &mut g.f_min, 58.0);
                            ui.label(egui::RichText::new("f max").size(SIZE_SMALL));
                            num_field(ui, &mut g.f_max, 58.0);
                            ui.end_row();

                            ui.label(egui::RichText::new("tones").size(SIZE_SMALL));
                            num_field(ui, &mut g.count, 58.0);
                            ui.label(egui::RichText::new("amplitude").size(SIZE_SMALL));
                            num_field(ui, &mut g.amplitude, 58.0);
                            ui.end_row();

                            ui.label(egui::RichText::new("spacing").size(SIZE_SMALL));
                            egui::ComboBox::from_id_salt(("gspacing", index))
                                .selected_text(g.spacing.label())
                                .width(76.0)
                                .show_ui(ui, |ui| {
                                    for s in Spacing::ALL {
                                        ui.selectable_value(&mut g.spacing, s, s.label());
                                    }
                                });
                            ui.label(egui::RichText::new("shape").size(SIZE_SMALL));
                            egui::ComboBox::from_id_salt(("gshape", index))
                                .selected_text(g.shape.label())
                                .width(76.0)
                                .show_ui(ui, |ui| {
                                    for s in Shape::ALL {
                                        ui.selectable_value(&mut g.shape, s, s.label());
                                    }
                                });
                            ui.end_row();
                        });

                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut row.generator.odd_only, "odd harmonics only")
                            .on_hover_text(
                                "Even-order distortion then lands on empty bins, \
                                 where it can be measured",
                            );
                    });
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        let spec = row.generator.parse();
                        if ui
                            .add_enabled(spec.is_some(), egui::Button::new("Replace"))
                            .on_hover_text("Discard the tones below and generate a fresh set")
                            .clicked()
                        {
                            if let Some(g) = spec {
                                row.tones = generate_tones(&g, f0, taken)
                                    .iter()
                                    .map(ToneRow::from_tone)
                                    .collect();
                                changed = true;
                            }
                        }
                        if ui
                            .add_enabled(spec.is_some(), egui::Button::new("Append"))
                            .on_hover_text("Add a generated set to the tones below")
                            .clicked()
                        {
                            if let Some(g) = spec {
                                let mut avoid = taken.clone();
                                for t in &row.tones {
                                    if let Some(f) = t.frequency_value() {
                                        avoid.insert((f / f0).round().max(1.0) as usize);
                                    }
                                }
                                row.tones.extend(
                                    generate_tones(&g, f0, &avoid)
                                        .iter()
                                        .map(ToneRow::from_tone),
                                );
                                changed = true;
                            }
                        }
                    });
                });

            ui.add_space(8.0);

            // ------------------------------------------------------------- table
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} tones", row.tones.len()))
                        .size(SIZE_SMALL)
                        .color(MUTED),
                );
                ui.label(
                    egui::RichText::new("· fill either frequency or period")
                        .size(SIZE_SMALL)
                        .color(FAINT),
                );
            });
            ui.add_space(3.0);

            let mut remove: Option<usize> = None;
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    egui::Grid::new(("tones", index))
                        .num_columns(5)
                        .spacing([8.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for title in ["#", "Frequency (Hz)", "Period (s)", "Amplitude", ""] {
                                ui.label(egui::RichText::new(title).size(SIZE_SMALL).color(MUTED));
                            }
                            ui.end_row();

                            for i in 0..row.tones.len() {
                                let tone = &mut row.tones[i];
                                ui.label(
                                    egui::RichText::new(format!("{}", i + 1))
                                        .size(SIZE_SMALL)
                                        .color(FAINT),
                                );
                                if num_field(ui, &mut tone.frequency, 90.0) {
                                    tone.sync_from_frequency();
                                    changed = true;
                                }
                                if num_field(ui, &mut tone.period, 90.0) {
                                    tone.sync_from_period();
                                    changed = true;
                                }
                                if num_field(ui, &mut tone.amplitude, 72.0) {
                                    changed = true;
                                }
                                if ui.small_button("×").clicked() {
                                    remove = Some(i);
                                }
                                ui.end_row();
                            }
                        });
                });

            if let Some(i) = remove {
                row.tones.remove(i);
                changed = true;
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Add tone").clicked() {
                    let next = row
                        .tones
                        .last()
                        .and_then(|t| t.frequency_value())
                        .map(|f| f + f0.max(0.1))
                        .unwrap_or(1.0);
                    row.tones.push(ToneRow::new(next, 1.0));
                    changed = true;
                }
                if ui
                    .add_enabled(!row.tones.is_empty(), egui::Button::new("Clear"))
                    .clicked()
                {
                    row.tones.clear();
                    changed = true;
                }
                ui.add_space(8.0);
                if ui
                    .add_sized([70.0, 22.0], egui::Button::new("Done"))
                    .clicked()
                {
                    close = true;
                }
            });

            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(format!(
                    "Frequencies snap to multiples of f0 = {:.4} Hz",
                    f0
                ))
                .size(SIZE_SMALL)
                .color(FAINT),
            );
        });

    Outcome {
        changed,
        close: close || !open,
    }
}

/// Keeps Vec2 in scope for sized widgets.
#[allow(dead_code)]
fn _sizing() -> Vec2 {
    Vec2::ZERO
}

/// Generator fields as strings, so typing is never fought by parsing.
#[derive(Clone)]
pub struct GeneratorRow {
    pub f_min: String,
    pub f_max: String,
    pub count: String,
    pub amplitude: String,
    pub spacing: Spacing,
    pub shape: Shape,
    pub odd_only: bool,
}

impl Default for GeneratorRow {
    fn default() -> Self {
        GeneratorRow {
            f_min: "0.1".into(),
            f_max: "2".into(),
            count: "6".into(),
            amplitude: "1".into(),
            spacing: Spacing::Linear,
            shape: Shape::Flat,
            odd_only: false,
        }
    }
}

impl GeneratorRow {
    pub fn parse(&self) -> Option<Generator> {
        let count: usize = self.count.trim().parse().ok()?;
        if count == 0 {
            return None;
        }
        Some(Generator {
            f_min: self.f_min.trim().parse().ok()?,
            f_max: self.f_max.trim().parse().ok()?,
            count,
            spacing: self.spacing,
            shape: self.shape,
            amplitude: self.amplitude.trim().parse().ok()?,
            odd_only: self.odd_only,
        })
    }
}
