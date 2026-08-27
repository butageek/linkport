//! Linkport core: config model, URL rule engine, browser launcher and event log.
//!
//! Everything in this crate is pure and platform-independent. Windows-specific
//! integration (default-browser registration, browser discovery) lives in
//! `linkport-win`.

pub mod config;
pub mod engine;
pub mod event;
pub mod launcher;

pub use config::{Browser, Config, PortalConfig, Rule, TARGET_BLOCK};
pub use engine::{Decision, Outcome, RuleTrace};
pub use event::Event;
