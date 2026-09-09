// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! JSON-LD serialisation of a hardware snapshot.
//!
//! # Why linked data rather than another JSON shape
//!
//! `ironmon snapshot --format json` already emits every reading. What it does not
//! emit is what any of the words mean. An agent receiving `{"id":
//! "cpu.total.utilization", "unit": "percent"}` has to be told out of band that
//! `percent` is a unit, that `cpu.total.utilization` names a measurement rather
//! than a setting, and that `unavailable` is not a value.
//!
//! JSON-LD carries that in the document. Terms resolve to IRIs, units resolve to
//! [QUDT], and a consumer that has never seen IronMonitor can follow the `@context` to
//! find out what it is looking at.
//!
//! [QUDT]: https://qudt.org/
//!
//! # Units are mapped where a mapping exists, and not invented where it does not
//!
//! `celsius`, `percent`, `bytes`, `hertz`, `watts`, `volts` and the rest have
//! exact QUDT terms and are mapped to them. `identifier` and `text` are not
//! quantities and have no QUDT equivalent, so they carry ironmon's own term rather
//! than being forced into a unit ontology where they do not belong.
//!
//! A wrong `@type` is worse than an absent one: it is a machine-readable claim
//! that a string is a physical quantity, and something downstream will do
//! arithmetic on it.
//!
//! # Absence survives the encoding
//!
//! This is the property the whole crate is built on and it is easy to lose in a
//! serialisation. An unavailable reading is emitted as a node with **no value**
//! and a `ironmon:unavailableReason`, not as a value of zero, null, or an omitted
//! node. An agent walking the graph can tell "not read, because X" from "read as
//! X" without knowing anything about ironmon.

use super::resolve::Reading;
use super::{Provenance, Unit};
use serde_json::{json, Map, Value};

/// The IRI prefix for ironmon's own terms.
pub const IRONMON_NS: &str = "https://nervosys.github.io/IronMonitor/ns#";
/// QUDT's unit vocabulary, used for the units that have an exact equivalent.
pub const QUDT_UNIT_NS: &str = "http://qudt.org/vocab/unit/";

/// The QUDT unit IRI for a ironmon unit, where one exists.
///
/// `None` for the two that are not quantities. Forcing `identifier` into a unit
/// ontology would tell a consumer that a GUID is a measurable amount of
/// something.
pub fn qudt_unit(unit: Unit) -> Option<&'static str> {
    Some(match unit {
        Unit::Celsius => "DEG_C",
        Unit::Percent => "PERCENT",
        Unit::Bytes => "BYTE",
        Unit::BytesPerSecond => "BYTE-PER-SEC",
        Unit::Hertz => "HZ",
        Unit::Megahertz => "MegaHZ",
        // QUDT has no megatransfer unit: a transfer is a bus operation, not an
        // SI quantity, and MT/s is only a frequency if you assume one bit per
        // transfer -- which is exactly the assumption DDR breaks. `NUM-PER-SEC`
        // says "this many per second" without claiming to know of what.
        Unit::MegatransfersPerSecond => "NUM-PER-SEC",
        Unit::Watts => "W",
        Unit::Milliwatts => "MilliW",
        Unit::Volts => "V",
        Unit::Rpm => "REV-PER-MIN",
        Unit::Seconds => "SEC",
        Unit::Hours => "HR",
        Unit::Milliseconds => "MilliSEC",
        Unit::Count => "NUM",
        // Not quantities. See the module documentation.
        Unit::Identifier | Unit::Text => return None,
    })
}

/// The `@context` a consumer follows to interpret the graph.
pub fn context() -> Value {
    json!({
        "ironmon": IRONMON_NS,
        "qudt": "http://qudt.org/schema/qudt/",
        // The QUDT unit vocabulary, as a prefix. This and the property below
        // were both called `unit` in the first version, so the object had a
        // duplicate key: the property definition won, the prefix was never
        // defined, and every `unit:PERCENT` in the graph was an unresolvable
        // name. The document looked correct and said nothing.
        "unit": QUDT_UNIT_NS,
        "xsd": "http://www.w3.org/2001/XMLSchema#",
        "id": "@id",
        "type": "@type",
        // `value` is deliberately not coerced to a single xsd type: readings are
        // numbers, strings and booleans depending on the entity, and declaring
        // one would misdescribe the others.
        "value": "ironmon:value",
        // `@type: @id` so the value is read as an IRI reference rather than a
        // string that happens to contain a colon.
        "hasUnit": {
            "@id": "qudt:hasUnit",
            "@type": "@id"
        },
        "provenance": "ironmon:provenance",
        "unavailableReason": "ironmon:unavailableReason",
        "entityKind": "ironmon:entityKind",
        "domain": "ironmon:domain",
        "observedAt": {
            "@id": "ironmon:observedAt",
            "@type": "xsd:dateTime"
        }
    })
}

/// One reading as a JSON-LD node.
fn node(reading: &Reading, ontology: &super::Ontology) -> Value {
    let mut map = Map::new();
    map.insert("id".into(), json!(format!("{IRONMON_NS}{}", reading.id)));

    // The type says what kind of thing this is, from the schema rather than
    // from the value's shape. An entity IronMonitor has no declaration for gets
    // `ironmon:Reading` and nothing more specific, which is honest.
    let entity = ontology.template_for(&reading.id);
    let kind = entity
        .map(|e| match e.kind {
            super::EntityKind::Measurement => "ironmon:Measurement",
            super::EntityKind::Identity => "ironmon:Identity",
            super::EntityKind::Setting => "ironmon:Setting",
            super::EntityKind::Limit => "ironmon:Limit",
            super::EntityKind::Diagnostic => "ironmon:Diagnostic",
        })
        .unwrap_or("ironmon:Reading");
    map.insert("type".into(), json!(kind));

    if let Some(e) = entity {
        map.insert("domain".into(), json!(e.domain.as_str()));
        map.insert("entityKind".into(), json!(e.kind.as_str()));
    }

    map.insert("provenance".into(), json!(reading.provenance.as_str()));

    match (&reading.value, reading.provenance) {
        // The property the crate exists to preserve, carried into the encoding:
        // no value, and a reason a consumer can read.
        (_, Provenance::Unavailable) | (None, _) => {
            map.insert(
                "unavailableReason".into(),
                json!(reading
                    .note
                    .clone()
                    .unwrap_or_else(|| "no reason recorded".to_string())),
            );
        }
        (Some(v), _) => {
            map.insert("value".into(), v.clone());
            if let Some(u) = reading.unit.and_then(qudt_unit) {
                map.insert("hasUnit".into(), json!(format!("unit:{u}")));
            } else if let Some(u) = reading.unit {
                // ironmon's own term for the things QUDT does not describe.
                map.insert("hasUnit".into(), json!(format!("ironmon:{}", u.as_str())));
            }
        }
    }

    Value::Object(map)
}

/// A whole snapshot as one JSON-LD document.
///
/// `observed_at` is passed in rather than read from the clock here, so the
/// document is a function of its inputs and a test can produce a fixed one.
pub fn document(readings: &[Reading], observed_at: &str) -> Value {
    let ontology = super::Ontology::build();
    let graph: Vec<Value> = readings.iter().map(|r| node(r, &ontology)).collect();

    json!({
        "@context": context(),
        "id": format!("{IRONMON_NS}snapshot"),
        "type": "ironmon:Snapshot",
        "observedAt": observed_at,
        "ironmon:readingCount": graph.len(),
        "@graph": graph,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::Unit;

    // Built literally rather than through `Reading`'s constructors, which are
    // private to `resolve`. Widening their visibility for a test would be the
    // test changing the code to suit itself.
    fn measured(id: &str, value: Value, unit: Option<Unit>) -> Reading {
        Reading {
            id: id.to_string(),
            value: Some(value),
            provenance: Provenance::Measured,
            unit,
            note: None,
        }
    }

    fn unavailable(id: &str, reason: &str) -> Reading {
        Reading {
            id: id.to_string(),
            value: None,
            provenance: Provenance::Unavailable,
            unit: Some(Unit::Percent),
            note: Some(reason.to_string()),
        }
    }

    #[test]
    fn the_context_defines_every_term_the_nodes_use() {
        let ctx = context();
        for term in [
            "value",
            "hasUnit",
            "provenance",
            "unavailableReason",
            "entityKind",
            "domain",
            "observedAt",
        ] {
            assert!(
                ctx.get(term).is_some(),
                "nodes emit {term:?} and the context does not define it, so a \
                 consumer following the document cannot resolve it"
            );
        }
    }

    /// Every prefixed name in the graph resolves through the context.
    ///
    /// The first version defined `unit` twice — once as the QUDT prefix and once
    /// as the property — so the prefix was silently lost and every
    /// `unit:PERCENT` became an unresolvable name. `the_context_defines_every_
    /// term_the_nodes_use` passed throughout, because it checked that terms were
    /// present and not that the names they produce can be followed.
    #[test]
    fn every_prefixed_name_the_graph_emits_has_a_defined_prefix() {
        let doc = document(
            &[
                measured("cpu.total.utilization", json!(1), Some(Unit::Percent)),
                measured("cpu.model", json!("x"), Some(Unit::Text)),
            ],
            "2026-01-01T00:00:00Z",
        );
        let ctx = doc["@context"].as_object().expect("context is an object");

        let mut checked = 0;
        for n in doc["@graph"].as_array().unwrap() {
            for key in ["hasUnit", "type"] {
                let Some(v) = n.get(key).and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some((prefix, _)) = v.split_once(':') else {
                    continue;
                };
                if v.starts_with("http") {
                    continue;
                }
                let defined = ctx.get(prefix).is_some();
                assert!(
                    defined,
                    "the graph emits {v:?} and the context defines no {prefix:?}                      prefix, so a consumer cannot resolve it"
                );
                checked += 1;
            }
        }
        assert!(
            checked > 0,
            "no prefixed names were examined, so this test proved nothing"
        );
    }

    /// The property that must survive the encoding.
    #[test]
    fn an_unavailable_reading_has_no_value_and_carries_its_reason() {
        let doc = document(
            &[unavailable("cpu.total.utilization", "the reader failed")],
            "2026-01-01T00:00:00Z",
        );
        let n = &doc["@graph"][0];
        assert!(
            n.get("value").is_none(),
            "an unavailable reading must not carry a value, not even null: a \
             consumer that finds one will use it"
        );
        assert_eq!(n["unavailableReason"], json!("the reader failed"));
        assert_eq!(n["provenance"], json!("unavailable"));
    }

    #[test]
    fn a_measured_reading_carries_its_value_and_a_qudt_unit() {
        let doc = document(
            &[measured(
                "cpu.total.utilization",
                json!(42.5),
                Some(Unit::Percent),
            )],
            "2026-01-01T00:00:00Z",
        );
        let n = &doc["@graph"][0];
        assert_eq!(n["value"], json!(42.5));
        assert_eq!(n["hasUnit"], json!("unit:PERCENT"));
        assert!(n["id"].as_str().unwrap().starts_with(IRONMON_NS));
    }

    /// A wrong `@type` is worse than an absent one.
    #[test]
    fn non_quantities_are_not_given_a_qudt_unit() {
        assert_eq!(qudt_unit(Unit::Identifier), None);
        assert_eq!(qudt_unit(Unit::Text), None);

        let doc = document(
            &[measured("cpu.model", json!("Ryzen"), Some(Unit::Text))],
            "2026-01-01T00:00:00Z",
        );
        let unit = doc["@graph"][0]["hasUnit"].as_str().unwrap();
        assert!(
            unit.starts_with("ironmon:"),
            "a text field must not be typed as a physical quantity — something \
             downstream will do arithmetic on it. got {unit}"
        );
    }

    /// Every quantity unit maps, so the check is that none was forgotten.
    #[test]
    fn every_quantity_unit_has_a_qudt_term() {
        for unit in [
            Unit::Celsius,
            Unit::Percent,
            Unit::Bytes,
            Unit::BytesPerSecond,
            Unit::Hertz,
            Unit::Megahertz,
            Unit::Watts,
            Unit::Milliwatts,
            Unit::Volts,
            Unit::Rpm,
            Unit::Seconds,
            Unit::Hours,
            Unit::Milliseconds,
            Unit::Count,
        ] {
            assert!(
                qudt_unit(unit).is_some(),
                "{:?} is a quantity and has no QUDT term; a consumer cannot \
                 convert or compare it",
                unit
            );
        }
    }

    #[test]
    fn the_document_is_a_function_of_its_inputs() {
        let readings = [measured(
            "cpu.total.utilization",
            json!(1),
            Some(Unit::Percent),
        )];
        let a = document(&readings, "2026-01-01T00:00:00Z");
        let b = document(&readings, "2026-01-01T00:00:00Z");
        assert_eq!(
            a, b,
            "the same readings and stamp must give the same document"
        );
    }

    #[test]
    fn a_declared_entity_carries_its_domain_and_kind() {
        let doc = document(
            &[measured(
                "cpu.total.utilization",
                json!(10),
                Some(Unit::Percent),
            )],
            "2026-01-01T00:00:00Z",
        );
        let n = &doc["@graph"][0];
        assert_eq!(n["domain"], json!("cpu"));
        assert!(n["type"].as_str().unwrap().starts_with("ironmon:"));
    }
}
