//! CSV signals and the JSON design artifact.
//!
//! The artifact is the contract between design and playback: this crate produces it,
//! the aircraft plays it back, and an estimator reads it to know which bins to watch.
//! Hand-rolled serialisation keeps the crate dependency-free.

use crate::design::Design;
use crate::sha256;
use std::io::{self, Write};

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Compact number formatting that round-trips and drops trailing zeros.
fn num(v: f64) -> String {
    if !v.is_finite() {
        return "null".into();
    }
    if v == v.trunc() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let mut s = format!("{:.10}", v);
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

fn list<T, F: Fn(&T) -> String>(items: &[T], f: F) -> String {
    let parts: Vec<String> = items.iter().map(|v| f(v)).collect();
    format!("[{}]", parts.join(","))
}

/// Build the artifact JSON. Returns the pretty-printed document.
pub fn artifact_json(design: &mut Design, created_utc: &str) -> String {
    let mut channels = Vec::new();
    for (i, spec) in design.specs.clone().iter().enumerate() {
        let ch = &mut design.channels[i];
        let freqs = ch.frequencies();
        let amps = ch.scaled_amplitudes();
        let rpf = ch.rpf();
        channels.push(format!(
            "{{\"name\":\"{}\",\"bins\":{},\"frequencies_hz\":{},\"amplitudes\":{},\
             \"phases_rad\":{},\"peak_limit\":{},\"rpf\":{},\"spec\":{{\"f_min_hz\":{},\
             \"f_max_hz\":{},\"n_tones\":{},\"shape\":\"{}\",\"spacing\":\"{}\"}}}}",
            escape(&ch.name),
            list(&ch.bins, |k| k.to_string()),
            list(&freqs, |v| num(*v)),
            list(&amps, |v| num(*v)),
            list(&ch.phases, |v| num(*v)),
            num(ch.peak_limit),
            num(rpf),
            num(spec.f_min),
            num(spec.f_max),
            spec.n_tones,
            escape(spec.shape.label()),
            escape(spec.spacing.label()),
        ));
    }

    let body = format!(
        "{{\"format\":\"persistex.excitation\",\"version\":1,\"created_utc\":\"{}\",\
         \"sample_rate_hz\":{},\"fundamental_hz\":{},\"record_length_s\":{},\
         \"n_periods\":{},\"duration_s\":{},\"n_samples\":{},\"bin_mode\":\"{}\",\
         \"channels\":[{}]}}",
        escape(created_utc),
        num(design.fs),
        num(design.f0),
        num(design.record_length()),
        design.n_periods,
        num(design.duration()),
        design.n_samples(),
        escape(design.bin_mode.label()),
        channels.join(",")
    );

    let digest = sha256::hex(body.as_bytes());
    // Insert the digest without disturbing the hashed bytes.
    format!("{}, \"sha256\": \"{}\"}}", &body[..body.len() - 1], digest)
}

pub fn write_json(design: &mut Design, created_utc: &str, path: &std::path::Path) -> io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(artifact_json(design, created_utc).as_bytes())?;
    file.write_all(b"\n")
}

pub fn write_csv(design: &mut Design, path: &std::path::Path) -> io::Result<()> {
    let fs = design.fs;
    let periods = design.n_periods;
    let columns: Vec<Vec<f64>> = (0..design.channels.len())
        .map(|i| design.channels[i].samples(fs, periods))
        .collect();
    let names: Vec<String> = design.channels.iter().map(|c| c.name.clone()).collect();
    let n = design.n_samples();

    let file = std::fs::File::create(path)?;
    let mut out = io::BufWriter::new(file);
    writeln!(out, "time,{}", names.join(","))?;
    for i in 0..n {
        write!(out, "{}", num(i as f64 / fs))?;
        for column in &columns {
            write!(out, ",{}", num(column[i]))?;
        }
        out.write_all(b"\n")?;
    }
    out.flush()
}

/// Current UTC timestamp as ISO-8601, without pulling in a date crate.
pub fn now_utc_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);

    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60
    )
}
