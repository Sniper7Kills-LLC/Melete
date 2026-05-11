use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Addresses a page within a planner by its calendar position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CalendarPageAddress {
    Year {
        year: i32,
        template_index: u32,
    },
    Quarter {
        year: i32,
        quarter: u8,
        template_index: u32,
    },
    Month {
        year: i32,
        month: u8,
        template_index: u32,
    },
    Week {
        year: i32,
        week: u8,
        template_index: u32,
    },
    Day {
        date: NaiveDate,
        template_index: u32,
    },
}

/// A page address within a planner notebook — wraps CalendarPageAddress.
pub type PlannerPageAddress = CalendarPageAddress;

/// Resolves a date to various calendar-level addresses.
pub fn resolve_date(date: NaiveDate) -> DateResolution {
    use chrono::Datelike;

    let year = date.year();
    let month = date.month() as u8;
    let quarter = ((month - 1) / 3) + 1;
    let week = date.iso_week().week() as u8;

    DateResolution {
        year,
        quarter,
        month,
        week,
        date,
    }
}

/// The resolved calendar components for a given date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateResolution {
    pub year: i32,
    pub quarter: u8,
    pub month: u8,
    pub week: u8,
    pub date: NaiveDate,
}
