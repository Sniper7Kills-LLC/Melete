//! Paper-style page templates — blank, ruled, wide-ruled, college-ruled.

pub mod blank;
pub mod college_ruled;
pub mod ruled;
pub mod wide_ruled;

pub use blank::{builtin_blank, BUILTIN_BLANK_ID};
pub use college_ruled::{builtin_college_ruled, BUILTIN_COLLEGE_RULED_ID};
pub use ruled::{builtin_ruled, BUILTIN_RULED_ID};
pub use wide_ruled::{builtin_wide_ruled, BUILTIN_WIDE_RULED_ID};
