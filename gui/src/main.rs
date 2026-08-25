//! persistex -- multisine excitation designer.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod plots;
mod table;
mod theme;
mod tones;

use egui::{Align2, Pos2, Rect, Vec2};
use persistex_core::design::{build_design, Design, InputSpec, Tone};
use persistex_core::export;
use persistex_core::optimize::{optimize_design, Effort, Progress};
use plots::{decimate, Axes, Trace};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use theme::*;
use tones::GeneratorRow;

/// One tone as edited: frequency and period are kept in step, either may be typed.
#[derive(Clone)]
pub struct ToneRow {
    pub frequency: String,
    pub period: String,
    pub amplitude: String,
}

impl ToneRow {
    pub fn new(frequency: f64, amplitude: f64) -> Self {
        let mut row = ToneRow {
            frequency: fmt(frequency),
            period: String::new(),
            amplitude: fmt(amplitude),
        };
        row.sync_from_frequency();
        row
    }

    pub fn from_tone(t: &Tone) -> Self {
        ToneRow::new(t.frequency, t.amplitude)
    }

    pub fn frequency_value(&self) -> Option<f64> {
        self.frequency
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| *v > 0.0)
    }

    /// Period follows frequency.
    pub fn sync_from_frequency(&mut self) {
        self.period = match self.frequency_value() {
            Some(f) => fmt(1.0 / f),
            None => String::new(),
        };
    }

    /// ...and frequency follows period, so either column can be the one typed in.
    pub fn sync_from_period(&mut self) {
        if let Ok(p) = self.period.trim().parse::<f64>() {
            if p > 0.0 {
                self.frequency = fmt(1.0 / p);
            }
        }
    }

    pub fn parse(&self) -> Option<Tone> {
        let f = self.frequency_value()?;
        let a = self.amplitude.trim().parse::<f64>().ok()?;
        Some(Tone::new(f, a))
    }
}

/// Editable string form of an `InputSpec`.
#[derive(Clone)]
pub struct SpecRow {
    pub name: String,
    pub peak: String,
    pub tones: Vec<ToneRow>,
    pub generator: GeneratorRow,
}

impl SpecRow {
    pub fn new(name: &str, peak: f64, tones: Vec<ToneRow>) -> Self {
        SpecRow {
            name: name.into(),
            peak: fmt(peak),
            tones,
            generator: GeneratorRow::default(),
        }
    }

    pub fn parse(&self) -> Result<InputSpec, String> {
        let peak = self
            .peak
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("{}: peak is not a number", self.name))?;
        let tones: Vec<Tone> = self.tones.iter().filter_map(|t| t.parse()).collect();
        if tones.is_empty() {
            return Err(format!("{}: add at least one tone", self.name));
        }
        Ok(InputSpec {
            name: self.name.trim().to_string(),
            peak_limit: peak,
            tones,
        })
    }

    pub fn band(&self) -> Option<(f64, f64)> {
        let mut values: Vec<f64> = self
            .tones
            .iter()
            .filter_map(|t| t.frequency_value())
            .collect();
        if values.is_empty() {
            return None;
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Some((values[0], values[values.len() - 1]))
    }
}

/// Compact, round-trippable formatting for the editable fields.
fn fmt(v: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    if v == v.trunc() && v.abs() < 1e12 {
        return format!("{}", v as i64);
    }
    let mut s = format!("{:.6}", v);
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

enum Msg {
    Progress(f64, String),
    Preview(usize, Vec<f64>, f64),
    Done(Vec<Vec<f64>>),
}

struct Sink {
    tx: Sender<Msg>,
    ctx: egui::Context,
}

impl Progress for Sink {
    fn progress(&mut self, fraction: f64, message: &str) {
        let _ = self.tx.send(Msg::Progress(fraction, message.to_string()));
        self.ctx.request_repaint();
    }
    fn preview(&mut self, channel: usize, signal: &[f64], rpf: f64) {
        let _ = self.tx.send(Msg::Preview(channel, signal.to_vec(), rpf));
        self.ctx.request_repaint();
    }
}

pub struct App {
    pub rows: Vec<SpecRow>,
    record: String,
    repeats: String,
    fs: String,
    effort: Effort,

    design: Option<Design>,
    timeseries: Vec<Vec<f64>>,
    pub metrics: Vec<(f64, String)>,
    previews: HashMap<usize, (Vec<f64>, f64)>,

    status: String,
    error: Option<String>,
    info: Vec<(String, bool)>,

    rx: Option<Receiver<Msg>>,
    cancel: Arc<AtomicBool>,
    pub running: bool,
    progress: f32,
    tab: usize,
    dirty: bool,
    /// Which input's tone editor is open, if any.
    pub editing: Option<usize>,
}

impl Default for App {
    fn default() -> Self {
        // two inputs interleaved over the same band, as a starting point
        let f0 = 1.0 / 30.0;
        let mut taken: HashSet<usize> = HashSet::new();
        let mut rows = Vec::new();
        for name in ["ail", "ele"] {
            let generator = GeneratorRow::default();
            let tones = match generator.parse() {
                Some(spec) => persistex_core::design::generate_tones(&spec, f0, &taken),
                None => Vec::new(),
            };
            for t in &tones {
                taken.insert((t.frequency / f0).round() as usize);
            }
            rows.push(SpecRow::new(
                name,
                1.0,
                tones.iter().map(ToneRow::from_tone).collect(),
            ));
        }
        App {
            rows,
            record: "30".into(),
            repeats: "2".into(),
            fs: "100".into(),
            effort: Effort::Standard,
            design: None,
            timeseries: Vec::new(),
            metrics: Vec::new(),
            previews: HashMap::new(),
            status: String::new(),
            error: None,
            info: Vec::new(),
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            running: false,
            progress: 0.0,
            tab: 0,
            dirty: true,
            editing: None,
        }
    }
}

impl App {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn rebuild(&mut self) {
        self.dirty = false;
        if self.running {
            return;
        }
        let specs: Result<Vec<InputSpec>, String> = self.rows.iter().map(|r| r.parse()).collect();
        let specs = match specs {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e);
                self.metrics.clear();
                return;
            }
        };
        let record = self.record.trim().parse::<f64>();
        let repeats = self.repeats.trim().parse::<usize>();
        let fs = self.fs.trim().parse::<f64>();
        let (record, repeats, fs) = match (record, repeats, fs) {
            (Ok(a), Ok(b), Ok(c)) => (a, b, c),
            _ => {
                self.error = Some("record length, repeats and sample rate must be numbers".into());
                self.metrics.clear();
                return;
            }
        };

        match build_design(&specs, record, repeats, fs) {
            Ok(design) => {
                self.design = Some(design);
                self.error = None;
                self.previews.clear();
                let n = self.design.as_ref().unwrap().channels.len();
                let tones = self.design.as_ref().unwrap().used_bins();
                self.refresh();
                self.status = format!("{} inputs, {} tones -- not yet optimised", n, tones);
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.metrics.clear();
            }
        }
    }

    fn refresh(&mut self) {
        let Some(design) = self.design.as_mut() else {
            return;
        };
        let fs = design.fs;
        let periods = design.n_periods;
        self.timeseries = (0..design.channels.len())
            .map(|i| design.channels[i].samples(fs, periods))
            .collect();
        self.metrics = (0..design.channels.len())
            .map(|i| {
                let f = design.channels[i].frequencies();
                let n = design.channels[i].n_tones();
                (
                    design.channels[i].rpf(),
                    format!("{:.3} - {:.3} Hz  ({})", f[0], f[f.len() - 1], n),
                )
            })
            .collect();

        let used = design.used_bins();
        let mut info = vec![
            (
                format!(
                    "f0 = {:.4} Hz  (1 / {:.4} s)",
                    design.f0,
                    design.record_length()
                ),
                false,
            ),
            (
                format!("{} tones across {} input(s)", used, design.channels.len()),
                false,
            ),
            (
                format!(
                    "{:.4} s = {} x {:.4} s",
                    design.duration(),
                    design.n_periods,
                    design.record_length()
                ),
                false,
            ),
            (
                format!("{} samples at {:.4} Hz", design.n_samples(), design.fs),
                false,
            ),
            (format!("nyquist {:.4} Hz", design.nyquist()), false),
        ];
        // Anything questionable is reported, never refused.
        for warning in &design.warnings {
            info.push((warning.clone(), true));
        }
        self.info = info;
    }

    pub fn start_optimize(&mut self, ctx: &egui::Context, only: Option<usize>) {
        if self.running || self.design.is_none() {
            return;
        }
        if self.dirty {
            self.rebuild();
            if self.design.is_none() {
                return;
            }
        }
        self.cancel.store(false, Ordering::Relaxed);
        self.previews.clear();
        self.progress = 0.0;
        self.running = true;

        let mut work = self.design.clone().unwrap();
        let effort = self.effort;
        let cancel = self.cancel.clone();
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let mut sink = Sink {
            tx: tx.clone(),
            ctx: ctx.clone(),
        };

        std::thread::spawn(move || {
            let targets = only.map(|i| vec![i]);
            let flag = cancel.clone();
            let is_cancelled = move || flag.load(Ordering::Relaxed);
            optimize_design(
                &mut work,
                effort,
                targets.as_deref(),
                &is_cancelled,
                &mut sink,
            );
            let phases = work.channels.iter().map(|c| c.phases.clone()).collect();
            let _ = tx.send(Msg::Done(phases));
            sink.ctx.request_repaint();
        });
    }

    fn drain(&mut self) {
        let mut done: Option<Vec<Vec<f64>>> = None;
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Msg::Progress(f, m) => {
                        self.progress = f as f32;
                        self.status = m;
                    }
                    Msg::Preview(i, sig, rpf) => {
                        self.previews.insert(i, (sig, rpf));
                    }
                    Msg::Done(phases) => done = Some(phases),
                }
            }
        }
        if let Some(phases) = done {
            self.running = false;
            self.rx = None;
            self.previews.clear();
            if self.cancel.load(Ordering::Relaxed) {
                self.status = "stopped -- phases unchanged".into();
                self.refresh();
                return;
            }
            if let Some(design) = self.design.as_mut() {
                for (ch, ph) in design.channels.iter_mut().zip(phases) {
                    ch.set_phases(ph);
                }
            }
            self.progress = 1.0;
            self.refresh();
            let worst = self
                .metrics
                .iter()
                .map(|m| m.0)
                .fold(f64::NEG_INFINITY, f64::max);
            self.status = format!(
                "optimised -- worst RPF {:.4} across {} inputs",
                worst,
                self.metrics.len()
            );
        }
    }

    /// The samples to draw: the live preview while optimising, else the record.
    fn trace(&self, index: usize, peak_limit: f64, n_periods: usize) -> Vec<f64> {
        if let Some((period, _)) = self.previews.get(&index) {
            let peak = period.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-12);
            let gain = peak_limit / peak;
            let mut out = Vec::with_capacity(period.len() * n_periods);
            for _ in 0..n_periods {
                out.extend(period.iter().map(|v| v * gain));
            }
            return out;
        }
        self.timeseries.get(index).cloned().unwrap_or_default()
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl App {
    /// The whole UI tree, independent of eframe, so it can be driven headlessly.
    pub fn ui(&mut self, ctx: &egui::Context) {
        self.drain();
        if self.dirty {
            self.rebuild();
        }

        egui::SidePanel::left("side")
            .exact_width(228.0)
            .resizable(false)
            .show(ctx, |ui| self.sidebar(ui, ctx));

        egui::TopBottomPanel::bottom("inputs")
            .resizable(false)
            .show(ctx, |ui| table::show(self, ui, ctx));

        self.show_tone_editor(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, 0, "  Time domain  ");
                ui.selectable_value(&mut self.tab, 1, "  Spectrum  ");
            });
            ui.add_space(4.0);
            let size = ui.available_size();
            let (_id, rect) = ui.allocate_space(size);
            let painter = ui.painter_at(rect);
            if self.tab == 0 {
                self.draw_time(&painter, rect);
            } else {
                self.draw_spectrum(&painter, rect);
            }
        });
    }
}

impl App {
    fn sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("persistex")
                .size(SIZE_TITLE)
                .strong()
                .color(TEXT),
        );
        ui.label(
            egui::RichText::new("multisine excitation design")
                .size(SIZE_SMALL)
                .color(MUTED),
        );
        ui.add_space(10.0);

        let mut changed = false;
        ui.group(|ui| {
            ui.set_width(200.0);
            ui.label(
                egui::RichText::new("Record")
                    .size(SIZE_HEAD)
                    .strong()
                    .color(TEXT),
            );
            ui.add_space(3.0);
            changed |= field(ui, "Length (s)", &mut self.record);
            changed |= field(ui, "Repeats", &mut self.repeats);
            changed |= field(ui, "Sample rate", &mut self.fs);
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.set_width(200.0);
            ui.label(
                egui::RichText::new("Optimise")
                    .size(SIZE_HEAD)
                    .strong()
                    .color(TEXT),
            );
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.add_sized(
                    [84.0, 18.0],
                    egui::Label::new(egui::RichText::new("Effort").size(SIZE_SMALL).color(TEXT)),
                );
                egui::ComboBox::from_id_salt("effort")
                    .selected_text(self.effort.label())
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        for e in Effort::ALL {
                            ui.selectable_value(&mut self.effort, e, e.label());
                        }
                    });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let can_run = !self.running && self.design.is_some();
                if ui
                    .add_enabled(
                        can_run,
                        egui::Button::new("Optimise all").min_size(Vec2::new(112.0, 24.0)),
                    )
                    .clicked()
                {
                    self.start_optimize(ctx, None);
                }
                if ui
                    .add_enabled(
                        self.running,
                        egui::Button::new("Stop").min_size(Vec2::new(60.0, 24.0)),
                    )
                    .clicked()
                {
                    self.cancel.store(true, Ordering::Relaxed);
                    self.status = "stopping...".into();
                }
            });
            ui.add_space(4.0);
            ui.add(egui::ProgressBar::new(self.progress).desired_height(6.0));
        });

        ui.add_space(8.0);
        ui.group(|ui| {
            ui.set_width(200.0);
            ui.label(
                egui::RichText::new("Export")
                    .size(SIZE_HEAD)
                    .strong()
                    .color(TEXT),
            );
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                if ui
                    .add_sized([88.0, 22.0], egui::Button::new("CSV"))
                    .clicked()
                {
                    self.save_csv();
                }
                if ui
                    .add_sized([88.0, 22.0], egui::Button::new("JSON"))
                    .clicked()
                {
                    self.save_json();
                }
            });
        });

        ui.add_space(10.0);
        egui::Frame::new()
            .fill(PANEL)
            .stroke(egui::Stroke::new(1.0_f32, FRAME))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.set_width(184.0);
                for (line, warn) in &self.info {
                    ui.label(egui::RichText::new(line).size(SIZE_SMALL).color(if *warn {
                        WARN
                    } else {
                        MUTED
                    }));
                    ui.add_space(2.0);
                }
            });

        if changed {
            self.dirty = true;
        }
    }

    /// The per-tone editor for whichever input was clicked.
    fn show_tone_editor(&mut self, ctx: &egui::Context) {
        let Some(index) = self.editing else { return };
        if index >= self.rows.len() {
            self.editing = None;
            return;
        }
        let f0 = self
            .record
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| *v > 0.0)
            .map(|v| 1.0 / v)
            .unwrap_or(1.0 / 30.0);

        // bins already used by the *other* inputs, so a generated set steps around
        // them and the inputs stay orthogonal over the record
        let mut taken: HashSet<usize> = HashSet::new();
        for (i, row) in self.rows.iter().enumerate() {
            if i == index {
                continue;
            }
            for tone in &row.tones {
                if let Some(f) = tone.frequency_value() {
                    taken.insert((f / f0).round().max(1.0) as usize);
                }
            }
        }

        let mut row = self.rows[index].clone();
        let outcome = tones::show(ctx, &mut row, index, f0, &taken);
        self.rows[index] = row;
        if outcome.changed {
            self.dirty = true;
        }
        if outcome.close {
            self.editing = None;
        }
    }

    fn save_csv(&mut self) {
        let Some(design) = self.design.as_mut() else {
            return;
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("excitation.csv")
            .add_filter("CSV", &["csv"])
            .save_file()
        {
            let aliased = design.aliased_tones();
            match export::write_csv(design, &path) {
                Ok(()) => {
                    self.status = if aliased.is_empty() {
                        format!("wrote {}", path.display())
                    } else {
                        format!(
                            "wrote {} -- warning: {} tone(s) at or above Nyquist will alias",
                            path.display(),
                            aliased.len()
                        )
                    }
                }
                Err(e) => self.status = format!("could not write: {}", e),
            }
        }
    }

    fn save_json(&mut self) {
        let Some(design) = self.design.as_mut() else {
            return;
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("excitation.json")
            .add_filter("JSON", &["json"])
            .save_file()
        {
            let stamp = export::now_utc_iso();
            match export::write_json(design, &stamp, &path) {
                Ok(()) => self.status = format!("wrote {}", path.display()),
                Err(e) => self.status = format!("could not write: {}", e),
            }
        }
    }

    fn draw_time(&mut self, painter: &egui::Painter, rect: Rect) {
        painter.rect_filled(rect, 0.0, BG);
        let Some(design) = self.design.as_ref() else {
            return;
        };
        let n = design.channels.len();
        if n == 0 || self.timeseries.is_empty() {
            return;
        }

        let (left, right, top, bottom) = (92.0f32, 22.0f32, 10.0f32, 36.0f32);
        let strip = (rect.height() - top - bottom) / n as f32;
        let duration = design.duration();
        let periods = design.n_periods;
        let record_length = design.record_length();

        let channels: Vec<(String, f64, usize)> = design
            .channels
            .iter()
            .map(|c| {
                (
                    c.name.clone(),
                    c.peak_limit,
                    *c.bins.iter().max().unwrap_or(&1),
                )
            })
            .collect();

        for (index, (name, peak_limit, k_max)) in channels.iter().enumerate() {
            let y0 = rect.top() + top + index as f32 * strip;
            let y1 = y0 + strip - 12.0;
            let plot = Rect::from_min_max(
                Pos2::new(rect.left() + left, y0),
                Pos2::new(rect.right() - right, y1),
            );
            let limit = peak_limit * 1.3;
            let axes = Axes::new(painter, plot, (0.0, duration), (-limit, limit));
            // No y tick labels: they would collide with the channel gutter, and the
            // peak limit annotated on its own line says more than -1/0/1 would.
            axes.frame(index == n - 1, false, 6, 2, true, index == n - 1, true);

            let colour = series(index);
            axes.hline_dashed(*peak_limit, tint(DANGER, 0.5));
            axes.hline_dashed(-peak_limit, tint(DANGER, 0.5));
            axes.text(
                Pos2::new(plot.right() - 4.0, axes.py(*peak_limit) - 8.0),
                Align2::RIGHT_CENTER,
                &format!("±{}", fmt(*peak_limit)),
                SIZE_SMALL,
                tint(DANGER, 0.25),
            );
            for p in 1..periods {
                axes.vline_dashed(p as f64 * record_length, FAINT);
            }

            let signal = self.trace(index, *peak_limit, periods);
            if signal.len() > 1 {
                let columns = plot.width().max(60.0) as usize;
                // cycles, not samples, decide whether a polyline can still be read
                let cycles = k_max * periods;
                let scale = duration / (signal.len() - 1) as f64;
                match decimate(&signal, columns, (cycles as f64) * 2.5 > columns as f64) {
                    Trace::Line(points) => {
                        let mapped: Vec<(f64, f64)> =
                            points.iter().map(|(x, y)| (x * scale, *y)).collect();
                        axes.polyline(&mapped, colour, TRACE_WIDTH);
                    }
                    Trace::Band(points) => {
                        let mapped: Vec<(f64, f64, f64)> = points
                            .iter()
                            .map(|(x, lo, hi)| (x * scale, *lo, *hi))
                            .collect();
                        axes.band(&mapped, colour);
                    }
                }
            }

            let live = self.previews.contains_key(&index);
            let rpf = self
                .previews
                .get(&index)
                .map(|(_, r)| *r)
                .or_else(|| self.metrics.get(index).map(|m| m.0))
                .unwrap_or(f64::NAN);
            let mid = (y0 + y1) * 0.5;
            plots::label(
                painter,
                Pos2::new(plot.left() - 14.0, mid - 8.0),
                Align2::RIGHT_CENTER,
                name,
                SIZE_HEAD,
                colour,
            );
            plots::label(
                painter,
                Pos2::new(plot.left() - 14.0, mid + 8.0),
                Align2::RIGHT_CENTER,
                &format!("RPF {:.3}", rpf),
                SIZE_SMALL,
                if live { colour } else { MUTED },
            );
        }

        plots::label(
            painter,
            Pos2::new(rect.center().x, rect.bottom() - 12.0),
            Align2::CENTER_CENTER,
            "time (s)",
            SIZE_SMALL,
            MUTED,
        );
    }

    fn draw_spectrum(&mut self, painter: &egui::Painter, rect: Rect) {
        painter.rect_filled(rect, 0.0, BG);
        let Some(design) = self.design.as_mut() else {
            return;
        };
        if design.channels.is_empty() {
            return;
        }

        let mut series_data: Vec<(String, Vec<f64>, Vec<f64>)> = Vec::new();
        for i in 0..design.channels.len() {
            let f = design.channels[i].frequencies();
            let a = design.channels[i].scaled_amplitudes();
            series_data.push((design.channels[i].name.clone(), f, a));
        }
        let peak = series_data
            .iter()
            .flat_map(|(_, _, a)| a.iter())
            .fold(0.0f64, |m, v| m.max(*v));
        let f_lo = series_data
            .iter()
            .map(|(_, f, _)| f[0])
            .fold(f64::INFINITY, f64::min);
        let f_hi = series_data
            .iter()
            .map(|(_, f, _)| f[f.len() - 1])
            .fold(f64::NEG_INFINITY, f64::max);
        let span = (f_hi - f_lo).max(1e-9);

        let plot = Rect::from_min_max(
            Pos2::new(rect.left() + 70.0, rect.top() + 18.0),
            Pos2::new(rect.right() - 22.0, rect.bottom() - 40.0),
        );
        let axes = Axes::new(
            painter,
            plot,
            ((f_lo - span * 0.06).max(0.0), f_hi + span * 0.06),
            (0.0, peak * 1.2),
        );
        axes.frame(true, true, 6, 4, false, true, true);

        for (index, (_, f, a)) in series_data.iter().enumerate() {
            axes.stems(f, a, series(index));
        }

        plots::label(
            painter,
            Pos2::new(rect.center().x, rect.bottom() - 14.0),
            Align2::CENTER_CENTER,
            "frequency (Hz)",
            SIZE_SMALL,
            MUTED,
        );
        plots::label(
            painter,
            Pos2::new(plot.left(), rect.top() + 6.0),
            Align2::LEFT_CENTER,
            "amplitude",
            SIZE_SMALL,
            MUTED,
        );

        for (index, (name, _, _)) in series_data.iter().enumerate() {
            let y = plot.top() + 12.0 + index as f32 * 16.0;
            let x = plot.right() - 10.0;
            painter.circle_filled(Pos2::new(x - 18.0, y), 3.5, series(index));
            plots::label(
                painter,
                Pos2::new(x - 26.0, y),
                Align2::RIGHT_CENTER,
                name,
                SIZE_SMALL,
                TEXT,
            );
        }
    }

    pub fn status_line(&self) -> (&str, bool) {
        match &self.error {
            Some(e) => (e.as_str(), true),
            None => (self.status.as_str(), false),
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized(
            [84.0, 18.0],
            egui::Label::new(egui::RichText::new(label).size(SIZE_SMALL).color(TEXT)),
        );
        changed = ui
            .add_sized([96.0, 20.0], egui::TextEdit::singleline(value))
            .changed();
    });
    changed
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1340.0, 880.0])
            .with_min_inner_size([1140.0, 700.0])
            .with_title("persistex"),
        ..Default::default()
    };
    eframe::run_native(
        "persistex",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            Ok(Box::new(App::default()))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drives the full UI tree without a window, which catches layout panics and
    /// widget id collisions that only surface once every branch is rendered.
    fn run(app: &mut App, frames: usize) {
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::light());
        for _ in 0..frames {
            let _ = ctx.run(egui::RawInput::default(), |c| app.ui(c));
        }
    }

    #[test]
    fn builds_a_default_design_and_renders() {
        let mut app = App::default();
        run(&mut app, 3);
        assert!(app.design.is_some(), "no design: {:?}", app.error);
        assert_eq!(app.metrics.len(), 2);
        assert!(app
            .metrics
            .iter()
            .all(|(rpf, _)| rpf.is_finite() && *rpf > 0.0));
        // the two default inputs are generated interleaved, so no shared bins
        assert!(app.design.as_ref().unwrap().shared_bins().is_empty());
    }

    #[test]
    fn both_tabs_and_the_tone_editor_render() {
        let mut app = App::default();
        run(&mut app, 2);
        app.tab = 1;
        run(&mut app, 2);
        app.editing = Some(0);
        run(&mut app, 2);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.editing, Some(0));
    }

    #[test]
    fn frequency_and_period_stay_in_step() {
        let mut t = ToneRow::new(0.25, 1.0);
        assert_eq!(t.period, "4");
        t.period = "2".into();
        t.sync_from_period();
        assert_eq!(t.frequency, "0.5");
        t.frequency = "4".into();
        t.sync_from_frequency();
        assert_eq!(t.period, "0.25");
    }

    #[test]
    fn editing_a_tone_rebuilds_the_design() {
        let mut app = App::default();
        run(&mut app, 2);
        app.rows[0].tones.truncate(3);
        app.rows[0].tones[0].frequency = "0.25".into();
        app.rows[0].tones[0].sync_from_frequency();
        app.mark_dirty();
        run(&mut app, 2);
        let d = app.design.as_ref().unwrap();
        assert_eq!(d.channels[0].n_tones(), 3);
        // snapped to the nearest harmonic of f0
        let got = d.channels[0].frequencies();
        assert!(
            got.iter().any(|f| (f - 0.25).abs() < 0.5 / 30.0 + 1e-9),
            "{got:?}"
        );
    }

    #[test]
    fn two_tones_on_the_same_harmonic_collapse_with_a_warning() {
        let mut app = App::default();
        run(&mut app, 2);
        // 0.7 and 0.701 Hz both land on bin 21 at f0 = 1/30
        app.rows[0].tones = vec![ToneRow::new(0.7, 1.0), ToneRow::new(0.701, 1.0)];
        app.mark_dirty();
        run(&mut app, 2);
        let d = app.design.as_ref().unwrap();
        assert_eq!(d.channels[0].n_tones(), 1);
        assert!(
            d.warnings.iter().any(|w| w.contains("collides")),
            "{:?}",
            d.warnings
        );
    }

    #[test]
    fn arbitrary_tone_sets_are_accepted() {
        let mut app = App::default();
        run(&mut app, 2);
        app.rows[0].tones = ["0.37", "1.9", "4.4"]
            .iter()
            .map(|f| ToneRow::new(f.parse().unwrap(), 1.0))
            .collect();
        app.mark_dirty();
        run(&mut app, 2);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.design.as_ref().unwrap().channels[0].n_tones(), 3);
    }

    #[test]
    fn above_nyquist_warns_but_still_builds() {
        let mut app = App::default();
        run(&mut app, 2);
        app.rows[0].tones = vec![ToneRow::new(80.0, 1.0)];
        app.mark_dirty();
        run(&mut app, 2);
        assert!(
            app.error.is_none(),
            "should not be refused: {:?}",
            app.error
        );
        let d = app.design.as_ref().unwrap();
        assert!(d.warnings.iter().any(|w| w.contains("Nyquist")));
        assert!(app
            .info
            .iter()
            .any(|(t, warn)| *warn && t.contains("Nyquist")));
    }

    #[test]
    fn bad_input_surfaces_an_error_without_panicking() {
        let mut app = App::default();
        run(&mut app, 2);
        app.rows[1].tones.clear();
        app.mark_dirty();
        run(&mut app, 2);
        assert!(
            app.error.as_ref().unwrap().contains("ele"),
            "{:?}",
            app.error
        );

        app.rows[1].tones = vec![ToneRow::new(1.0, 1.0)];
        app.mark_dirty();
        run(&mut app, 2);
        assert!(app.error.is_none(), "{:?}", app.error);
    }

    #[test]
    fn high_sample_rates_keep_drawing_one_trace() {
        // regression: choosing the envelope on sample count rendered its two
        // boundary lines in place of the single real trace
        let mut app = App::default();
        for fs in ["50", "100", "200", "1000"] {
            app.fs = fs.into();
            app.mark_dirty();
            run(&mut app, 2);
            let design = app.design.as_ref().unwrap();
            let k_max = *design.channels[0].bins.iter().max().unwrap();
            let signal = app.trace(0, 1.0, design.n_periods);
            let columns = 900usize;
            let force = (k_max * design.n_periods) as f64 * 2.5 > columns as f64;
            assert!(!force, "fs={fs} wrongly forced the envelope");
            assert!(matches!(decimate(&signal, columns, force), Trace::Line(_)));
        }
    }
}
