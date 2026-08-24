//! Editable per-input specification table.

use crate::theme::*;
use crate::{App, SpecRow};
use egui::Vec2;
use persistex_core::design::{InputSpec, Shape, Spacing};

const W_NAME: f32 = 74.0;
const W_NUM: f32 = 58.0;
const W_TONES: f32 = 52.0;
const W_COMBO: f32 = 92.0;

pub fn show(app: &mut App, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Inputs")
            .size(SIZE_HEAD)
            .strong()
            .color(TEXT),
    );
    ui.add_space(4.0);

    let mut changed = false;
    let mut remove: Option<usize> = None;
    let mut run: Option<usize> = None;
    let running = app.running;
    let n_rows = app.rows.len();

    egui::Grid::new("input_table")
        .num_columns(12)
        .spacing([8.0, 5.0])
        .striped(false)
        .show(ui, |ui| {
            for title in [
                "",
                "Input",
                "f min",
                "f max",
                "Tones",
                "Peak",
                "Amplitude",
                "Spacing",
                "RPF",
                "Allocated band",
                "Optimise",
                "Remove",
            ] {
                ui.label(egui::RichText::new(title).size(SIZE_SMALL).color(MUTED));
            }
            ui.end_row();

            for index in 0..n_rows {
                // colour swatch, tying the row to its trace
                let (rect, _) = ui.allocate_exact_size(Vec2::new(6.0, 20.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 1.0, series(index));

                let row: &mut SpecRow = &mut app.rows[index];
                changed |= ui
                    .add_enabled(
                        !running,
                        egui::TextEdit::singleline(&mut row.name).desired_width(W_NAME),
                    )
                    .changed();
                changed |= ui
                    .add_enabled(
                        !running,
                        egui::TextEdit::singleline(&mut row.f_min).desired_width(W_NUM),
                    )
                    .changed();
                changed |= ui
                    .add_enabled(
                        !running,
                        egui::TextEdit::singleline(&mut row.f_max).desired_width(W_NUM),
                    )
                    .changed();
                changed |= ui
                    .add_enabled(
                        !running,
                        egui::TextEdit::singleline(&mut row.n_tones).desired_width(W_TONES),
                    )
                    .changed();
                changed |= ui
                    .add_enabled(
                        !running,
                        egui::TextEdit::singleline(&mut row.peak).desired_width(W_NUM),
                    )
                    .changed();

                let mut shape = row.shape;
                egui::ComboBox::from_id_salt(("shape", index))
                    .selected_text(shape.label())
                    .width(W_COMBO)
                    .show_ui(ui, |ui| {
                        for s in Shape::ALL {
                            if ui.selectable_value(&mut shape, s, s.label()).changed() {
                                changed = true;
                            }
                        }
                    });
                app.rows[index].shape = shape;

                let mut spacing = app.rows[index].spacing;
                egui::ComboBox::from_id_salt(("spacing", index))
                    .selected_text(spacing.label())
                    .width(W_COMBO)
                    .show_ui(ui, |ui| {
                        for s in Spacing::ALL {
                            if ui.selectable_value(&mut spacing, s, s.label()).changed() {
                                changed = true;
                            }
                        }
                    });
                app.rows[index].spacing = spacing;

                match app.metrics.get(index) {
                    Some((rpf, band)) => {
                        ui.label(
                            egui::RichText::new(format!("{:.4}", rpf))
                                .size(SIZE_BODY)
                                .color(TEXT),
                        );
                        ui.label(egui::RichText::new(band).size(SIZE_SMALL).color(MUTED));
                    }
                    None => {
                        ui.label(egui::RichText::new("--").size(SIZE_BODY).color(MUTED));
                        ui.label(egui::RichText::new("--").size(SIZE_SMALL).color(MUTED));
                    }
                }

                if ui
                    .add_enabled(!running, egui::Button::new("Run"))
                    .on_hover_text("Optimise this input's phases only")
                    .clicked()
                {
                    run = Some(index);
                }
                if ui
                    .add_enabled(!running && n_rows > 1, egui::Button::new("Remove"))
                    .on_hover_text("Delete this input from the design")
                    .clicked()
                {
                    remove = Some(index);
                }
                ui.end_row();
            }
        });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!running, egui::Button::new("Add input"))
            .clicked()
        {
            // a new row copies the last one, so adding inputs is one click
            let mut spec = app
                .rows
                .last()
                .and_then(|r| r.parse().ok())
                .unwrap_or_default();
            spec.name = format!("u{}", app.rows.len() + 1);
            app.rows.push(SpecRow::from_spec(&spec));
            changed = true;
        }
        let (text, error) = app.status_line();
        ui.add_space(10.0);
        ui.label(egui::RichText::new(text).size(SIZE_SMALL).color(if error {
            DANGER
        } else {
            MUTED
        }));
    });
    ui.add_space(6.0);

    if let Some(i) = remove {
        app.rows.remove(i);
        changed = true;
    }
    if changed {
        app.mark_dirty();
    }
    if let Some(i) = run {
        app.start_optimize(ctx, Some(i));
    }
}

/// Keeps `InputSpec: Default` in scope for the Add button.
#[allow(dead_code)]
fn _default_spec() -> InputSpec {
    InputSpec::default()
}
