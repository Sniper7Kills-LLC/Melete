//! Tiny templating engine for planner section + page titles.
//!
//! Supports the placeholders documented on `TitleContext::value`. Unknown
//! placeholders are left literal (e.g. `{foo}` stays as `{foo}`) so format
//! strings degrade gracefully.

use chrono::{Datelike, NaiveDate};

pub struct TitleContext {
    pub date: NaiveDate,
}

impl TitleContext {
    pub fn new(date: NaiveDate) -> Self {
        Self { date }
    }

    fn value(&self, key: &str) -> Option<String> {
        let d = self.date;
        match key {
            "year" => Some(format!("{:04}", d.year())),
            "month" => Some(format!("{:02}", d.month())),
            "month_name" => Some(month_name(d.month()).to_string()),
            "week" => Some(format!("{:02}", d.iso_week().week())),
            "day" => Some(format!("{:02}", d.day())),
            "weekday" => Some(weekday_name(d.weekday()).to_string()),
            "date" => Some(d.format("%Y-%m-%d").to_string()),
            _ => None,
        }
    }
}

fn month_name(m: u32) -> &'static str {
    match m {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

fn weekday_name(wd: chrono::Weekday) -> &'static str {
    use chrono::Weekday::*;
    match wd {
        Mon => "Monday",
        Tue => "Tuesday",
        Wed => "Wednesday",
        Thu => "Thursday",
        Fri => "Friday",
        Sat => "Saturday",
        Sun => "Sunday",
    }
}

/// Render a format string by substituting `{name}` tokens. Tokens that do not
/// match a known variable are left untouched.
pub fn render(format: &str, ctx: &TitleContext) -> String {
    let mut out = String::with_capacity(format.len());
    let bytes = format.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'{' {
            if let Some(end) = format[i + 1..].find('}') {
                let key = &format[i + 1..i + 1 + end];
                if let Some(v) = ctx.value(key) {
                    out.push_str(&v);
                    i += end + 2;
                    continue;
                }
                // Unknown placeholder — pass through literally.
                out.push_str(&format[i..i + end + 2]);
                i += end + 2;
                continue;
            }
            // Unterminated `{` — treat as literal.
            out.push('{');
            i += 1;
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn substitutes_year_month_day() {
        let ctx = TitleContext::new(date(2026, 5, 2));
        assert_eq!(render("{year}-{month}-{day}", &ctx), "2026-05-02");
    }

    #[test]
    fn substitutes_named_components() {
        let ctx = TitleContext::new(date(2026, 5, 2));
        assert_eq!(
            render("{weekday} {month_name} {day}", &ctx),
            "Saturday May 02"
        );
    }

    #[test]
    fn iso_date_token() {
        let ctx = TitleContext::new(date(2026, 1, 1));
        assert_eq!(render("{date}", &ctx), "2026-01-01");
    }

    #[test]
    fn week_token_zero_padded() {
        let ctx = TitleContext::new(date(2026, 1, 5));
        // 2026-01-05 is the Monday starting ISO week 02.
        assert_eq!(render("Week {week} {year}", &ctx), "Week 02 2026");
    }

    #[test]
    fn unknown_placeholder_passes_through() {
        let ctx = TitleContext::new(date(2026, 5, 2));
        assert_eq!(render("{foo} {year}", &ctx), "{foo} 2026");
    }

    #[test]
    fn literal_braces_preserved() {
        let ctx = TitleContext::new(date(2026, 5, 2));
        assert_eq!(render("plain text", &ctx), "plain text");
    }
}
