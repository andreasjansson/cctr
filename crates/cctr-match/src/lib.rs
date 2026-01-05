//! Pattern matching with template variables and constraints.
//!
//! This crate provides a way to match text against patterns containing
//! template variables, with optional type declarations and constraints.
//!
//! # Example
//!
//! ```
//! use cctr_match::{Pattern, VarType};
//!
//! let pattern = Pattern::new("Completed in {{ time }}s")
//!     .var("time", VarType::Number)
//!     .constraint("time > 0")
//!     .constraint("time < 60");
//!
//! assert!(pattern.matches("Completed in 1.5s").unwrap());
//! assert!(!pattern.matches("Completed in 120s").unwrap()); // constraint fails
//! ```

mod expr;
mod matcher;

pub use expr::{EvalError, Value};
pub use matcher::{MatchError, MatchResult, Pattern, VarType};
