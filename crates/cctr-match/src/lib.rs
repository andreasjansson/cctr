//! Pattern matching with template variables and constraints.
//!
//! This crate provides a way to match text against patterns containing
//! template variables, with optional type declarations and constraints.
//!
//! # Example
//!
//! ```
//! use cctr_match::{Pattern, VarType, MatchError};
//!
//! let pattern = Pattern::new("Completed in {{ time }}s")
//!     .var("time", VarType::Number)
//!     .constraint("time > 0")
//!     .constraint("time < 60");
//!
//! // Pattern matches and constraints satisfied
//! assert!(pattern.matches("Completed in 1.5s").unwrap());
//!
//! // Pattern matches but constraint fails - returns error
//! assert!(matches!(
//!     pattern.matches("Completed in 120s"),
//!     Err(MatchError::ConstraintNotSatisfied { .. })
//! ));
//!
//! // Pattern doesn't match at all - returns Ok(false)
//! assert!(!pattern.matches("Failed in 1.5s").unwrap());
//! ```

mod expr;
mod matcher;

pub use expr::{EvalError, Value};
pub use matcher::{MatchError, MatchResult, Pattern, VarType};
