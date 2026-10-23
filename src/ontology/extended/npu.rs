//! Neural processing units, as `crate::silicon::query_npus` enumerates them.

#[allow(unused_imports)]
use super::super::resolve::*;
#[allow(unused_imports)]
use super::super::{Domain as D, Entity, EntityKind as K, Provenance as P, Unit as U};

const NO_NPU_WHY: &str = "no neural processing unit was enumerated on this machine";
const CORES_WHY: &str = concat!(
    "the platform reports no core count for this NPU. It is not inferred from ",
    "the vendor, so no figure is given"
);
const UTILIZATION_WHY: &str = concat!(
    "this NPU's platform reader does not measure utilization, which is not the ",
    "same as an idle NPU"
);
const POWER_WHY: &str = "the platform reports no power draw for this NPU";
const FREQUENCY_WHY: &str = "the platform reports no clock frequency for this NPU";

pub(super) fn declare(add: &mut dyn FnMut(Entity)) {
    // `npu.<none>` is declared for every domain by the base schema, so it is not
    // declared here.
    add(Entity::new(
        "npu.{n}.name",
        D::Npu,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "The NPU's name as the platform reports it, such as Apple Neural Engine or Intel AI Boost.",
    ));
    add(Entity::new(
        "npu.{n}.vendor",
        D::Npu,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "The vendor of the NPU, as the platform reports it.",
    ));
    add(Entity::new(
        "npu.{n}.cores",
        D::Npu,
        K::Identity,
        Some(U::Count),
        P::Specification,
        true,
        "The number of compute units the NPU declares. A property of the silicon, not a measurement of this run.",
    ));
    add(Entity::new(
        "npu.{n}.utilization",
        D::Npu,
        K::Measurement,
        Some(U::Percent),
        P::Measured,
        true,
        "The NPU's busy percentage, sampled from the platform, where the platform measures it.",
    ));
    add(Entity::new(
        "npu.{n}.power",
        D::Npu,
        K::Measurement,
        Some(U::Watts),
        P::Measured,
        true,
        "The NPU's power draw in watts, sampled from the platform, where the platform reports it.",
    ));
    add(Entity::new(
        "npu.{n}.frequency",
        D::Npu,
        K::Measurement,
        Some(U::Megahertz),
        P::Measured,
        true,
        "The NPU's clock frequency in MHz, where the platform reports it.",
    ));
}

pub(super) fn resolve(out: &mut Vec<Reading>) {
    let npus = match crate::silicon::query_npus() {
        Ok(npus) => npus,
        Err(e) => {
            out.push(Reading::unavailable(
                "npu.<none>",
                None,
                format!("the NPU query failed: {e}"),
            ));
            return;
        }
    };
    if npus.is_empty() {
        out.push(Reading::unavailable("npu.<none>", None, NO_NPU_WHY));
        return;
    }

    for (index, npu) in npus.iter().enumerate() {
        let base = format!("npu.{index}");
        push_text(out, format!("{base}.name"), &npu.name);
        push_text(out, format!("{base}.vendor"), &npu.vendor);
        push_spec_opt(
            out,
            format!("{base}.cores"),
            npu.cores.map(|c| serde_json::json!(c)),
            Some(U::Count),
            CORES_WHY,
        );
        push_opt(
            out,
            format!("{base}.utilization"),
            npu.utilization.map(|u| serde_json::json!(u)),
            Some(U::Percent),
            UTILIZATION_WHY,
        );
        push_opt(
            out,
            format!("{base}.power"),
            npu.power_watts.map(|w| serde_json::json!(w)),
            Some(U::Watts),
            POWER_WHY,
        );
        push_opt(
            out,
            format!("{base}.frequency"),
            npu.frequency_mhz.map(|f| serde_json::json!(f)),
            Some(U::Megahertz),
            FREQUENCY_WHY,
        );
    }
}
