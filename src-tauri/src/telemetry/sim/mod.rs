//! Telemetry sources, one module per simulator.
//!
//! Everything a single simulator knows about — its shared-memory bridge, REST
//! clients, log readers and install discovery — lives under `sim::<id>`. The
//! modules outside `sim` stay simulator agnostic and must not reach into one:
//! they only see the frame and the source contract in the parent module.

pub(crate) mod lmu;
#[cfg(not(all(target_os = "windows", lmu_sdk)))]
pub(crate) mod mock;
