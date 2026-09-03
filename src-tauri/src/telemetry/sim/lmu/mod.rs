//! Everything specific to Le Mans Ultimate.
//!
//! `install` is always compiled because the control panel reports whether the
//! telemetry plugin is present even in builds made without the shared-memory
//! SDK. The rest needs the SDK header and the `bridge.cpp` symbols, so it is
//! gated the same way `build.rs` gates the native bridge.

pub(crate) mod install;

#[cfg(all(target_os = "windows", lmu_sdk))]
mod driver_ranks;
#[cfg(all(target_os = "windows", lmu_sdk))]
mod event_split;
#[cfg(all(target_os = "windows", lmu_sdk))]
mod racecontrol;
#[cfg(all(target_os = "windows", lmu_sdk))]
mod rest;
#[cfg(all(target_os = "windows", lmu_sdk))]
mod source;

#[cfg(all(target_os = "windows", lmu_sdk))]
pub(crate) use source::LmuTelemetrySource;
