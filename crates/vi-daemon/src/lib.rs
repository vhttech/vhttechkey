//! Deprecated: use the vime-* equivalent crate instead.
//! vi-daemon library — shared types and helpers used by the binary and tests.
//!
//! # Lock ordering
//!
//! Two shared-state locks are used across the event-loop and IPC paths:
//!
//! - **Lock A** — engine method (input-method selector, e.g. `vi-telex`)
//! - **Lock B** — Wayland state (`WaylandShared`: active, surrounding, serial, pending_preedit)
//!
//! **Rule**: whenever both locks must be held simultaneously, always acquire
//! Lock A before Lock B.  Any code path that needs to flush or invalidate
//! preedit after a method change must update the engine method (Lock A) first
//! and only then acquire the Wayland state (Lock B).

pub mod compat;
pub mod dbus_service;
pub mod detect;
pub mod event_loop;
pub mod ipc;
pub mod signal;
pub mod watchdog;

pub use vi_core as core;
pub use vi_platform as platform;
