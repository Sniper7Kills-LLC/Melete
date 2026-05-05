//! Note-taking page templates  -  Cornell notes, music staff.

pub mod cornell_notes;
pub mod music_staff;

pub use cornell_notes::{builtin_cornell_notes, BUILTIN_CORNELL_NOTES_ID};
pub use music_staff::{builtin_music_staff, BUILTIN_MUSIC_STAFF_ID};
