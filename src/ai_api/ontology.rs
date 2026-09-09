//! Hardware Ontology for AI Agent Discoverability
//!
//! # Superseded by [`crate::ontology`]
//!
//! This module describes six domains and twenty-four properties. It has no
//! provenance, so a consumer cannot tell a measured value from a published
//! constant; no resolver, so an id here cannot be turned into a reading; and no
//! command exposes it, so an agent cannot reach it without linking the library.
//!
//! [`crate::ontology`] covers the same ground with all three, and is what
//! `ironmon describe`, `ironmon get` and `ironmon snapshot` speak. Two things called
//! "ontology" in one crate is the naming drift the new module exists to end, so
//! this one is deprecated rather than left as a second answer to the same
//! question. A test asserts every domain named here is present there, so nothing
//! is lost by moving.

use serde::{Deserialize, Serialize};

/// Hardware domain ontology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareOntology {
    pub version: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub domains: Vec<HardwareDomain>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareDomain {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub properties: Vec<DomainProperty>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainProperty {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub data_type: &'static str,
    pub unit: Option<&'static str>,
}

impl HardwareOntology {
    pub fn complete() -> Self {
        Self {
            version: "1.0.0",
            name: "IronMonitor Hardware Ontology",
            description: "Comprehensive ontology for hardware monitoring enabling AI agents to understand computer hardware state.",
            domains: vec![
                HardwareDomain {
                    id: "gpu",
                    name: "Graphics Processing Unit",
                    description: "GPU hardware for graphics, ML, and parallel compute. Supports NVIDIA, AMD, Intel, Apple.",
                    properties: vec![
                        DomainProperty { id: "name", name: "GPU Name", description: "Model name", data_type: "string", unit: None },
                        DomainProperty { id: "vendor", name: "Vendor", description: "Manufacturer", data_type: "string", unit: None },
                        DomainProperty { id: "utilization", name: "Utilization", description: "Current usage %", data_type: "percentage", unit: Some("%") },
                        DomainProperty { id: "memory_used", name: "Memory Used", description: "VRAM in use", data_type: "bytes", unit: Some("bytes") },
                        DomainProperty { id: "temperature", name: "Temperature", description: "Core temperature", data_type: "temperature", unit: Some("C") },
                        DomainProperty { id: "power_draw", name: "Power Draw", description: "Power consumption", data_type: "power", unit: Some("W") },
                    ],
                },
                HardwareDomain {
                    id: "cpu",
                    name: "Central Processing Unit",
                    description: "CPU for general computation. Per-core utilization, frequency, thermal state.",
                    properties: vec![
                        DomainProperty { id: "model", name: "CPU Model", description: "Processor model name", data_type: "string", unit: None },
                        DomainProperty { id: "cores", name: "Cores", description: "Number of cores", data_type: "integer", unit: None },
                        DomainProperty { id: "utilization", name: "Utilization", description: "Current usage %", data_type: "percentage", unit: Some("%") },
                        DomainProperty { id: "frequency", name: "Frequency", description: "Current clock speed", data_type: "frequency", unit: Some("MHz") },
                    ],
                },
                HardwareDomain {
                    id: "memory",
                    name: "System Memory",
                    description: "RAM and swap space monitoring.",
                    properties: vec![
                        DomainProperty { id: "total", name: "Total RAM", description: "Total memory", data_type: "bytes", unit: Some("bytes") },
                        DomainProperty { id: "used", name: "Used Memory", description: "Memory in use", data_type: "bytes", unit: Some("bytes") },
                        DomainProperty { id: "usage_percent", name: "Usage %", description: "Memory usage percentage", data_type: "percentage", unit: Some("%") },
                    ],
                },
                HardwareDomain {
                    id: "disk",
                    name: "Storage Devices",
                    description: "Disk and storage monitoring (NVMe, SATA, etc.).",
                    properties: vec![
                        DomainProperty { id: "name", name: "Device Name", description: "Disk device name", data_type: "string", unit: None },
                        DomainProperty { id: "size", name: "Size", description: "Total capacity", data_type: "bytes", unit: Some("bytes") },
                        DomainProperty { id: "temperature", name: "Temperature", description: "Disk temperature", data_type: "temperature", unit: Some("C") },
                    ],
                },
                HardwareDomain {
                    id: "network",
                    name: "Network Interfaces",
                    description: "Network interface monitoring.",
                    properties: vec![
                        DomainProperty { id: "name", name: "Interface Name", description: "Interface name", data_type: "string", unit: None },
                        DomainProperty { id: "rx_rate", name: "Receive Rate", description: "Current receive bandwidth", data_type: "bytes", unit: Some("bytes/sec") },
                        DomainProperty { id: "tx_rate", name: "Transmit Rate", description: "Current transmit bandwidth", data_type: "bytes", unit: Some("bytes/sec") },
                    ],
                },
                HardwareDomain {
                    id: "process",
                    name: "System Processes",
                    description: "Running process monitoring.",
                    properties: vec![
                        DomainProperty { id: "pid", name: "Process ID", description: "Unique process identifier", data_type: "integer", unit: None },
                        DomainProperty { id: "name", name: "Process Name", description: "Executable name", data_type: "string", unit: None },
                        DomainProperty { id: "cpu_percent", name: "CPU Usage", description: "CPU utilization %", data_type: "percentage", unit: Some("%") },
                        DomainProperty { id: "memory_bytes", name: "Memory Usage", description: "Physical memory used", data_type: "bytes", unit: Some("bytes") },
                    ],
                },
            ],
        }
    }
}

#[cfg(test)]
mod supersession_tests {
    use super::*;
    use crate::ontology::{Domain, Ontology};

    /// Deprecating this module is only safe if its successor covers it. Every domain
    /// named here must exist in `crate::ontology`, or moving would lose something.
    #[test]
    fn every_legacy_domain_exists_in_the_current_ontology() {
        let legacy = HardwareOntology::complete();
        assert!(
            !legacy.domains.is_empty(),
            "an empty legacy ontology would make this assertion vacuous"
        );

        // Every legacy domain name is also a name in the current ontology, so no
        // translation table is needed — and inventing one would be asserting a
        // correspondence rather than checking it.
        for domain in &legacy.domains {
            assert!(
                Domain::parse(domain.id).is_some(),
                "legacy domain {:?} has no counterpart in crate::ontology, so \
                 deprecating this module would lose it",
                domain.id
            );
        }
    }

    /// The successor must be the larger of the two, or "superseded" is the wrong
    /// word for it.
    #[test]
    fn the_current_ontology_is_not_smaller_than_the_one_it_replaces() {
        let legacy = HardwareOntology::complete();
        let legacy_properties: usize = legacy.domains.iter().map(|d| d.properties.len()).sum();
        let current = Ontology::build().entities.len();
        assert!(
            current > legacy_properties,
            "crate::ontology declares {current} entities against the legacy \
             {legacy_properties} properties; it should be a superset"
        );
    }
}
