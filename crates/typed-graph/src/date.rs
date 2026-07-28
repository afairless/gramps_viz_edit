//! Date types for the Gramps genealogy data model.
//!
//! This module provides [`DateValue`], [`DateQuality`], and [`DateModifier`]
//! types that match Gramps' `DateVal` structure. These are used by the
//! GraphBuilder API for setting dates on persons, events, and families.

/// Quality of a date value — how precise/trustworthy it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum DateQuality {
    /// Exact date is known with certainty.
    #[default]
    Exact,
    /// Date is an estimate.
    Estimated,
    /// Date was calculated from other information.
    Calculated,
}

/// Modifier on a date value — what the date represents.
#[derive(Clone, Debug, PartialEq)]
pub enum DateModifier {
    /// No modifier — exact date.
    None,
    /// Date is known to be before the given value.
    Before,
    /// Date is known to be after the given value.
    After,
    /// Date is about/around the given value.
    About,
    /// Date is between two values.
    Range {
        start: Box<DateValue>,
        end: Box<DateValue>,
    },
    /// Date spans a period from start to end.
    Span {
        start: Box<DateValue>,
        end: Box<DateValue>,
    },
}

/// A Gregorian calendar date value, matching Gramps' `DateVal` structure.
///
/// Supports year-only, year-month, and full year-month-day precision.
/// Dates are AD only (years 1-9999). BC support is not yet implemented.
///
/// # Examples
///
/// ```
/// use typed_graph::date::{DateValue, DateQuality, DateModifier};
///
/// let date = DateValue::new(1870);
/// assert_eq!(date.year, 1870);
/// assert_eq!(date.quality, DateQuality::Exact);
///
/// let full = DateValue::new_ymd(1890, 6, 15);
/// assert_eq!(full.display_text(), "1890-06-15");
///
/// let about = DateValue {
///     modifier: DateModifier::About,
///     ..DateValue::new(1870)
/// };
/// assert_eq!(about.display_text(), "about 1870");
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct DateValue {
    /// Calendar year (1-9999).
    pub year: u16,
    /// Month (1-12), or None if year-only.
    pub month: Option<u8>,
    /// Day (1-31), or None if month is None or day is unknown.
    pub day: Option<u8>,
    /// Quality of the date (Exact, Estimated, Calculated).
    pub quality: DateQuality,
    /// Modifier on the date (None, Before, After, About, Range, Span).
    pub modifier: DateModifier,
    /// Free-form text representation of the date.
    pub text: Option<String>,
}

impl DateValue {
    /// Create a new year-only [`DateValue`] with exact quality and no modifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use typed_graph::date::DateValue;
    ///
    /// let date = DateValue::new(1870);
    /// assert_eq!(date.year, 1870);
    /// assert!(date.month.is_none());
    /// assert!(date.day.is_none());
    /// ```
    pub fn new(year: u16) -> Self {
        DateValue {
            year,
            month: None,
            day: None,
            quality: DateQuality::Exact,
            modifier: DateModifier::None,
            text: None,
        }
    }

    /// Create a full year-month-day [`DateValue`] with exact quality and no modifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use typed_graph::date::DateValue;
    ///
    /// let date = DateValue::new_ymd(1890, 6, 15);
    /// assert_eq!(date.year, 1890);
    /// assert_eq!(date.month, Some(6));
    /// assert_eq!(date.day, Some(15));
    /// ```
    pub fn new_ymd(year: u16, month: u8, day: u8) -> Self {
        DateValue {
            year,
            month: Some(month),
            day: Some(day),
            quality: DateQuality::Exact,
            modifier: DateModifier::None,
            text: None,
        }
    }

    /// Synthesize a display text string from the structured date fields.
    ///
    /// Produces strings matching Gramps' date display conventions:
    /// - Exact: "1870" (year), "1870-06" (year-month), "1870-06-15" (year-month-day)
    /// - About: "about 1870"
    /// - Before: "before 1900"
    /// - After: "after 1950"
    /// - Estimated: "estimated 1805"
    /// - Calculated: "calculated 1805"
    /// - Range: "between 1890 and 1900"
    /// - Span: "from 1890 to 1900"
    ///
    /// # Examples
    ///
    /// ```
    /// use typed_graph::date::{DateValue, DateQuality, DateModifier};
    ///
    /// assert_eq!(DateValue::new(1870).display_text(), "1870");
    /// assert_eq!(DateValue::new_ymd(1870, 6, 15).display_text(), "1870-06-15");
    ///
    /// let about = DateValue { modifier: DateModifier::About, ..DateValue::new(1870) };
    /// assert_eq!(about.display_text(), "about 1870");
    /// ```
    pub fn display_text(&self) -> String {
        // Build the base date string
        let base = match (self.month, self.day) {
            (Some(m), Some(d)) => format!("{:04}-{:02}-{:02}", self.year, m, d),
            (Some(m), None) => format!("{:04}-{:02}", self.year, m),
            (None, _) => format!("{:04}", self.year),
        };

        // Apply modifier prefix
        let modified = match &self.modifier {
            DateModifier::None => base,
            DateModifier::Before => format!("before {}", base),
            DateModifier::After => format!("after {}", base),
            DateModifier::About => format!("about {}", base),
            DateModifier::Range { start, end } => {
                format!(
                    "between {} and {}",
                    start.display_text(),
                    end.display_text()
                )
            }
            DateModifier::Span { start, end } => {
                format!("from {} to {}", start.display_text(), end.display_text())
            }
        };

        // Apply quality prefix (only if not exact)
        match self.quality {
            DateQuality::Exact => modified,
            DateQuality::Estimated => format!("estimated {}", modified),
            DateQuality::Calculated => format!("calculated {}", modified),
        }
    }

    /// Check whether the date value is structurally valid.
    ///
    /// Rules:
    /// - Year must be in [1, 9999]
    /// - Month must be in [1, 12] if Some
    /// - Day must be valid for the given month/year if both month and day are Some
    ///
    /// # Examples
    ///
    /// ```
    /// use typed_graph::date::DateValue;
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
                let max_days = days_in_month(self.year, m);
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

    // -----------------------------------------------------------------------
    // DateValue construction
    // -----------------------------------------------------------------------

    #[test]
    fn date_value_new_year() {
        let date = DateValue::new(1870);
        assert_eq!(date.year, 1870);
        assert!(date.month.is_none());
        assert!(date.day.is_none());
        assert_eq!(date.quality, DateQuality::Exact);
        assert_eq!(date.modifier, DateModifier::None);
    }

    #[test]
    fn date_value_new_ymd() {
        let date = DateValue::new_ymd(1890, 6, 15);
        assert_eq!(date.year, 1890);
        assert_eq!(date.month, Some(6));
        assert_eq!(date.day, Some(15));
        assert_eq!(date.quality, DateQuality::Exact);
        assert_eq!(date.modifier, DateModifier::None);
    }

    #[test]
    fn date_quality_default_is_exact() {
        assert_eq!(DateQuality::default(), DateQuality::Exact);
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
            quality: DateQuality::Exact,
            modifier: DateModifier::None,
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
            quality: DateQuality::Exact,
            modifier: DateModifier::None,
            text: None,
        };
        assert_eq!(date.display_text(), "1870-06");
    }

    #[test]
    fn date_value_display_text_about() {
        let date = DateValue {
            modifier: DateModifier::About,
            ..DateValue::new(1870)
        };
        assert_eq!(date.display_text(), "about 1870");
    }

    #[test]
    fn date_value_display_text_estimated() {
        let date = DateValue {
            quality: DateQuality::Estimated,
            ..DateValue::new(1805)
        };
        assert_eq!(date.display_text(), "estimated 1805");
    }

    #[test]
    fn date_value_display_text_before() {
        let date = DateValue {
            modifier: DateModifier::Before,
            ..DateValue::new(1900)
        };
        assert_eq!(date.display_text(), "before 1900");
    }

    #[test]
    fn date_value_display_text_after() {
        let date = DateValue {
            modifier: DateModifier::After,
            ..DateValue::new(1950)
        };
        assert_eq!(date.display_text(), "after 1950");
    }

    #[test]
    fn date_value_display_text_range() {
        let date = DateValue {
            modifier: DateModifier::Range {
                start: Box::new(DateValue::new(1890)),
                end: Box::new(DateValue::new(1900)),
            },
            ..DateValue::new(1890)
        };
        assert_eq!(date.display_text(), "between 1890 and 1900");
    }

    #[test]
    fn date_value_display_text_span() {
        let date = DateValue {
            modifier: DateModifier::Span {
                start: Box::new(DateValue::new(1890)),
                end: Box::new(DateValue::new(1900)),
            },
            ..DateValue::new(1890)
        };
        assert_eq!(date.display_text(), "from 1890 to 1900");
    }

    #[test]
    fn date_value_display_text_calculated() {
        let date = DateValue {
            quality: DateQuality::Calculated,
            ..DateValue::new(1805)
        };
        assert_eq!(date.display_text(), "calculated 1805");
    }

    #[test]
    fn date_value_display_text_estimated_about() {
        let date = DateValue {
            quality: DateQuality::Estimated,
            modifier: DateModifier::About,
            ..DateValue::new(1870)
        };
        assert_eq!(date.display_text(), "estimated about 1870");
    }

    #[test]
    fn date_value_display_text_range_with_ymd() {
        let date = DateValue {
            modifier: DateModifier::Range {
                start: Box::new(DateValue::new_ymd(1890, 6, 1)),
                end: Box::new(DateValue::new_ymd(1900, 9, 15)),
            },
            ..DateValue::new(1890)
        };
        assert_eq!(date.display_text(), "between 1890-06-01 and 1900-09-15");
    }
}
