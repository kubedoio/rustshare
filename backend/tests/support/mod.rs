//! Shared test support for the Calendar Application integration suites.
//!
//! `calendar_harness` holds the DB/app setup, tenant/user/token helpers, the
//! response-JSON and multipart builders, and the `SERIAL` guard the calendar
//! suites share. `mock_provider` holds the pluggable mock OAuth + calendar
//! provider server used by the Google and Outlook sync suites.
//!
//! Compiled into each calendar test binary via `mod support;`; unused items in
//! any single binary are expected, so dead-code warnings are allowed here.
#![allow(dead_code)]

pub mod calendar_harness;
pub mod mock_provider;
