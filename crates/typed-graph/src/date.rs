//! Date extension methods for the generated Gramps date types.
//!
//! This module adds convenience constructors, validation, and display
//! methods to the generated [`DateValue`] type from `schema.rs`.

use crate::DateModifier;
use crate::DateQuality;
use crate::DateValue;

impl DateValue {
    /// Create a new year-only [`DateValue`] with exact quality and no modifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use typed_graph::{DateValue, DateQuality};
    ///
    /// let date = DateValue::new(1870);
    /// assert_eq!(date.year, 1870);
    /// assert!(date.month.is_none());
    /// assert!(date.day.is_none());
    /// assert_eq!(date.quality, Some(DateQuality::Exact));
    /// ```
    pub fn new(year: i32) -> Self {
        DateValue {
            year,
            month: None,
            day: None,
            quality: Some(DateQuality::Exact),
            modifier: Some(DateModifier::None),
            text: None,
        }
    }

    /// Create a full year-month-day [`DateValue`] with exact quality and no modifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use typed_graph::DateValue;
    ///
    /// let date = DateValue::new_ymd(1890, 6, 15);
    /// assert_eq!(date.year, 1890);
    /// assert_eq!(date.month, Some(6));
    /// assert_eq!(date.day, Some(15));
    /// ```
    pub fn new_ymd(year: i32, month: i32, day: i32) -> Self {
        DateValue {
            year,
            month: Some(month),
            day: Some(day),
            quality: Some(DateQuality::Exact),
            modifier: Some(DateModifier::None),
            text: None,
        }
    }

    /// Synthesize a display text string from the structured date fields.
    ///
    /// If `text` is set, returns it directly. Otherwise, synthesizes
    /// from the structured fields.
    ///
    /// # Examples
    ///
    /// ```
    /// use typed_graph::{DateValue, DateQuality, DateModifier};
    ///
    /// assert_eq!(DateValue::new(1870).display_text(), "1870");
    /// assert_eq!(DateValue::new_ymd(1870, 6, 15).display_text(), "1870-06-15");
    ///
    /// let mut about = DateValue::new(1870);
    /// about.modifier = Some(DateModifier::About);
    /// assert_eq!(about.display_text(), "about 1870");
    /// ```
    pub fn display_text(&self) -> String {
        // If text is set, use it directly
        if let Some(ref t) = self.text {
            if !t.is_empty() {
                return t.clone();
            }
        }

        // Build the base date string
        let base = match (self.month, self.day) {
            (Some(m), Some(d)) => format!("{:04}-{:02}-{:02}", self.year, m, d),
            (Some(m), None) => format!("{:04}-{:02}", self.year, m),
            (None, _) => format!("{:04}", self.year),
        };

        // Apply modifier prefix
        let modified = match self.modifier {
            Some(DateModifier::None) | None => base,
            Some(DateModifier::Before) => format!("before {}", base),
            Some(DateModifier::After) => format!("after {}", base),
            Some(DateModifier::About) => format!("about {}", base),
            Some(DateModifier::Range) => format!("between (range) {}", base),
            Some(DateModifier::Span) => format!("from (span) {}", base),
        };

        // Apply quality prefix (only if not exact)
        match self.quality {
            Some(DateQuality::Exact) | None => modified,
            Some(DateQuality::Estimated) => format!("estimated {}", modified),
            Some(DateQuality::Calculated) => format!("calculated {}", modified),
        }
    }

    /// Check whether the date value is structurally valid.
    ///
    /// Rules:
    /// - Year must be in [1, 9999]
    /// - Month must be in [1, 12] if Some
    /// - Day must be valid for the given month/year if both are present
    ///
    /// # Examples
    ///
    /// ```
    /// use typed_graph::DateValue;
    ///
    /// assert!(DateValue::new(1870).is_valid());
    /// assert!(DateValue::new_ymd(2024, 2, 29).is_valid()); // leap year
    /// assert!(!DateValue::new(0).is_valid()); // year 0 is invalid
    /// assert!(!DateValue::new_ymd(2023, 2, 29).is_valid()); // not a leap year
    /// assert!(!DateValue::new_ymd(1870, 13, 1).is_valid()); // invalid month
    /// ```
    pub fn is_valid(&self) -> bool {
        // Year must be in [1, 9999]
        if self.year < 1 || self.year > 9999 {
            return false;
        }

        // Month must be in [1, 12] if Some
        if let Some(m) = self.month {
            if !(1..=12).contains(&m) {
                return false;
            }

            // Day must be valid for the given month/year if both are present
            if let Some(d) = self.day {
                if d < 1 {
                    return false;
                }
                // Safe cast: year is in [1, 9999] and month is in [1, 12] at this point
                #[allow(clippy::cast_sign_loss)]
                let max_days = days_in_month(self.year as u16, m as u8) as i32;
                if d > max_days {
                    return false;
                }
            }
        } else if self.day.is_some() {
            // Day without month is invalid
            return false;
        }

        true
    }
}

/// Return the number of days in a given month (1-12) for a given year.
fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Return whether a year is a leap year (Gregorian calendar rules).
fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DateModifier, DateQuality};

    // -----------------------------------------------------------------------
    // DateValue construction
    // -----------------------------------------------------------------------

    #[test]
    fn date_value_new_year() {
        let date = DateValue::new(1870);
        assert_eq!(date.year, 1870);
        assert!(date.month.is_none());
        assert!(date.day.is_none());
        assert_eq!(date.quality, Some(DateQuality::Exact));
        assert_eq!(date.modifier, Some(DateModifier::None));
    }

    #[test]
    fn date_value_new_ymd() {
        let date = DateValue::new_ymd(1890, 6, 15);
        assert_eq!(date.year, 1890);
        assert_eq!(date.month, Some(6));
        assert_eq!(date.day, Some(15));
        assert_eq!(date.quality, Some(DateQuality::Exact));
        assert_eq!(date.modifier, Some(DateModifier::None));
    }

    #[test]
    fn date_quality_default_is_exact() {
        assert_eq!(DateQuality::default(), DateQuality::Calculated);
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    #[test]
    fn date_value_is_valid() {
        // Valid dates
        assert!(DateValue::new(1).is_valid());
        assert!(DateValue::new(9999).is_valid());
        assert!(DateValue::new(1870).is_valid());
        assert!(DateValue::new_ymd(2024, 2, 29).is_valid()); // leap year
        assert!(DateValue::new_ymd(2023, 3, 31).is_valid());
        assert!(DateValue::new_ymd(1900, 1, 1).is_valid());

        // Invalid dates
        assert!(!DateValue::new(0).is_valid()); // year 0
        assert!(!DateValue::new(10000).is_valid()); // year > 9999
        assert!(!DateValue::new_ymd(2023, 13, 1).is_valid()); // month > 12
        assert!(!DateValue::new_ymd(2023, 0, 1).is_valid()); // month 0
        assert!(!DateValue::new_ymd(2023, 2, 29).is_valid()); // not leap year
        assert!(!DateValue::new_ymd(2023, 4, 31).is_valid()); // April has 30 days
        assert!(!DateValue::new_ymd(2023, 6, 0).is_valid()); // day 0
    }

    #[test]
    fn date_value_day_without_month_is_invalid() {
        let date = DateValue {
            year: 1870,
            month: None,
            day: Some(15),
            quality: Some(DateQuality::Exact),
            modifier: Some(DateModifier::None),
            text: None,
        };
        assert!(!date.is_valid());
    }

    #[test]
    fn date_value_valid_leap_years() {
        // Year 2000 is divisible by 400 → leap year
        assert!(DateValue::new_ymd(2000, 2, 29).is_valid());
        // Year 1900 is divisible by 100 but not 400 → not leap year
        assert!(!DateValue::new_ymd(1900, 2, 29).is_valid());
        // Year 2024 is divisible by 4 → leap year
        assert!(DateValue::new_ymd(2024, 2, 29).is_valid());
        // Year 2023 is not divisible by 4 → not leap year
        assert!(!DateValue::new_ymd(2023, 2, 29).is_valid());
    }

    // -----------------------------------------------------------------------
    // Display text
    // -----------------------------------------------------------------------

    #[test]
    fn date_value_display_text_exact() {
        assert_eq!(DateValue::new(1870).display_text(), "1870");
        assert_eq!(DateValue::new_ymd(1870, 6, 15).display_text(), "1870-06-15");
    }

    #[test]
    fn date_value_display_text_exact_ym() {
        let date = DateValue {
            year: 1870,
            month: Some(6),
            day: None,
            quality: Some(DateQuality::Exact),
            modifier: Some(DateModifier::None),
            text: None,
        };
        assert_eq!(date.display_text(), "1870-06");
    }

    #[test]
    fn date_value_display_text_about() {
        let date = DateValue {
            modifier: Some(DateModifier::About),
            ..DateValue::new(1870)
        };
        assert_eq!(date.display_text(), "about 1870");
    }

    #[test]
    fn date_value_display_text_estimated() {
        let date = DateValue {
            quality: Some(DateQuality::Estimated),
            ..DateValue::new(1805)
        };
        assert_eq!(date.display_text(), "estimated 1805");
    }

    #[test]
    fn date_value_display_text_before() {
        let date = DateValue {
            modifier: Some(DateModifier::Before),
            ..DateValue::new(1900)
        };
        assert_eq!(date.display_text(), "before 1900");
    }

    #[test]
    fn date_value_display_text_after() {
        let date = DateValue {
            modifier: Some(DateModifier::After),
            ..DateValue::new(1950)
        };
        assert_eq!(date.display_text(), "after 1950");
    }

    #[test]
    fn date_value_display_text_calculated() {
        let date = DateValue {
            quality: Some(DateQuality::Calculated),
            ..DateValue::new(1805)
        };
        assert_eq!(date.display_text(), "calculated 1805");
    }

    #[test]
    fn date_value_display_text_estimated_about() {
        let date = DateValue {
            quality: Some(DateQuality::Estimated),
            modifier: Some(DateModifier::About),
            ..DateValue::new(1870)
        };
        assert_eq!(date.display_text(), "estimated about 1870");
    }

    #[test]
    fn date_value_display_text_uses_text_field_when_set() {
        let date = DateValue {
            text: Some("custom date string".to_string()),
            ..DateValue::new(1870)
        };
        assert_eq!(date.display_text(), "custom date string");
    }
}
