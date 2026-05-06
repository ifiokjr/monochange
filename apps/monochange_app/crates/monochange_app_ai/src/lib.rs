//! AI agents — OpenRouter client and task-specific agents.
//!
//! Handles communication with OpenRouter API for AI-powered features:
//! - Issue-to-changeset scoping
//! - Feature request analysis
//! - Changelog polishing
//! - Feedback categorization and triage

pub mod client;
pub mod changeset;
pub mod changelog;
pub mod scoping;
