//! GPU energy for an inference engine, and the electricity price it is costed at.
//!
//! An engine reports tokens; the GPU reports energy. Joules per token is the energy
//! the engine's GPUs used in a window over the tokens the engine generated in it.
//! Both are cumulative counters, so the figure needs two samples and is never given
//! from one.
//!
//! The energy is the whole GPU's, from NVML's total energy counter. It includes any
//! other work running on those GPUs in the same window, and the reason text says so.
//! It is never estimated from power draw: an integral of sampled power misses every
//! change between samples, and would be a different figure under the same name.
//!
//! Which GPUs belong to an engine is taken only from something that says so: the
//! GPUs IronWorks lists in its own metrics, or `IRONMON_<PROVIDER>_GPUS` set by the
//! operator. Nothing is assumed from the machine having one GPU, since a CPU-only
//! engine on that machine would then be charged for whatever the GPU was doing.

use super::{ProviderKind, ProviderSample};

/// The cumulative energy of an engine's GPUs at one poll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnergyReading {
    /// Sum of NVML's total-energy counters over the GPUs, in millijoules.
    pub millijoules: u64,
    /// The GPUs summed, as NVML names them (UUIDs), in a fixed order. A window is
    /// only taken between two readings over the same set.
    pub gpus: Vec<String>,
}

/// Energy at one poll, or why there is none.
pub type Energy = Result<EnergyReading, String>;

/// A GPU named by whoever says the engine runs on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuSelector {
    Uuid(String),
    PciBusId(String),
    Index(u32),
}

/// The variable that names a provider's GPUs, such as `IRONMON_VLLM_GPUS`.
pub fn gpus_env(kind: ProviderKind) -> String {
    format!("IRONMON_{}_GPUS", kind.as_str().to_ascii_uppercase())
}

/// The variable that sets the electricity price, as `<amount> <currency>` per kWh.
pub const PRICE_ENV: &str = "IRONMON_ELECTRICITY_PRICE";

/// An electricity price per kilowatt-hour, as the operator set it.
#[derive(Debug, Clone, PartialEq)]
pub struct Price {
    pub per_kwh: f64,
    pub currency: String,
}

/// Parse `0.15 USD`, `0.15 USD/kWh` or `0.15USD`. The currency is required: a bare
/// number would be costed in a currency nobody named.
pub fn parse_price(text: &str) -> Result<Price, String> {
    let text = text.trim();
    let text = text
        .strip_suffix("/kWh")
        .or_else(|| text.strip_suffix("/kwh"))
        .unwrap_or(text)
        .trim();
    let split = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let (number, currency) = text.split_at(split);
    let per_kwh: f64 = number
        .parse()
        .map_err(|_| format!("{PRICE_ENV} must start with a number, as in `0.15 USD`"))?;
    if !per_kwh.is_finite() || per_kwh < 0.0 {
        return Err(format!("{PRICE_ENV} must be a non-negative number"));
    }
    let currency = currency.trim();
    if currency.is_empty() || !currency.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(format!(
            "{PRICE_ENV} must name a currency after the number, as in `0.15 USD`"
        ));
    }
    Ok(Price {
        per_kwh,
        currency: currency.to_ascii_uppercase(),
    })
}

/// The price from [`PRICE_ENV`], or why there is none.
pub fn price_from_env() -> Result<Price, String> {
    match std::env::var(PRICE_ENV) {
        Ok(v) if !v.trim().is_empty() => parse_price(&v),
        _ => Err(format!(
            "set {PRICE_ENV} (for example `0.15 USD`) to cost local inference energy"
        )),
    }
}

/// Parse a GPU list: comma-separated UUIDs (`GPU-…`), PCI bus ids (`0000:01:00.0`)
/// or NVML indices.
pub fn parse_selectors(text: &str) -> Result<Vec<GpuSelector>, String> {
    let mut out = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if part.to_ascii_uppercase().starts_with("GPU-") {
            out.push(GpuSelector::Uuid(part.to_string()));
        } else if part.contains(':') {
            out.push(GpuSelector::PciBusId(part.to_string()));
        } else if let Ok(index) = part.parse::<u32>() {
            out.push(GpuSelector::Index(index));
        } else {
            return Err(format!(
                "{part:?} is not a GPU UUID, PCI bus id or NVML index"
            ));
        }
    }
    if out.is_empty() {
        return Err("the GPU list is empty".into());
    }
    Ok(out)
}

/// The GPUs a sample says its engine runs on, or why none are known.
pub fn selectors_for(
    kind: ProviderKind,
    sample: &ProviderSample,
) -> Result<Vec<GpuSelector>, String> {
    let var = gpus_env(kind);
    if let Ok(text) = std::env::var(&var) {
        if !text.trim().is_empty() {
            return parse_selectors(&text).map_err(|e| format!("{var}: {e}"));
        }
    }
    if let Some(gpus) = sample.ironworks.as_ref().and_then(|s| s.per_gpu.as_ref()) {
        let selectors: Vec<GpuSelector> = gpus
            .iter()
            .filter_map(|g| {
                g.uuid
                    .clone()
                    .map(GpuSelector::Uuid)
                    .or_else(|| g.pci_bus_id.clone().map(GpuSelector::PciBusId))
            })
            .collect();
        if !selectors.is_empty() && selectors.len() == gpus.len() {
            return Ok(selectors);
        }
        return Err(
            "the engine lists GPUs without a UUID or PCI bus id, so they cannot be found".into(),
        );
    }
    Err(format!(
        "{} does not say which GPUs it runs on; set {var} to their UUIDs or NVML indices",
        kind.display_name()
    ))
}

/// The engine's GPU energy now, or why it cannot be read.
pub fn read_for(kind: ProviderKind, sample: &ProviderSample) -> Energy {
    let selectors = selectors_for(kind, sample)?;
    read(&selectors)
}

#[cfg(feature = "nvidia")]
fn nvml() -> Result<&'static nvml_wrapper::Nvml, String> {
    static NVML: std::sync::OnceLock<Result<nvml_wrapper::Nvml, String>> =
        std::sync::OnceLock::new();
    NVML.get_or_init(|| nvml_wrapper::Nvml::init().map_err(|e| format!("NVML: {e}")))
        .as_ref()
        .map_err(Clone::clone)
}

/// Sum NVML's total-energy counters over the selected GPUs.
#[cfg(feature = "nvidia")]
pub fn read(selectors: &[GpuSelector]) -> Energy {
    let nvml = nvml()?;
    let mut millijoules: u64 = 0;
    let mut gpus = Vec::new();
    for selector in selectors {
        let device = match selector {
            GpuSelector::Uuid(u) => nvml.device_by_uuid(u.as_str()),
            GpuSelector::PciBusId(p) => nvml.device_by_pci_bus_id(p.as_str()),
            GpuSelector::Index(i) => nvml.device_by_index(*i),
        }
        .map_err(|e| format!("NVML could not find {selector:?}: {e}"))?;
        let energy = device.total_energy_consumption().map_err(|e| {
            format!("NVML reports no energy counter for {selector:?} (it needs a Volta or newer GPU): {e}")
        })?;
        millijoules = millijoules.saturating_add(energy);
        gpus.push(device.uuid().unwrap_or_else(|_| format!("{selector:?}")));
    }
    gpus.sort();
    Ok(EnergyReading { millijoules, gpus })
}

/// Without NVML there is no energy counter to read.
#[cfg(not(feature = "nvidia"))]
pub fn read(_selectors: &[GpuSelector]) -> Energy {
    Err("this build has no NVML support, so GPU energy is not read".into())
}

/// Joules over the window between two readings of the same GPUs, or why not.
pub fn joules_between(before: &Energy, now: &Energy) -> Result<f64, String> {
    let (before, now) = match (before, now) {
        (Ok(b), Ok(n)) => (b, n),
        (Err(e), _) | (_, Err(e)) => return Err(e.clone()),
    };
    if before.gpus != now.gpus {
        return Err("the engine's GPUs changed between samples".into());
    }
    if now.millijoules < before.millijoules {
        return Err("the GPU energy counter went backwards (a driver reload resets it)".into());
    }
    Ok((now.millijoules - before.millijoules) as f64 / 1000.0)
}

/// Cost of `joules` at `price`.
pub fn cost_of(joules: f64, price: &Price) -> f64 {
    joules / 3_600_000.0 * price.per_kwh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_price_needs_a_number_and_a_currency() {
        assert_eq!(
            parse_price("0.15 USD/kWh"),
            Ok(Price {
                per_kwh: 0.15,
                currency: "USD".into()
            })
        );
        assert_eq!(parse_price("0.3eur").unwrap().currency, "EUR");
        assert!(parse_price("0.15").is_err());
        assert!(parse_price("USD").is_err());
        assert!(parse_price("-1 USD").is_err());
    }

    #[test]
    fn gpu_lists_take_uuids_bus_ids_and_indices() {
        assert_eq!(
            parse_selectors("GPU-abc, 0000:01:00.0,2"),
            Ok(vec![
                GpuSelector::Uuid("GPU-abc".into()),
                GpuSelector::PciBusId("0000:01:00.0".into()),
                GpuSelector::Index(2),
            ])
        );
        assert!(parse_selectors("gpu zero").is_err());
        assert!(parse_selectors(" , ").is_err());
    }

    #[test]
    fn an_engine_that_names_no_gpu_gets_no_energy() {
        // vLLM's metrics carry no GPU identity, and the variable is unset here.
        let reason = selectors_for(ProviderKind::Vllm, &ProviderSample::default()).unwrap_err();
        assert!(reason.contains("IRONMON_VLLM_GPUS"), "{reason}");
    }

    #[test]
    fn energy_between_two_readings_of_the_same_gpus() {
        let at = |mj| -> Energy {
            Ok(EnergyReading {
                millijoules: mj,
                gpus: vec!["GPU-a".into()],
            })
        };
        assert_eq!(joules_between(&at(1_000), &at(6_000)), Ok(5.0));
        assert!(joules_between(&at(6_000), &at(1_000)).is_err());
        let other: Energy = Ok(EnergyReading {
            millijoules: 9_000,
            gpus: vec!["GPU-b".into()],
        });
        assert!(joules_between(&at(1_000), &other).is_err());
        assert!(joules_between(&Err("no NVML".into()), &at(1)).is_err());
    }

    #[test]
    fn a_kilowatt_hour_costs_its_price() {
        let price = Price {
            per_kwh: 0.2,
            currency: "USD".into(),
        };
        assert!((cost_of(3_600_000.0, &price) - 0.2).abs() < 1e-12);
    }
}
