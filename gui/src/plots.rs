//! Plot rendering on egui's painter, which antialiases properly.

use crate::theme::*;
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2};

/// Points kept per pixel column when thinning a polyline.
///
/// Above 1 so the curve stays smooth, but not far above: sub-pixel segments are
/// near-degenerate and egui's tessellator turns them into visible beading along the
/// stroke. The cycle test upstream already guarantees several samples per cycle.
const POINT_BUDGET: usize = 2;

pub enum Trace {
    /// (sample index, value)
    Line(Vec<(f64, f64)>),
    /// (sample index, min, max)
    Band(Vec<(f64, f64, f64)>),
}

/// Reduce a trace to the plot's pixel budget.
///
/// `force_band` means the trace's *cycles* outrun the pixels, where a polyline is a
/// moire mess and a min/max envelope is the honest picture. Otherwise the trace is
/// drawn as a polyline, thinned by striding if there are far more samples than
/// pixels. Sample count alone must never select the envelope: raising the sample
/// rate does not make a waveform any harder to draw.
pub fn decimate(values: &[f64], columns: usize, force_band: bool) -> Trace {
    let n = values.len();
    if n == 0 {
        return Trace::Line(Vec::new());
    }
    if force_band {
        let mut out = Vec::with_capacity(columns);
        for c in 0..columns {
            let lo = c * n / columns;
            let hi = ((c + 1) * n / columns).max(lo + 1).min(n);
            let chunk = &values[lo..hi];
            let mn = chunk.iter().cloned().fold(f64::INFINITY, f64::min);
            let mx = chunk.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            out.push(((lo + hi) as f64 * 0.5, mn, mx));
        }
        return Trace::Band(out);
    }

    let target = (columns * POINT_BUDGET).max(2);
    if n <= target {
        return Trace::Line(
            values
                .iter()
                .enumerate()
                .map(|(i, v)| (i as f64, *v))
                .collect(),
        );
    }
    let stride = (n / target).max(2);
    let mut out: Vec<(f64, f64)> = (0..n)
        .step_by(stride)
        .map(|i| (i as f64, values[i]))
        .collect();
    if out.last().map(|p| p.0 as usize) != Some(n - 1) {
        out.push(((n - 1) as f64, values[n - 1])); // keep the trace full width
    }
    Trace::Line(out)
}

/// 1-2-5 ticks per decade, for a log axis.
pub fn log_ticks(lo: f64, hi: f64) -> Vec<f64> {
    if lo <= 0.0 || hi <= lo || !lo.is_finite() || !hi.is_finite() {
        return vec![lo];
    }
    let mut ticks = Vec::new();
    let first = lo.log10().floor() as i32;
    let last = hi.log10().ceil() as i32;
    for decade in first..=last {
        let base = 10f64.powi(decade);
        for m in [1.0, 2.0, 5.0] {
            let v = m * base;
            if v >= lo * 0.999 && v <= hi * 1.001 {
                ticks.push(v);
            }
        }
    }
    if ticks.is_empty() {
        ticks.push(lo);
    }
    ticks
}

pub fn nice_ticks(lo: f64, hi: f64, target: usize) -> Vec<f64> {
    if hi <= lo || !hi.is_finite() || !lo.is_finite() {
        return vec![lo];
    }
    let raw = (hi - lo) / target.max(1) as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let mut step = 10.0 * magnitude;
    for m in [1.0, 2.0, 2.5, 5.0] {
        if raw <= m * magnitude {
            step = m * magnitude;
            break;
        }
    }
    let mut ticks = Vec::new();
    let mut v = (lo / step - 1e-9).ceil() * step;
    while v <= hi + step * 1e-9 {
        ticks.push(if v.abs() < step * 1e-9 { 0.0 } else { v });
        v += step;
    }
    ticks
}

/// Log-axis labels: significant digits vary by decade, so format per value.
pub fn format_log_tick(value: f64) -> String {
    if value >= 1.0 {
        format!("{}", value)
    } else {
        let decimals = (-value.log10().floor()) as usize;
        format!("{:.*}", decimals.min(6), value)
    }
}

pub fn format_tick(value: f64, step: f64) -> String {
    if step >= 1000.0 || (value != 0.0 && value.abs() < 0.001) {
        return format!("{}", value);
    }
    let decimals = if step < 1.0 {
        ((-step.log10()).ceil() as i32 + 1).clamp(0, 4)
    } else {
        0
    };
    format!("{:.*}", decimals as usize, value)
}

/// A rectangular plotting region mapping data coordinates onto screen pixels.
pub struct Axes<'a> {
    pub painter: &'a Painter,
    pub rect: Rect,
    pub xlo: f64,
    pub xhi: f64,
    pub ylo: f64,
    pub yhi: f64,
    /// Map x logarithmically. A linear frequency axis is unreadable once the tones
    /// span more than a decade -- they pile up against the left edge.
    pub log_x: bool,
}

impl<'a> Axes<'a> {
    pub fn new(painter: &'a Painter, rect: Rect, xlim: (f64, f64), ylim: (f64, f64)) -> Self {
        let (xlo, mut xhi) = xlim;
        let (ylo, mut yhi) = ylim;
        if xhi <= xlo {
            xhi = xlo + 1.0;
        }
        if yhi <= ylo {
            yhi = ylo + 1.0;
        }
        Axes {
            painter,
            rect,
            xlo,
            xhi,
            ylo,
            yhi,
            log_x: false,
        }
    }

    pub fn with_log_x(mut self, log_x: bool) -> Self {
        if log_x && self.xlo <= 0.0 {
            self.xlo = self.xhi / 1000.0;
        }
        self.log_x = log_x;
        self
    }

    pub fn px(&self, x: f64) -> f32 {
        let f = if self.log_x {
            let lo = self.xlo.max(1e-12).log10();
            let hi = self.xhi.max(1e-12).log10();
            (x.max(1e-12).log10() - lo) / (hi - lo).max(1e-12)
        } else {
            (x - self.xlo) / (self.xhi - self.xlo)
        };
        self.rect.left() + f as f32 * self.rect.width()
    }

    pub fn py(&self, y: f64) -> f32 {
        self.rect.bottom() - ((y - self.ylo) / (self.yhi - self.ylo)) as f32 * self.rect.height()
    }

    #[allow(clippy::too_many_arguments)] // an axis genuinely has this many switches
    pub fn frame(
        &self,
        x_ticks: bool,
        y_ticks: bool,
        n_x: usize,
        n_y: usize,
        zero: bool,
        bottom: bool,
        left: bool,
    ) {
        self.painter.rect_filled(self.rect, 0.0, PANEL);

        let ticks = if self.log_x {
            log_ticks(self.xlo, self.xhi)
        } else {
            nice_ticks(self.xlo, self.xhi, n_x)
        };
        let step = if ticks.len() > 1 {
            ticks[1] - ticks[0]
        } else {
            1.0
        };
        for t in &ticks {
            let x = self.px(*t);
            self.painter.line_segment(
                [
                    Pos2::new(x, self.rect.top()),
                    Pos2::new(x, self.rect.bottom()),
                ],
                Stroke::new(THIN, GRID),
            );
            if x_ticks {
                self.painter.text(
                    Pos2::new(x, self.rect.bottom() + 4.0),
                    Align2::CENTER_TOP,
                    if self.log_x {
                        format_log_tick(*t)
                    } else {
                        format_tick(*t, step)
                    },
                    FontId::proportional(SIZE_SMALL),
                    MUTED,
                );
            }
        }

        if y_ticks {
            let ticks = nice_ticks(self.ylo, self.yhi, n_y);
            let step = if ticks.len() > 1 {
                ticks[1] - ticks[0]
            } else {
                1.0
            };
            for t in &ticks {
                let y = self.py(*t);
                self.painter.line_segment(
                    [
                        Pos2::new(self.rect.left(), y),
                        Pos2::new(self.rect.right(), y),
                    ],
                    Stroke::new(THIN, GRID),
                );
                self.painter.text(
                    Pos2::new(self.rect.left() - 6.0, y),
                    Align2::RIGHT_CENTER,
                    format_tick(*t, step),
                    FontId::proportional(SIZE_SMALL),
                    MUTED,
                );
            }
        }

        if zero && self.ylo < 0.0 && 0.0 < self.yhi {
            let y = self.py(0.0);
            self.painter.line_segment(
                [
                    Pos2::new(self.rect.left(), y),
                    Pos2::new(self.rect.right(), y),
                ],
                Stroke::new(THIN, ZERO),
            );
        }
        if bottom {
            self.painter.line_segment(
                [
                    Pos2::new(self.rect.left(), self.rect.bottom()),
                    Pos2::new(self.rect.right(), self.rect.bottom()),
                ],
                Stroke::new(THIN, FRAME),
            );
        }
        if left {
            self.painter.line_segment(
                [
                    Pos2::new(self.rect.left(), self.rect.top()),
                    Pos2::new(self.rect.left(), self.rect.bottom()),
                ],
                Stroke::new(THIN, FRAME),
            );
        }
    }

    pub fn polyline(&self, points: &[(f64, f64)], color: Color32, width: f32) {
        if points.len() < 2 {
            return;
        }
        let pts: Vec<Pos2> = points
            .iter()
            .map(|(x, y)| Pos2::new(self.px(*x), self.py(*y)))
            .collect();
        self.painter
            .add(Shape::line(pts, Stroke::new(width, color)));
    }

    /// Filled min/max envelope, drawn as translucent vertical spans so a concave
    /// outline never has to be tessellated.
    pub fn band(&self, points: &[(f64, f64, f64)], color: Color32) {
        let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 70);
        for (x, lo, hi) in points {
            let px = self.px(*x);
            self.painter.line_segment(
                [Pos2::new(px, self.py(*hi)), Pos2::new(px, self.py(*lo))],
                Stroke::new(1.5_f32, fill),
            );
        }
        let upper: Vec<(f64, f64)> = points.iter().map(|(x, _, hi)| (*x, *hi)).collect();
        let lower: Vec<(f64, f64)> = points.iter().map(|(x, lo, _)| (*x, *lo)).collect();
        self.polyline(&upper, color, THIN);
        self.polyline(&lower, color, THIN);
    }

    pub fn stems(&self, xs: &[f64], ys: &[f64], color: Color32) {
        let base = self.py(0.0);
        let light = tint(color, 0.35);
        for (x, y) in xs.iter().zip(ys) {
            let px = self.px(*x);
            self.painter.line_segment(
                [Pos2::new(px, base), Pos2::new(px, self.py(*y))],
                Stroke::new(THIN, light),
            );
        }
        for (x, y) in xs.iter().zip(ys) {
            self.painter
                .circle_filled(Pos2::new(self.px(*x), self.py(*y)), 3.0, color);
        }
    }

    pub fn hline_dashed(&self, y: f64, color: Color32) {
        let py = self.py(y);
        self.painter.add(Shape::dashed_line(
            &[
                Pos2::new(self.rect.left(), py),
                Pos2::new(self.rect.right(), py),
            ],
            Stroke::new(THIN, color),
            4.0,
            4.0,
        ));
    }

    pub fn vline_dashed(&self, x: f64, color: Color32) {
        let px = self.px(x);
        self.painter.add(Shape::dashed_line(
            &[
                Pos2::new(px, self.rect.top()),
                Pos2::new(px, self.rect.bottom()),
            ],
            Stroke::new(THIN, color),
            2.0,
            5.0,
        ));
    }

    pub fn text(&self, pos: Pos2, align: Align2, text: &str, size: f32, color: Color32) {
        self.painter
            .text(pos, align, text, FontId::proportional(size), color);
    }
}

pub fn label(painter: &Painter, pos: Pos2, align: Align2, text: &str, size: f32, color: Color32) {
    painter.text(pos, align, text, FontId::proportional(size), color);
}

pub fn _unused(_: Vec2) {}
