//! Franklin Planner-style page templates — daily and weekly spreads.

pub mod franklin_daily;
pub mod franklin_weekly;

pub use franklin_daily::{builtin_franklin_daily, BUILTIN_FRANKLIN_DAILY_ID};
pub use franklin_weekly::{builtin_franklin_weekly, BUILTIN_FRANKLIN_WEEKLY_ID};
