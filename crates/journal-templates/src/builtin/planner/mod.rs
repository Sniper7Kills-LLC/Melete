//! Generic planner page templates — daily / monthly goals / quarterly
//! review. Brand-specific planner layouts (Full Focus, Franklin) live in
//! sibling folders.

pub mod daily_planner;
pub mod monthly_goals;
pub mod quarterly_review;

pub use daily_planner::{builtin_daily_planner, BUILTIN_DAILY_PLANNER_ID};
pub use monthly_goals::{builtin_monthly_goals, BUILTIN_MONTHLY_GOALS_ID};
pub use quarterly_review::{builtin_quarterly_review, BUILTIN_QUARTERLY_REVIEW_ID};
