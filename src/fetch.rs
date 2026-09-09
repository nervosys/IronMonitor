// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! A neofetch-style summary: logo on the left, the machine on the right.
//!
//! # What this is for
//!
//! One screen a person can read at a glance, and paste into an issue. It is the
//! least precise surface IronMonitor has and the most looked at, which is exactly why
//! it must not round an absence into a plausible number.
//!
//! # Unknown is printed as unknown
//!
//! Every other tool of this shape prints a dash, a zero, or omits the line. This
//! one prints the reason where there is room for it, because the difference
//! between "this machine has no GPU" and "IronMonitor cannot read GPUs on this
//! platform" is the difference between buying hardware and filing a bug.
//!
//! The rendering is deliberately built from [`crate::ontology::resolve`] rather
//! than from the individual monitors. One resolver means one set of answers: a
//! summary that disagreed with `ironmon snapshot` about the same machine would
//! make both untrustworthy, and the summary is the one people quote.

use crate::ontology::resolve::Reading;
use std::collections::BTreeMap;

/// One colour, as an ANSI SGR code.
///
/// Written out rather than pulled from `colored`, because this module is in the
/// library and `colored` is behind the `cli` feature. A summary that only exists
/// in one build configuration is not much of a summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colour {
    Blue,
    Cyan,
    Green,
    Yellow,
    Red,
    Magenta,
    White,
    Grey,
}

impl Colour {
    fn code(self) -> &'static str {
        match self {
            Colour::Blue => "[34m",
            Colour::Cyan => "[36m",
            Colour::Green => "[32m",
            Colour::Yellow => "[33m",
            Colour::Red => "[31m",
            Colour::Magenta => "[35m",
            Colour::White => "[97m",
            Colour::Grey => "[90m",
        }
    }

    /// Wrap text, or return it untouched when colour is off.
    pub fn paint(self, text: &str, colour: bool) -> String {
        if colour {
            format!("{}{text}[0m", self.code())
        } else {
            text.to_string()
        }
    }
}

/// A line of ASCII art and the colour it is drawn in.
pub struct ArtLine(pub &'static str, pub Colour);

/// ASCII art for the running platform, in the style neofetch established.
///
/// Drawn for this crate rather than copied: neofetch is MIT and its art could be
/// vendored with attribution, but writing the shapes here keeps the licensing of
/// this AGPL crate simple and lets each line carry its own colour, which is what
/// makes the Windows flag read as four panes rather than a block of hashes.
///
/// The shapes are the conventional ones — a four-pane flag, a penguin, an apple —
/// because a summary people paste into issues should look like the tool they
/// expect. They are not vendor logos and ironmon claims no trademark in them.
pub fn logo() -> Vec<ArtLine> {
    #[cfg(windows)]
    {
        // The four-pane flag. Each pane its own colour is the detail that makes
        // it legible; a single-colour version reads as noise.
        vec![
            ArtLine("        ,.=:!!t3Z3z.,       ", Colour::Red),
            ArtLine("       :tt:::tt333EE3       ", Colour::Red),
            ArtLine("       Et:::ztt33EEEL  @Ee.,", Colour::Green),
            ArtLine("      ;tt:::tt333EE7 ;EEEEE.", Colour::Green),
            ArtLine("     :Et:::zt333EEQ. $EEEEE.", Colour::Blue),
            ArtLine("     it::::tt333EEF @EEEEEE.", Colour::Blue),
            ArtLine("    ;3=*^```\"*4EEV :EEEEEEE.", Colour::Yellow),
            ArtLine("    ,.=::::!t=., ` @EEEEEEE.", Colour::Yellow),
        ]
    }
    #[cfg(target_os = "linux")]
    {
        // Tux.
        vec![
            ArtLine("         _nnnn_         ", Colour::White),
            ArtLine("        dGGGGMMb        ", Colour::White),
            ArtLine("       @p~qp~~qMb       ", Colour::White),
            ArtLine("       M|@||@) M|       ", Colour::Yellow),
            ArtLine("       @,----.JM|       ", Colour::Yellow),
            ArtLine("      JS^\\__/  qKL      ", Colour::White),
            ArtLine("     dZP        qKRb     ", Colour::White),
            ArtLine("    dZP          qKKb    ", Colour::White),
            ArtLine("   fZP            SMMb   ", Colour::White),
            ArtLine("   HZM            MMMM   ", Colour::White),
            ArtLine("   FqM            MMMM   ", Colour::Yellow),
            ArtLine(" __| \".        |\\dS\"qML ", Colour::Yellow),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        // The apple, in the classic six-band colouring.
        vec![
            ArtLine("                 c.'      ", Colour::Green),
            ArtLine("              ,xNMM.      ", Colour::Green),
            ArtLine("            .OMMMMo       ", Colour::Yellow),
            ArtLine("            lMM\"          ", Colour::Yellow),
            ArtLine("  .;loddo:.  .olloddol;.  ", Colour::Red),
            ArtLine("cKMMMMMMMMMMNWMMMMMMMMMM0:", Colour::Red),
            ArtLine(".KMMMMMMMMMMMMMMMMMMMMMMMWd", Colour::Magenta),
            ArtLine(" XMMMMMMMMMMMMMMMMMMMMMMMX.", Colour::Magenta),
            ArtLine(" .XMMMMMMMMMMMMMMMMMMMMMMk ", Colour::Blue),
            ArtLine("  .XMMMMMMMMMMMMMMMMMMMMX. ", Colour::Blue),
            ArtLine("    kMMMMMMMMMMMMMMMMMMd   ", Colour::Cyan),
            ArtLine("     ;KMMMMMMMWXXWMMMMMk.  ", Colour::Cyan),
        ]
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        vec![
            ArtLine("     +-------+    ", Colour::Cyan),
            ArtLine("     | ..... |    ", Colour::Cyan),
            ArtLine("     | ..... |    ", Colour::Cyan),
            ArtLine("     +-------+    ", Colour::Cyan),
        ]
    }
}

/// The visible width of an art line, so padding is right when colour is off.
fn art_width(art: &[ArtLine]) -> usize {
    art.iter().map(|l| l.0.chars().count()).max().unwrap_or(0)
}

/// One line of the summary: a label, and either a value or why there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub label: String,
    /// The rendered value, or `None` when it was not readable.
    pub value: Option<String>,
    /// Why there is no value. Present exactly when `value` is `None`.
    pub reason: Option<String>,
}

impl Line {
    /// How this line renders, unknown included.
    pub fn render(&self) -> String {
        match (&self.value, &self.reason) {
            (Some(v), _) => v.clone(),
            (None, Some(r)) => format!("unknown — {r}"),
            // Unreachable through `summary`, which always supplies one or the
            // other. Rendered rather than panicking: a display surface that
            // aborts is worse than one that says it does not know.
            (None, None) => "unknown".to_string(),
        }
    }
}

fn find<'a>(readings: &'a [Reading], id: &str) -> Option<&'a Reading> {
    readings.iter().find(|r| r.id == id)
}

/// Render one reading as a line, carrying its reason when it has no value.
/// Every reading measured in degrees Celsius, whatever its id.
///
/// `ironmon cli temperature` used to render `IronMonitor::snapshot().temperature`,
/// whose Windows reader looks only for CPU and motherboard sensors through
/// OpenHardwareMonitor, LibreHardwareMonitor and ACPI thermal zones — and
/// deliberately skips GPUs, with the comment "GPU temps come from NVML". That
/// is true of the crate and false of the command. With none of those three
/// present the map was empty and the command printed "No temperature sensors
/// detected", on a desktop where `ironmon snapshot` read two GPUs and three NVMe
/// drives seconds earlier, in the same binary.
///
/// Selecting on the declared unit rather than on the id shape means a
/// temperature entity added anywhere in the ontology is shown without this
/// function being touched. The unit is carried on unavailable readings too, so
/// a sensor that cannot be read still appears, with its reason.
pub fn temperatures(readings: &[Reading]) -> Vec<&Reading> {
    let mut out: Vec<&Reading> = readings
        .iter()
        .filter(|r| r.unit == Some(crate::ontology::Unit::Celsius))
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Every reading describing power draw, limits, batteries or profiles.
///
/// Unit alone does not identify these — a battery percentage, a profile name
/// and a milliwatt draw are all power facts with different units — so this
/// matches on the id. The defect it fixes is the same one as [`temperatures`]:
/// `PowerStats.rails` is hwmon, which Windows does not expose, so
/// `ironmon cli power` said "No power rails exposed by this platform" while the
/// ontology held both GPUs' draw and limit, the battery percentage and the
/// active power profile.
pub fn power(readings: &[Reading]) -> Vec<&Reading> {
    let mut out: Vec<&Reading> = readings
        .iter()
        .filter(|r| r.id.starts_with("power.") || r.id.contains(".power."))
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn line(
    readings: &[Reading],
    label: &str,
    id: &str,
    fmt: impl Fn(&serde_json::Value) -> String,
) -> Line {
    match find(readings, id) {
        Some(r) => match &r.value {
            Some(v) => Line {
                label: label.to_string(),
                value: Some(fmt(v)),
                reason: None,
            },
            None => Line {
                label: label.to_string(),
                value: None,
                reason: Some(
                    r.note
                        .clone()
                        .unwrap_or_else(|| "no reason recorded".to_string()),
                ),
            },
        },
        None => Line {
            label: label.to_string(),
            value: None,
            reason: Some(format!("{id} is not in this snapshot")),
        },
    }
}

fn as_text(v: &serde_json::Value) -> String {
    v.as_str()
        .map(str::to_string)
        .unwrap_or_else(|| v.to_string())
}

/// Binary prefixes, because the divisor is 1024.
///
/// This divided by 1024 and labelled the result GB, so 100,547,727,360 bytes of
/// installed memory printed as "93.6 GB". 93.6 is the right number and GB is
/// the wrong name for it: that quantity is 93.6 GiB, or 100.5 GB. A crate that
/// carries QUDT units through its ontology should not lose them in the one line
/// most people read.
fn as_bytes(v: &serde_json::Value) -> String {
    let b = v.as_f64().unwrap_or(0.0);
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut n = b;
    let mut i = 0;
    while n >= 1024.0 && i + 1 < UNITS.len() {
        n /= 1024.0;
        i += 1;
    }
    format!("{n:.1} {}", UNITS[i])
}

fn as_percent(v: &serde_json::Value) -> String {
    format!("{:.1}%", v.as_f64().unwrap_or(0.0))
}

/// The summary lines, in display order.
///
/// Takes readings rather than gathering them, so the same function renders a
/// live machine and a fixture.
pub fn summary(readings: &[Reading]) -> Vec<Line> {
    let mut out = vec![
        line(readings, "OS", "system.os.name", as_text),
        line(readings, "Host", "system.hostname", as_text),
        line(readings, "Kernel", "system.kernel.version", as_text),
        line(readings, "Arch", "system.architecture", as_text),
        line(readings, "Uptime", "system.uptime", |v| {
            let secs = v.as_f64().unwrap_or(0.0) as u64;
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        }),
        line(readings, "CPU", "cpu.model", as_text),
        // The logical count under a "Cores" label put "Cores 24" directly
        // beneath "AMD Ryzen 9 9900X 12-Core Processor", contradicting the line
        // above it. Both counts are measured entities; name each one.
        line(readings, "Cores", "cpu.cores.physical", as_text),
        line(readings, "Threads", "cpu.cores.logical", as_text),
        line(readings, "CPU usage", "cpu.total.utilization", as_percent),
        line(readings, "Memory", "memory.total", as_bytes),
        line(readings, "Mem used", "memory.utilization", as_percent),
    ];

    // GPUs are instanced, so they are gathered rather than named. A machine with
    // none gets the `gpu.<none>` diagnostic and its reason, which is the whole
    // point: "no GPU here" and "IronMonitor cannot read GPUs here" look different.
    let gpus: BTreeMap<&str, &Reading> = readings
        .iter()
        .filter(|r| r.id.starts_with("gpu.") && r.id.ends_with(".name"))
        .map(|r| (r.id.as_str(), r))
        .collect();

    if gpus.is_empty() {
        let reason = find(readings, "gpu.<none>")
            .and_then(|r| r.note.clone())
            .unwrap_or_else(|| "no gpu readings in this snapshot".to_string());
        out.push(Line {
            label: "GPU".into(),
            value: None,
            reason: Some(reason),
        });
    } else {
        for (i, (_, r)) in gpus.iter().enumerate() {
            out.push(Line {
                label: if i == 0 {
                    "GPU".into()
                } else {
                    format!("GPU {i}")
                },
                value: r.value.as_ref().map(as_text),
                reason: r.note.clone(),
            });
        }
    }

    out
}

/// The summary without the artwork.
///
/// The default, because most of the time the machine is the point and the logo
/// is decoration. `--ascii` asks for the decoration.
pub fn render_plain(readings: &[Reading], colour: bool) -> String {
    let lines = summary(readings);
    let width = lines
        .iter()
        .map(|l| l.label.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for l in &lines {
        let label = Colour::Cyan.paint(&l.label, colour);
        let pad = width.saturating_sub(l.label.chars().count());
        let value = match &l.value {
            Some(v) => Colour::White.paint(v, colour),
            None => Colour::Grey.paint(&l.render(), colour),
        };
        out.push_str(&format!(
            "{label}{:pad$}  {value}
",
            "",
            pad = pad
        ));
    }
    out
}

/// The whole thing: logo beside the summary.
///
/// `colour` off produces the same layout in plain text, because this output gets
/// piped and pasted at least as often as it gets looked at.
pub fn render(readings: &[Reading], colour: bool) -> String {
    let art = logo();
    let lines = summary(readings);
    let label_width = lines
        .iter()
        .map(|l| l.label.chars().count())
        .max()
        .unwrap_or(0);
    let art_width = art_width(&art);
    let mut out = String::new();

    // Neither side truncates the other: art longer than the summary keeps
    // drawing, and a summary longer than the art keeps listing. A renderer that
    // silently drops rows lies by omission, and the GPU list is exactly the part
    // that varies in length.
    let rows = art.len().max(lines.len());
    for i in 0..rows {
        let (art_text, art_colour) = match art.get(i) {
            Some(ArtLine(t, c)) => (*t, Some(*c)),
            None => ("", None),
        };
        let pad = art_width.saturating_sub(art_text.chars().count());
        let left = match art_colour {
            Some(c) => c.paint(art_text, colour),
            None => art_text.to_string(),
        };

        match lines.get(i) {
            Some(l) => {
                let label = Colour::Cyan.paint(&l.label, colour);
                // Padding is computed from the uncoloured label: escape codes
                // have no width, and counting them right-shifts every value on
                // the line.
                let label_pad = label_width.saturating_sub(l.label.chars().count());
                let value = match (&l.value, &l.reason) {
                    (Some(v), _) => Colour::White.paint(v, colour),
                    // Unknowns in grey. They are information, not errors, and
                    // colouring them red would make an ordinary desktop with no
                    // battery look broken.
                    (None, _) => Colour::Grey.paint(&l.render(), colour),
                };
                out.push_str(&format!(
                    "{left}{:pad$}  {label}{:label_pad$}  {value}
",
                    "",
                    "",
                    pad = pad,
                    label_pad = label_pad
                ));
            }
            None => out.push_str(&format!(
                "{left}
"
            )),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::{Provenance, Unit};

    fn measured(id: &str, v: serde_json::Value) -> Reading {
        Reading {
            id: id.into(),
            value: Some(v),
            provenance: Provenance::Measured,
            unit: Some(Unit::Text),
            note: None,
        }
    }

    fn unavailable(id: &str, why: &str) -> Reading {
        Reading {
            id: id.into(),
            value: None,
            provenance: Provenance::Unavailable,
            unit: None,
            note: Some(why.into()),
        }
    }

    /// The property that separates this from every other tool of its shape.
    #[test]
    fn an_unreadable_field_prints_the_reason_rather_than_a_dash() {
        let readings = [unavailable(
            "cpu.model",
            "the platform CPU reader failed: permission denied",
        )];
        let lines = summary(&readings);
        let cpu = lines.iter().find(|l| l.label == "CPU").unwrap();
        assert!(cpu.value.is_none());
        let rendered = cpu.render();
        assert!(
            rendered.contains("permission denied"),
            "a summary that prints a dash makes a reader guess whether the \
             machine lacks the thing or ironmon lacks the reader: {rendered}"
        );
    }

    /// The distinction a dash would destroy.
    #[test]
    fn no_gpu_and_no_gpu_reader_render_differently() {
        let absent = [unavailable("gpu.<none>", "no GPU detected on this machine")];
        let unreadable = [unavailable("gpu.<none>", "GPU enumeration failed")];

        let a = summary(&absent);
        let b = summary(&unreadable);
        let ga = a.iter().find(|l| l.label == "GPU").unwrap().render();
        let gb = b.iter().find(|l| l.label == "GPU").unwrap().render();

        assert_ne!(
            ga, gb,
            "buying a graphics card and filing a bug are different responses, and \
             the summary is where most people will look first"
        );
        assert!(ga.contains("no GPU detected"));
        assert!(gb.contains("enumeration failed"));
    }

    #[test]
    fn a_present_gpu_is_named() {
        let readings = [measured("gpu.0.name", serde_json::json!("RTX 4090"))];
        let lines = summary(&readings);
        let gpu = lines.iter().find(|l| l.label == "GPU").unwrap();
        assert_eq!(gpu.value.as_deref(), Some("RTX 4090"));
    }

    #[test]
    fn several_gpus_are_all_listed() {
        let readings = [
            measured("gpu.0.name", serde_json::json!("A")),
            measured("gpu.1.name", serde_json::json!("B")),
        ];
        let labels: Vec<String> = summary(&readings)
            .into_iter()
            .filter(|l| l.label.starts_with("GPU"))
            .map(|l| l.label)
            .collect();
        assert_eq!(labels, vec!["GPU", "GPU 1"]);
    }

    /// An id the snapshot never produced is distinguishable from one it produced
    /// as unavailable.
    #[test]
    fn a_missing_id_says_so_rather_than_claiming_it_was_unreadable() {
        let lines = summary(&[]);
        let cpu = lines.iter().find(|l| l.label == "CPU").unwrap();
        assert!(cpu.render().contains("not in this snapshot"));
    }

    /// Colour off must leave no escape codes at all: this output is piped and
    /// pasted at least as often as it is looked at.
    #[test]
    fn colour_off_produces_plain_text() {
        let readings = [measured("cpu.model", serde_json::json!("x"))];
        for rendered in [render(&readings, false), render_plain(&readings, false)] {
            assert!(
                !rendered.contains('\u{1b}'),
                "escape codes survived with colour off: {}",
                rendered.escape_debug()
            );
        }
    }

    #[test]
    fn colour_on_closes_every_sequence_it_opens() {
        let readings = [measured("cpu.model", serde_json::json!("x"))];
        let rendered = render_plain(&readings, true);
        assert!(rendered.contains('\u{1b}'));
        let opens = rendered.matches("\u{1b}[").count();
        let resets = rendered.matches("\u{1b}[0m").count();
        assert_eq!(
            opens - resets,
            resets,
            "every opening sequence needs its reset, or colour bleeds into the \
             terminal after the command exits"
        );
    }

    /// Escape codes have no width, so padding computed over a coloured string
    /// right-shifts every value on the line.
    #[test]
    fn columns_line_up_whether_or_not_colour_is_on() {
        let readings = [
            measured("cpu.model", serde_json::json!("a")),
            measured("system.hostname", serde_json::json!("b")),
        ];
        let plain = render_plain(&readings, false);
        let coloured = render_plain(&readings, true);

        let stripped: String = {
            let mut out = String::new();
            let mut chars = coloured.chars();
            while let Some(c) = chars.next() {
                if c == '\u{1b}' {
                    for c2 in chars.by_ref() {
                        if c2 == 'm' {
                            break;
                        }
                    }
                } else {
                    out.push(c);
                }
            }
            out
        };
        assert_eq!(
            stripped, plain,
            "colour changed the layout, which means padding was computed over \
             escape codes"
        );
    }

    /// The art is drawn per line so each can carry its own colour.
    #[test]
    fn every_art_line_declares_a_colour_and_has_content() {
        let art = logo();
        assert!(!art.is_empty(), "this platform has no art at all");
        for ArtLine(text, _) in &art {
            assert!(!text.is_empty(), "an empty art line renders as a blank row");
        }
    }

    #[test]
    fn bytes_render_at_a_readable_scale() {
        assert_eq!(as_bytes(&serde_json::json!(1024)), "1.0 KiB");
        assert_eq!(as_bytes(&serde_json::json!(1_073_741_824u64)), "1.0 GiB");
        // 100,547,727,360 bytes is this desktop's installed memory. It is
        // 93.6 GiB and 100.5 GB; the label has to say which.
        assert_eq!(as_bytes(&serde_json::json!(100_547_727_360u64)), "93.6 GiB");
    }

    #[test]
    fn the_logo_and_the_summary_both_survive_being_longer() {
        // More lines than art, and more art than lines: neither may truncate the
        // other, because a summary that silently drops a row is a summary that
        // lies by omission.
        let many: Vec<Reading> = (0..20)
            .map(|i| measured(&format!("gpu.{i}.name"), serde_json::json!("G")))
            .collect();
        let rendered = render(&many, false);
        assert!(rendered.lines().count() >= 20);

        let none = render(&[], false);
        assert!(none.lines().count() >= logo().len());
    }

    /// The command named "temperature" must not show one subsystem's sensors.
    ///
    /// `ironmon cli temperature` printed "No temperature sensors detected" on a
    /// desktop with five readable sensors, because it rendered a struct whose
    /// Windows reader covers CPU and motherboard only and skips GPUs on
    /// purpose. Every temperature entity is declared in the ontology whether or
    /// not this machine can read it, so this assertion holds on any host,
    /// including a CI runner with no sensors at all.
    ///
    /// It fails if temperatures are ever sourced from one subsystem again.
    /// One `Reading` per declared entity, with the declared unit and no value.
    ///
    /// The selection functions are predicates over readings, so they can be
    /// exercised against the ontology's *declarations* — which exist on every
    /// host — instead of against a resolved snapshot, which does not: a CI
    /// runner with no GPU produces no `gpu.0.thermal.temperature` row at all.
    /// Two tests here asserted against `resolve::snapshot()` and passed on this
    /// desktop while failing on all three runners, which is the whole reason
    /// the distinction is written down.
    fn declared_readings() -> Vec<Reading> {
        crate::ontology::Ontology::build()
            .entities
            .values()
            .map(|e| Reading {
                id: e.id.clone(),
                value: None,
                provenance: crate::ontology::Provenance::Unavailable,
                unit: e.unit,
                note: None,
            })
            .collect()
    }

    #[test]
    fn temperature_selection_spans_every_subsystem_that_declares_one() {
        let declared = declared_readings();
        let temps = temperatures(&declared);

        assert!(
            !temps.is_empty(),
            "the ontology declares temperature entities; none were selected"
        );

        for r in &temps {
            assert_eq!(
                r.unit,
                Some(crate::ontology::Unit::Celsius),
                "{} was selected as a temperature and is not measured in celsius",
                r.id
            );
        }

        let prefixes: std::collections::BTreeSet<&str> = temps
            .iter()
            .filter_map(|r| r.id.split('.').next())
            .collect();
        assert!(
            prefixes.len() > 1,
            concat!(
                "every temperature came from one subsystem ({:?}). That is what ",
                "the defect looked like: the CPU/motherboard reader alone, ",
                "reporting no sensors on a machine with GPU and disk sensors ",
                "it never consulted."
            ),
            prefixes
        );
    }

    /// Power is not only hwmon rails, which is the whole of what Windows lacks.
    #[test]
    fn power_selection_reaches_past_hwmon_rails() {
        let declared = declared_readings();
        let power = power(&declared);

        assert!(
            !power.is_empty(),
            "the ontology declares power entities; none were selected"
        );
        assert!(
            power.iter().any(|r| r.id.contains(".power.")),
            concat!(
                "no per-device power reading was selected; `ironmon cli power` ",
                "would again report nothing on a platform without hwmon rails"
            )
        );
    }
}
