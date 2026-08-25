//! The input table: one row per excitation input, with a per-tone editor behind it.

use crate::theme::*;
use crate::{App, SpecRow, ToneRow};
use persistex_core::design::Tone;

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
    let mut edit: Option<usize> = None;
    let running = app.running;
    let n_rows = app.rows.len();

    egui::Grid::new("input_table")
        .num_columns(9)
        .spacing([8.0, 5.0])
        .show(ui, |ui| {
            for title in [
                "",
                "Input",
                "Peak",
                "Tones",
                "Band (Hz)",
                "RPF",
                "Allocated band",
                "",
                "",
            ] {
                ui.label(egui::RichText::new(title).size(SIZE_SMALL).color(MUTED));
            }
            ui.end_row();

            for index in 0..n_rows {
                let (rect, _) =
                    ui.allocate_exact_size(egui::Vec2::new(6.0, 20.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 1.0, series(index));

                let row: &mut SpecRow = &mut app.rows[index];
                changed |= ui
                    .add_enabled(
                        !running,
                        egui::TextEdit::singleline(&mut row.name).desired_width(78.0),
                    )
                    .changed();
                changed |= ui
                    .add_enabled(
                        !running,
                        egui::TextEdit::singleline(&mut row.peak).desired_width(56.0),
                    )
                    .changed();

                let count = row.tones.len();
                let band = row.band();
                if ui
                    .add_enabled(
                        !running,
                        egui::Button::new(
                            egui::RichText::new(format!("{} tones", count)).size(SIZE_SMALL),
                        ),
                    )
                    .on_hover_text("Edit this input's tones one by one")
                    .clicked()
                {
                    edit = Some(index);
                }
                match band {
                    Some((lo, hi)) => ui.label(
                        egui::RichText::new(format!("{:.4} - {:.4}", lo, hi))
                            .size(SIZE_SMALL)
                            .color(MUTED),
                    ),
                    None => ui.label(egui::RichText::new("--").size(SIZE_SMALL).color(MUTED)),
                };

                match app.metrics.get(index) {
                    Some((rpf, allocated)) => {
                        ui.label(
                            egui::RichText::new(format!("{:.4}", rpf))
                                .size(SIZE_BODY)
                                .color(TEXT),
                        );
                        ui.label(egui::RichText::new(allocated).size(SIZE_SMALL).color(MUTED));
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
            // a new input copies the last one's tones, ready to be edited
            let tones: Vec<ToneRow> = app
                .rows
                .last()
                .map(|r| r.tones.clone())
                .unwrap_or_else(|| vec![ToneRow::from_tone(&Tone::new(1.0, 1.0))]);
            let name = format!("u{}", app.rows.len() + 1);
            app.rows.push(SpecRow::new(&name, 1.0, tones));
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
        if app.editing == Some(i) {
            app.editing = None;
        }
        changed = true;
    }
    if let Some(i) = edit {
        app.editing = Some(i);
    }
    if changed {
        app.mark_dirty();
    }
    if let Some(i) = run {
        app.start_optimize(ctx, Some(i));
    }
}
