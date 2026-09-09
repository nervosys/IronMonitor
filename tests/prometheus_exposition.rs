//! The exporter's output has to parse as Prometheus text exposition.
//!
//! This surface had no test file. A defect here is not a wrong number on a
//! dashboard — Prometheus rejects a scrape containing duplicate samples, so one
//! malformed section discards every metric the endpoint serves, including the
//! correct ones.

use std::collections::BTreeMap;

use ironmonlib::prometheus::PrometheusExporter;

fn exported() -> String {
    let mut exporter = PrometheusExporter::new("ironmon");
    exporter.collect_system_metrics();
    exporter.export()
}

fn sample_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
}

/// One `# HELP` and one `# TYPE` per metric name, which the format requires.
///
/// `PrometheusExporter::add` pushed a new family per sample, so the headers
/// repeated once per disk, per GPU and per network interface — twenty times for
/// `ironmon_network_rx_bytes_total` on the machine this was written on.
#[test]
fn each_metric_name_is_declared_once() {
    let text = exported();
    let mut helps: BTreeMap<&str, usize> = BTreeMap::new();
    let mut types: BTreeMap<&str, usize> = BTreeMap::new();

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# HELP ") {
            *helps
                .entry(rest.split(' ').next().unwrap_or(""))
                .or_default() += 1;
        } else if let Some(rest) = line.strip_prefix("# TYPE ") {
            *types
                .entry(rest.split(' ').next().unwrap_or(""))
                .or_default() += 1;
        }
    }

    let repeated: Vec<String> = helps
        .iter()
        .chain(types.iter())
        .filter(|(_, count)| **count > 1)
        .map(|(name, count)| format!("{name} declared {count} times"))
        .collect();

    assert!(
        repeated.is_empty(),
        "the text format allows one HELP and one TYPE per metric name: {repeated:#?}"
    );
}

/// No two samples may share a name and a label set in one scrape.
///
/// This is the fatal one. `collect_network_metrics` built an `interface` label
/// and then called the unlabelled `MetricFamily::counter`, so every interface
/// emitted `ironmon_network_rx_bytes_total` with no labels and its own value —
/// twenty identical series, which Prometheus rejects as duplicate samples,
/// taking the whole endpoint down rather than just the network metrics.
#[test]
fn no_two_samples_share_a_name_and_labels() {
    let text = exported();
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();

    for line in sample_lines(&text) {
        // Everything up to the final space is the series identity.
        let Some((identity, _value)) = line.rsplit_once(' ') else {
            continue;
        };
        *seen.entry(identity).or_default() += 1;
    }

    let duplicated: Vec<String> = seen
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(identity, count)| format!("{identity} appears {count} times"))
        .collect();

    assert!(
        duplicated.is_empty(),
        "duplicate samples make Prometheus reject the entire scrape, so these \
         discard every other metric on the endpoint too: {duplicated:#?}"
    );
}

// Why there is no "every per-instance metric carries a label" test here.
//
// There was one, and it was deleted rather than kept green. The rule it tried
// to express -- a metric collected in a loop must be labelled -- is real, but
// a test cannot see the loop, so it guessed from the name and needed an
// exemption every time a legitimately whole-machine metric appeared:
// `ironmon_gpu_count`, then `ironmon_swap_used_bytes`, then
// `ironmon_uptime_seconds`. Three exemptions in one sitting, none of them a
// defect.
//
// **A test that needs a growing allowlist to stay green has stopped testing an
// invariant and started describing the current output.** The failure it was
// built to catch -- an unlabelled metric emitted once per device -- produces
// duplicate samples, and `no_two_samples_share_a_name_and_labels` above
// catches that exactly, with no heuristic and no exemptions.

/// Metric names the bundled Grafana dashboards query.
///
/// `grafana/` ships three dashboards, and they are a contract: a panel querying
/// a name nothing exports renders empty against a live server, which looks like
/// broken hardware rather than a broken dashboard. `http_server.rs` records that
/// this whole class of failure already happened once — the names lacked the
/// `ironmon_` prefix and all three dashboards were blank — and nothing prevented
/// it recurring. Six queried names were unpublished when the first version of
/// this check was written, including `ironmon_gpu_clock_graphics_mhz`, which the
/// exporter published as `ironmon_gpu_clock_core_mhz`: the same number under a
/// name no dashboard asks for.
fn dashboard_metrics(root: &std::path::Path) -> std::collections::BTreeSet<String> {
    let mut queried = std::collections::BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(root.join("grafana")) else {
        return queried;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (idx, _) in text.match_indices("ironmon_") {
            queried.insert(
                text[idx..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect(),
            );
        }
    }
    queried
}

/// A publisher's source with everything that is not publishing code removed.
///
/// **Two things have to come out, and each was learned from a wrong version of
/// this test.**
///
/// Comments, because the first version searched raw source and passed while the
/// defect was restored: the comment explaining the defect contained the name it
/// was looking for. A test a comment can satisfy is not a test.
///
/// And `#[cfg(test)]` modules, because splitting this check found the same hole
/// again in a new place. `http_server.rs` contains
/// `assert!(!text.contains("ironmon_disk_read_bytes_total"))` — a test pinning
/// that the name is *not* published — and a plain source search read that string
/// literal as a publisher. The metric counted as covered on the strength of a
/// test asserting it was missing.
fn publishing_code(path: &std::path::Path) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };

    let mut code = String::with_capacity(text.len());
    for line in text.lines() {
        let without_comment = match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        };
        code.push_str(without_comment);
        code.push('\n');
    }

    // Drop each `#[cfg(test)] mod ... { ... }` by matching its braces.
    let mut out = String::with_capacity(code.len());
    let mut rest = code.as_str();
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let Some(open) = after.find('{') else {
            break;
        };
        let mut depth = 0usize;
        let mut close = None;
        for (i, c) in after[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        match close {
            Some(end) => rest = &after[end..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Which dashboard metrics a publisher does not emit.
fn unpublished_by(root: &std::path::Path, files: &[&str]) -> Vec<String> {
    let code: String = files
        .iter()
        .map(|f| publishing_code(&root.join(f)))
        .collect();

    dashboard_metrics(root)
        .into_iter()
        .filter(|name| {
            // Emitted either as the full name or through `prefixed("...")`,
            // which prepends `ironmon_`.
            let bare = name.strip_prefix("ironmon_").unwrap_or(name);
            !code.contains(&format!("\"{name}\"")) && !code.contains(&format!("\"{bare}\""))
        })
        .collect()
}

/// Assert a publisher's gaps are exactly `known`, no more and no fewer.
///
/// The second half matters as much as the first. An exemption list that is
/// allowed to go stale stops describing anything: a gap that gets closed but
/// stays listed makes the test quietly weaker, and this crate has already once
/// quoted a coverage figure that had been out of date for several commits.
/// Closing a gap must therefore fail here until the entry is removed.
fn assert_gaps_are_exactly(publisher: &str, missing: &[String], known: &[&str]) {
    let missing: std::collections::BTreeSet<&str> = missing.iter().map(String::as_str).collect();
    let known: std::collections::BTreeSet<&str> = known.iter().copied().collect();

    let unexpected: Vec<&&str> = missing.difference(&known).collect();
    assert!(
        unexpected.is_empty(),
        "{publisher} does not publish {unexpected:#?}, which the bundled \
         dashboards query. Those panels render empty against a live server."
    );

    let closed: Vec<&&str> = known.difference(&missing).collect();
    assert!(
        closed.is_empty(),
        "{publisher} now publishes {closed:#?}, which are still listed as known \
         gaps. Remove them from the list — an exemption nobody prunes is how \
         this check gets quietly weaker."
    );
}

/// Every dashboard metric has a publisher *somewhere* in the crate.
#[test]
fn every_dashboard_metric_is_published_somewhere() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(
        !dashboard_metrics(root).is_empty(),
        "no metric names found in grafana/, so this test checks nothing"
    );

    let missing = unpublished_by(root, &["src/prometheus.rs", "src/http_server.rs"]);
    assert!(
        missing.is_empty(),
        "the bundled dashboards query metrics no publisher emits, so those \
         panels render empty against a live server: {missing:#?}"
    );
}

/// The endpoint the server actually serves must publish them.
///
/// This is the check that matters, and until it was split out of the one above
/// it did not exist. There are two publishers, they have *different* gaps, and a
/// test accepting either one covered every gap in both: `prometheus.rs` supplies
/// the four names `http_server.rs` misses and `http_server.rs` supplies the two
/// that `prometheus.rs` misses, so the combined check read 24/24 while neither
/// publisher was complete and only one of them is reachable over HTTP.
///
/// `/api/v1/metrics/prometheus` serves `MetricCollector`, filled by
/// `record_snapshot`. A name only `PrometheusExporter` knows is not on the wire.
#[test]
fn the_served_endpoint_publishes_every_dashboard_metric() {
    // Empty, and it must stay that way. Each of these three was limited by
    // the pipeline `Snapshot` rather than by the recorder — `record_snapshot` is
    // deliberately pure over a `Snapshot`, and that purity is what makes it
    // testable, so it cannot go read a sensor the snapshot does not carry. The
    // fix in each case was to put the reading in the snapshot: `disk_io` in
    // `400608c`, `cpu_temperatures` after it.
    const KNOWN_GAPS: &[&str] = &[];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let missing = unpublished_by(root, &["src/http_server.rs"]);
    assert_gaps_are_exactly("the served endpoint", &missing, KNOWN_GAPS);
}

/// And so must the library exporter, which has its own, different gaps.
#[test]
fn the_library_exporter_publishes_every_dashboard_metric() {
    // Empty, and it must stay that way. These four were never blocked on
    // anything — `PrometheusExporter` collects from the system directly and
    // could always have read them. They were simply metrics nobody taught it,
    // and the combined check never asked because `http_server.rs` publishes all
    // four. Adding an entry here means the exporter lost a metric.
    const KNOWN_GAPS: &[&str] = &[];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let missing = unpublished_by(root, &["src/prometheus.rs"]);
    assert_gaps_are_exactly("the library exporter", &missing, KNOWN_GAPS);
}

/// The other Prometheus renderer — the one the HTTP server actually serves.
///
/// `/api/v1/metrics/prometheus` does not serve `PrometheusExporter`. It serves
/// `MetricCollector::export_prometheus`, a second implementation, and nothing
/// outside `src/prometheus.rs` referenced the first one at all: the complete,
/// correctly formatted exporter was unreachable and the served renderer was the
/// smaller one.
///
/// `record_with_labels` encodes a labelled series into its storage key as
/// `name:{gpu=0}` — a map key, not exposition syntax. Rendered verbatim that is
/// invalid: Prometheus needs `name{gpu="0"}`, quoted and without the colon.
#[test]
fn the_served_renderer_emits_valid_label_syntax() {
    use ironmonlib::observability::MetricCollector;

    let collector = MetricCollector::new();
    collector.record("ironmon_uptime_seconds", 1234.0);
    collector.record_with_labels("ironmon_gpu_temperature_celsius", 42.0, &[("gpu", "0")]);
    collector.record_with_labels(
        "ironmon_gpu_temperature_celsius",
        37.0,
        &[("gpu", "1"), ("vendor", "NVIDIA")],
    );

    let text = collector.export_prometheus();

    assert!(
        !text.contains(":{"),
        "the storage key leaked into the exposition, which no scraper parses: {text}"
    );
    assert!(
        text.contains("ironmon_gpu_temperature_celsius{gpu=\"0\"} 42"),
        "a single label should render as gpu=\"0\": {text}"
    );
    assert!(
        text.contains("ironmon_gpu_temperature_celsius{gpu=\"1\",vendor=\"NVIDIA\"} 37"),
        "several labels should render comma-separated and quoted: {text}"
    );
    assert!(
        text.contains("ironmon_uptime_seconds 1234"),
        "an unlabelled series should render unchanged: {text}"
    );
}
