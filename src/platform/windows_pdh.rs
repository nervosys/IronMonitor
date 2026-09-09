//! Per-core delivered performance, from the PDH performance counters.
//!
//! `CallNtPowerInformation`'s `CurrentMhz` is the nominal clock on Windows 10
//! and later — it reads the same value for every core, idle and under load, so
//! `platform::windows::get_cpu_frequency` reports no current clock rather than
//! publishing a specification behind a measurement's provenance. That left
//! `cpu.core.{n}.frequency` absent on every Windows machine.
//!
//! `\Processor Information(*)\% Processor Performance` is what Task Manager
//! reads. It is derived by the kernel from the APERF and MPERF counters, so it
//! reflects clock actually delivered, it differs per core, and it exceeds 100%
//! when a core boosts — on the development machine it ranges from 60% to 124%
//! of a 4400 MHz nominal, which is a real 2.6–5.4 GHz spread.
//!
//! **It is a rate, so it needs two collections separated in time.** The query is
//! therefore opened once and kept, and each call differences against the
//! previous collection — no sleeping in a monitor's refresh path.
//!
//! The interval is primed when the query opens: one collection, a 120 ms sleep,
//! and every call from the first onward has something to difference against.
//!
//! That cost is deliberate. The first shape returned `None` from the first call
//! so the *second* would work, which is right for a monitoring loop and wrong
//! for everything else — **a one-shot process only ever makes a first call**.
//! `ironmon cli cpu` printed "Clock: not read" on a machine whose cores were at
//! 5 GHz, and would have done so forever; so would one ontology snapshot or one
//! agent tool call. 120 ms once per process, on the first read of a CPU
//! frequency and never again, buys a correct answer for every caller.

use std::sync::{Mutex, OnceLock};

use windows::core::w;
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE,
};

/// An open PDH query and the counter within it.
///
/// The handles are process-wide integers rather than pointers, but
/// `PdhCollectQueryData` is not safe to call concurrently against one query, so
/// the whole thing lives behind a mutex.
struct PerformanceQuery {
    query: isize,
    counter: isize,
}

// SAFETY: `query` and `counter` are PDH handles, which are process-wide and not
// bound to the opening thread. Every use is serialised by the mutex below.
unsafe impl Send for PerformanceQuery {}

static QUERY: OnceLock<Option<Mutex<PerformanceQuery>>> = OnceLock::new();

fn query() -> Option<&'static Mutex<PerformanceQuery>> {
    QUERY
        .get_or_init(|| {
            let mut query: isize = 0;
            if unsafe { PdhOpenQueryW(None, 0, &mut query) } != 0 {
                return None;
            }
            let mut counter: isize = 0;
            // The English name is used explicitly: the localised counter path
            // differs by system language, and `PdhAddCounterW` would fail on a
            // machine that is not running in English.
            let added = unsafe {
                PdhAddEnglishCounterW(
                    query,
                    w!("\\Processor Information(*)\\% Processor Performance"),
                    0,
                    &mut counter,
                )
            };
            if added != 0 {
                return None;
            }
            // Prime the interval here, once, rather than making the first
            // caller absorb it.
            //
            // A rate needs two collections, and the previous shape returned
            // `None` from the first call so that the *second* had something to
            // difference against. That is correct for a monitoring loop and
            // wrong for everything else: a one-shot process only ever makes a
            // first call. `ironmon cli cpu` printed "Clock: not read" on a machine
            // whose cores were sitting at 5 GHz, and would have done so forever
            // -- as would a single ontology snapshot, or one agent tool call.
            //
            // So the cost is paid at open time: one collection, a short sleep,
            // and the query is usable from the first call onward. ~120 ms once
            // per process, on the first read of a CPU frequency and never again.
            unsafe { PdhCollectQueryData(query) };
            std::thread::sleep(std::time::Duration::from_millis(120));

            Some(Mutex::new(PerformanceQuery { query, counter }))
        })
        .as_ref()
}

/// Delivered performance per logical processor, as a percentage of nominal.
///
/// Indexed by the processor number PDH reports, which is `"<group>,<cpu>"` —
/// the `_Total` rows and any per-group totals are skipped. Returns `None` when
/// the counter is unavailable, and on the first call, which only primes the
/// interval.
pub(crate) fn processor_performance_percent() -> Option<Vec<Option<f64>>> {
    let lock = query()?;
    let state = lock.lock().ok()?;

    if unsafe { PdhCollectQueryData(state.query) } != 0 {
        return None;
    }

    let mut size: u32 = 0;
    let mut count: u32 = 0;
    // The first call reports the buffer size it needs and returns
    // PDH_MORE_DATA; anything else is a real failure.
    unsafe {
        PdhGetFormattedCounterArrayW(state.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None)
    };
    if size == 0 {
        return None;
    }

    let mut buffer = vec![0u8; size as usize];
    let fetched = unsafe {
        PdhGetFormattedCounterArrayW(
            state.counter,
            PDH_FMT_DOUBLE,
            &mut size,
            &mut count,
            Some(buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W),
        )
    };
    if fetched != 0 {
        return None;
    }

    let items = unsafe {
        std::slice::from_raw_parts(
            buffer.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W,
            count as usize,
        )
    };

    let mut out: Vec<Option<f64>> = Vec::new();
    for item in items {
        let name = unsafe { item.szName.to_string() }.unwrap_or_default();
        // "0,5" is group 0 processor 5. "_Total" and "0,_Total" are aggregates.
        let Some((_group, cpu)) = name.split_once(',') else {
            continue;
        };
        let Ok(index) = cpu.parse::<usize>() else {
            continue;
        };
        if out.len() <= index {
            out.resize(index + 1, None);
        }
        out[index] = Some(unsafe { item.FmtValue.Anonymous.doubleValue });
    }

    (!out.is_empty()).then_some(out)
}
