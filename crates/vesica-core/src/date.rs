//! Partial dates: a range plus an explicit precision.
//!
//! Investigative sources say "2019", "March 2019" and
//! "2019-03-14T08:22:00Z", and the difference between them is evidence. A
//! [`PartialDate`] keeps it: a lower bound, an upper bound and a
//! [`DatePrecision`] saying how much of the value was actually known.
//!
//! There is deliberately **no** accessor that returns a single instant. If you
//! find yourself wanting one, you want [`PartialDate::lower_bound`] or
//! [`PartialDate::upper_bound`] and you should say in the calling code which
//! end of the range you meant and why. Comparison goes through
//! [`PartialDate::compare`], which can answer "these overlap, I cannot order
//! them" — a point-in-time cannot express that, which is the whole reason this
//! type exists.

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::warning::{Warning, WarningCode};

/// How much of a date was actually known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatePrecision {
    /// Only the year: `2019`.
    Year,
    /// Year and month: `2019-03`.
    Month,
    /// A calendar day: `2019-03-14`.
    Day,
    /// A timestamp: `2019-03-14T08:22:00Z`.
    Instant,
}

impl DatePrecision {
    /// The stable string form, as stored in `entity_prop.date_prec`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Year => "year",
            Self::Month => "month",
            Self::Day => "day",
            Self::Instant => "instant",
        }
    }

    /// Parse the stored string form.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "year" => Some(Self::Year),
            "month" => Some(Self::Month),
            "day" => Some(Self::Day),
            "instant" => Some(Self::Instant),
            _ => None,
        }
    }
}

impl std::fmt::Display for DatePrecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which way round a `dd/mm/yyyy`-style date should be read when both
/// readings are possible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateStyle {
    /// `12/04/1978` is 12 April 1978. The default everywhere except the
    /// handful of regions below.
    DayFirst,
    /// `12/04/1978` is 4 December 1978.
    MonthFirst,
}

impl DateStyle {
    /// The convention used in an ISO-3166-1 alpha-2 region.
    ///
    /// Only the month-first regions need listing; everywhere else, and when
    /// the case has no region set, is day-first.
    pub fn for_region(region: Option<&str>) -> Self {
        match region.map(str::to_ascii_uppercase).as_deref() {
            Some("US" | "PH" | "FM" | "PW" | "MH" | "AS" | "GU" | "MP" | "VI") => Self::MonthFirst,
            _ => Self::DayFirst,
        }
    }
}

/// Why a date could not be represented as a [`PartialDate`].
///
/// These are genuine errors rather than warnings: unlike a bad check digit,
/// there is no range to store. `2019-02-29` names a day that did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DateError {
    /// The input was empty or whitespace only.
    #[error("a date is required but the value was empty")]
    Empty,
    /// The year had two digits, so the century is a guess.
    #[error("`{found}` has a two-digit year; the century is ambiguous")]
    TwoDigitYear {
        /// The offending component as written.
        found: String,
    },
    /// The value is not in any shape this parser accepts.
    #[error("`{found}` is not a recognised date")]
    Unrecognised {
        /// The input, trimmed.
        found: String,
    },
    /// A month outside 1-12.
    #[error("month {month} does not exist")]
    MonthOutOfRange {
        /// The month as written.
        month: u32,
    },
    /// A day that does not exist in that month of that year.
    #[error("{year:04}-{month:02}-{day:02} is not a real date")]
    DayOutOfRange {
        /// Year component.
        year: i32,
        /// Month component.
        month: u32,
        /// Day component.
        day: u32,
    },
    /// A year outside the range this system represents.
    #[error("year {year} is outside the supported range 1-9999")]
    YearOutOfRange {
        /// The year as parsed.
        year: i32,
    },
    /// The time-of-day part could not be read.
    #[error("`{found}` is not a recognised time of day")]
    BadTime {
        /// The time portion as written.
        found: String,
    },
    /// A caller-supplied lower bound was later than the upper bound.
    #[error("the lower bound is later than the upper bound")]
    BoundsInverted,
}

impl DateError {
    /// A machine-actionable description of what a valid value looks like.
    ///
    /// An agent reads this to correct itself without asking a human.
    pub fn fix(&self) -> String {
        match self {
            Self::Empty => "supply an ISO-8601 date: `2019`, `2019-03`, `2019-03-14` or \
                 `2019-03-14T08:22:00Z`"
                .to_string(),
            Self::TwoDigitYear { .. } => {
                "write the full four-digit year, e.g. `1978` not `78`; the century is \
                 never guessed"
                    .to_string()
            }
            Self::Unrecognised { .. } => {
                "use an ISO-8601 date or prefix of one: `2019`, `2019-03`, `2019-03-14`, \
                 `2019-03-14T08:22:00Z`. Day-first `14/03/2019` is also accepted."
                    .to_string()
            }
            Self::MonthOutOfRange { .. } => "use a month between 01 and 12".to_string(),
            Self::DayOutOfRange { .. } => {
                "use a day that exists in that month; February has 29 days only in a \
                 leap year"
                    .to_string()
            }
            Self::YearOutOfRange { .. } => "use a year between 1 and 9999".to_string(),
            Self::BadTime { .. } => {
                "use `HH:MM`, `HH:MM:SS` or `HH:MM:SS.sss`, optionally followed by `Z` \
                 or an offset like `+02:00`"
                    .to_string()
            }
            Self::BoundsInverted => "supply bounds with lower <= upper".to_string(),
        }
    }
}

/// The result of reading a date out of text: the date, plus anything the
/// parser had to assume to get there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDate {
    /// The date.
    pub date: PartialDate,
    /// Assumptions made while parsing — an assumed timezone, or a chosen
    /// reading of an ambiguous `dd/mm` order.
    pub warnings: Vec<Warning>,
}

/// How two partial dates sit relative to each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateOrder {
    /// The whole of the left range ends before the right one starts.
    Before,
    /// The whole of the left range starts after the right one ends.
    After,
    /// The ranges share at least one instant, so no order can be asserted.
    /// `2019` and `2019-03-14` are always in this state.
    Overlapping,
}

/// Serialisation shape. Deserialisation routes back through
/// [`PartialDate::from_bounds`] so a hand-edited or hostile payload cannot
/// produce a date whose lower bound is after its upper bound.
#[derive(Serialize, Deserialize)]
struct PartialDateRepr {
    text: String,
    precision: DatePrecision,
    lower: DateTime<Utc>,
    upper: DateTime<Utc>,
}

impl TryFrom<PartialDateRepr> for PartialDate {
    type Error = DateError;
    fn try_from(r: PartialDateRepr) -> Result<Self, Self::Error> {
        Self::from_bounds(r.text, r.precision, r.lower, r.upper)
    }
}

/// A date known to some precision, held as the closed interval it denotes.
///
/// Constructed by [`PartialDate::parse`] or one of the `from_*` constructors.
/// The invariant `lower_bound() <= upper_bound()` holds for every value that
/// exists, because there is no way to build one that violates it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "PartialDateRepr", try_from = "PartialDateRepr")]
pub struct PartialDate {
    text: String,
    precision: DatePrecision,
    lower: DateTime<Utc>,
    upper: DateTime<Utc>,
}

impl From<PartialDate> for PartialDateRepr {
    fn from(d: PartialDate) -> Self {
        Self {
            text: d.text,
            precision: d.precision,
            lower: d.lower,
            upper: d.upper,
        }
    }
}

fn day_start(d: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&d.and_time(NaiveTime::MIN))
}

/// The upper bound of a day is its last nanosecond, so that the bound is
/// genuinely inclusive and no instant falls into a gap between two adjacent
/// precisions.
fn day_end(d: NaiveDate) -> DateTime<Utc> {
    let end = NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).unwrap_or(NaiveTime::MIN);
    Utc.from_utc_datetime(&d.and_time(end))
}

fn last_day_of_month(year: i32, month: u32) -> Option<NaiveDate> {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(ny, nm, 1)?.pred_opt()
}

fn check_year(year: i32) -> Result<(), DateError> {
    if (1..=9999).contains(&year) {
        Ok(())
    } else {
        Err(DateError::YearOutOfRange { year })
    }
}

impl PartialDate {
    /// A whole year.
    pub fn from_year(year: i32) -> Result<Self, DateError> {
        check_year(year)?;
        let lower =
            NaiveDate::from_ymd_opt(year, 1, 1).ok_or(DateError::YearOutOfRange { year })?;
        let upper =
            NaiveDate::from_ymd_opt(year, 12, 31).ok_or(DateError::YearOutOfRange { year })?;
        Ok(Self {
            text: format!("{year:04}"),
            precision: DatePrecision::Year,
            lower: day_start(lower),
            upper: day_end(upper),
        })
    }

    /// A whole calendar month.
    pub fn from_year_month(year: i32, month: u32) -> Result<Self, DateError> {
        check_year(year)?;
        if !(1..=12).contains(&month) {
            return Err(DateError::MonthOutOfRange { month });
        }
        let first =
            NaiveDate::from_ymd_opt(year, month, 1).ok_or(DateError::YearOutOfRange { year })?;
        let last = last_day_of_month(year, month).ok_or(DateError::MonthOutOfRange { month })?;
        Ok(Self {
            text: format!("{year:04}-{month:02}"),
            precision: DatePrecision::Month,
            lower: day_start(first),
            upper: day_end(last),
        })
    }

    /// A calendar day.
    ///
    /// Rejects days that do not exist — `2019-02-29` is an error, because
    /// there is no interval to store for a day that never happened.
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Result<Self, DateError> {
        check_year(year)?;
        if !(1..=12).contains(&month) {
            return Err(DateError::MonthOutOfRange { month });
        }
        let d = NaiveDate::from_ymd_opt(year, month, day).ok_or(DateError::DayOutOfRange {
            year,
            month,
            day,
        })?;
        Ok(Self {
            text: format!("{year:04}-{month:02}-{day:02}"),
            precision: DatePrecision::Day,
            lower: day_start(d),
            upper: day_end(d),
        })
    }

    /// A timestamp. Lower and upper bound are the same instant.
    pub fn from_instant(at: DateTime<Utc>) -> Result<Self, DateError> {
        check_year(at.year())?;
        Ok(Self {
            text: at.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
            precision: DatePrecision::Instant,
            lower: at,
            upper: at,
        })
    }

    /// Rebuild a date from stored columns (`value`, `date_prec`, `date_start`,
    /// `date_end`).
    ///
    /// The only constructor that takes bounds directly, and it enforces the
    /// ordering invariant rather than trusting the caller.
    pub fn from_bounds(
        text: impl Into<String>,
        precision: DatePrecision,
        lower: DateTime<Utc>,
        upper: DateTime<Utc>,
    ) -> Result<Self, DateError> {
        if lower > upper {
            return Err(DateError::BoundsInverted);
        }
        Ok(Self {
            text: text.into(),
            precision,
            lower,
            upper,
        })
    }

    /// Read a date from text.
    ///
    /// Accepts ISO-8601 dates and prefixes of them — `1978`, `1978-04`,
    /// `1978-04-12`, `1978-04-12T09:00:00Z` — and, because analysts type them,
    /// slash- and dot-separated dates such as `12/04/1978` and `12.04.1978`.
    /// `style` decides how to read a slash date whose first two components are
    /// both plausible months; the choice is reported as a warning.
    pub fn parse(input: &str, style: DateStyle) -> Result<ParsedDate, DateError> {
        let s = input.trim();
        if s.is_empty() {
            return Err(DateError::Empty);
        }
        if s.contains('/') || (s.contains('.') && !s.contains(':')) {
            return parse_separated(s, style);
        }
        parse_iso_like(s)
    }

    /// Read an ISO-8601 date or prefix, with no slash-date fallback.
    pub fn parse_iso(input: &str) -> Result<ParsedDate, DateError> {
        let s = input.trim();
        if s.is_empty() {
            return Err(DateError::Empty);
        }
        parse_iso_like(s)
    }

    /// The canonical text form: `1978`, `1978-04`, `1978-04-12` or an RFC-3339
    /// timestamp in UTC. This is what goes in `entity_prop.value_norm`.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// How much of the date was known.
    pub fn precision(&self) -> DatePrecision {
        self.precision
    }

    /// The first instant the date could denote, inclusive.
    pub fn lower_bound(&self) -> DateTime<Utc> {
        self.lower
    }

    /// The last instant the date could denote, inclusive.
    pub fn upper_bound(&self) -> DateTime<Utc> {
        self.upper
    }

    /// The lower bound as an RFC-3339 string, for `entity_prop.date_start`.
    pub fn lower_iso(&self) -> String {
        self.lower
            .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
    }

    /// The upper bound as an RFC-3339 string, for `entity_prop.date_end`.
    pub fn upper_iso(&self) -> String {
        self.upper
            .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
    }

    /// True when every instant `other` could denote is also an instant `self`
    /// could denote. `2019` contains `2019-03-14`; the reverse is false.
    pub fn contains(&self, other: &PartialDate) -> bool {
        self.lower <= other.lower && other.upper <= self.upper
    }

    /// True when the two ranges share at least one instant.
    pub fn overlaps(&self, other: &PartialDate) -> bool {
        self.lower <= other.upper && other.lower <= self.upper
    }

    /// Order the two ranges, or report that they overlap and cannot be
    /// ordered.
    ///
    /// This is the only comparison offered. There is no `PartialOrd`, because
    /// `<` on two dates that overlap would have to answer something, and every
    /// available answer is a lie.
    pub fn compare(&self, other: &PartialDate) -> DateOrder {
        if self.upper < other.lower {
            DateOrder::Before
        } else if self.lower > other.upper {
            DateOrder::After
        } else {
            DateOrder::Overlapping
        }
    }

    /// ISO prefixes of this date from year down to its own precision, coarsest
    /// first: `["2019", "2019-03", "2019-03-14"]`.
    ///
    /// This is what makes a year-precision birth date block against a
    /// day-precision one: the two share the `2019` key.
    pub fn iso_prefixes(&self) -> Vec<String> {
        let y = self.lower.year();
        let mut out = vec![format!("{y:04}")];
        if self.precision >= DatePrecision::Month {
            out.push(format!("{y:04}-{:02}", self.lower.month()));
        }
        if self.precision >= DatePrecision::Day {
            out.push(format!(
                "{y:04}-{:02}-{:02}",
                self.lower.month(),
                self.lower.day()
            ));
        }
        out
    }
}

impl std::fmt::Display for PartialDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// True if every byte is an ASCII digit and there is at least one.
fn all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn parse_year_component(s: &str, whole: &str) -> Result<i32, DateError> {
    if !all_digits(s) {
        return Err(DateError::Unrecognised {
            found: whole.to_string(),
        });
    }
    if s.len() == 2 {
        return Err(DateError::TwoDigitYear {
            found: s.to_string(),
        });
    }
    if s.len() != 4 {
        return Err(DateError::Unrecognised {
            found: whole.to_string(),
        });
    }
    s.parse::<i32>().map_err(|_| DateError::Unrecognised {
        found: whole.to_string(),
    })
}

fn parse_small(s: &str, whole: &str) -> Result<u32, DateError> {
    if !all_digits(s) || s.len() > 2 {
        return Err(DateError::Unrecognised {
            found: whole.to_string(),
        });
    }
    s.parse::<u32>().map_err(|_| DateError::Unrecognised {
        found: whole.to_string(),
    })
}

/// Split an ISO-ish value into its date part and optional time part.
fn split_datetime(s: &str) -> (&str, Option<&str>) {
    if let Some(idx) = s.find(['T', 't']) {
        return (&s[..idx], Some(&s[idx + 1..]));
    }
    if let Some(idx) = s.find(' ')
        && s[idx + 1..].contains(':')
    {
        return (&s[..idx], Some(&s[idx + 1..]));
    }
    (s, None)
}

fn parse_iso_like(s: &str) -> Result<ParsedDate, DateError> {
    let (date_part, time_part) = split_datetime(s);

    let comps: Vec<&str> = date_part.split('-').collect();
    if date_part.starts_with('-') || comps.iter().any(|c| c.is_empty()) {
        return Err(DateError::Unrecognised {
            found: s.to_string(),
        });
    }

    match (comps.as_slice(), time_part) {
        ([y], None) => {
            let year = parse_year_component(y, s)?;
            Ok(ParsedDate {
                date: PartialDate::from_year(year)?,
                warnings: Vec::new(),
            })
        }
        ([y, m], None) => {
            let year = parse_year_component(y, s)?;
            let month = parse_small(m, s)?;
            Ok(ParsedDate {
                date: PartialDate::from_year_month(year, month)?,
                warnings: Vec::new(),
            })
        }
        ([y, m, d], None) => {
            let year = parse_year_component(y, s)?;
            let month = parse_small(m, s)?;
            let day = parse_small(d, s)?;
            Ok(ParsedDate {
                date: PartialDate::from_ymd(year, month, day)?,
                warnings: Vec::new(),
            })
        }
        ([y, m, d], Some(time)) => {
            // Validate the calendar part first so that `2019-02-29T00:00:00Z`
            // reports the impossible day rather than a vague parse failure.
            let year = parse_year_component(y, s)?;
            let month = parse_small(m, s)?;
            let day = parse_small(d, s)?;
            let _ = PartialDate::from_ymd(year, month, day)?;
            parse_instant(year, month, day, time, s)
        }
        _ => Err(DateError::Unrecognised {
            found: s.to_string(),
        }),
    }
}

fn parse_instant(
    year: i32,
    month: u32,
    day: u32,
    time: &str,
    whole: &str,
) -> Result<ParsedDate, DateError> {
    let canonical = format!("{year:04}-{month:02}-{day:02}T{time}");

    if let Ok(dt) = DateTime::parse_from_rfc3339(&canonical) {
        return Ok(ParsedDate {
            date: PartialDate::from_instant(dt.with_timezone(&Utc))?,
            warnings: Vec::new(),
        });
    }

    // No offset given. Read it as UTC and say so.
    const NAIVE_FORMATS: [&str; 4] = [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%dT%H",
    ];
    for fmt in NAIVE_FORMATS {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&canonical, fmt) {
            return Ok(ParsedDate {
                date: PartialDate::from_instant(Utc.from_utc_datetime(&naive))?,
                warnings: vec![Warning::new(
                    WarningCode::TimezoneAssumed,
                    format!("`{whole}` has no timezone offset; read as UTC"),
                )],
            });
        }
    }

    Err(DateError::BadTime {
        found: time.to_string(),
    })
}

/// `12/04/1978`, `12.04.1978`, `1978/04/12`, `04/1978`.
fn parse_separated(s: &str, style: DateStyle) -> Result<ParsedDate, DateError> {
    let sep = if s.contains('/') { '/' } else { '.' };
    let parts: Vec<&str> = s.split(sep).map(str::trim).collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(DateError::Unrecognised {
            found: s.to_string(),
        });
    }

    match parts.as_slice() {
        // `04/1978` — a month.
        [a, b] if b.len() == 4 => {
            let year = parse_year_component(b, s)?;
            let month = parse_small(a, s)?;
            Ok(ParsedDate {
                date: PartialDate::from_year_month(year, month)?,
                warnings: Vec::new(),
            })
        }
        // `1978/04` — also a month.
        [a, b] if a.len() == 4 => {
            let year = parse_year_component(a, s)?;
            let month = parse_small(b, s)?;
            Ok(ParsedDate {
                date: PartialDate::from_year_month(year, month)?,
                warnings: Vec::new(),
            })
        }
        // `1978/04/12` — unambiguous, year first.
        [a, b, c] if a.len() == 4 => {
            let year = parse_year_component(a, s)?;
            let month = parse_small(b, s)?;
            let day = parse_small(c, s)?;
            Ok(ParsedDate {
                date: PartialDate::from_ymd(year, month, day)?,
                warnings: Vec::new(),
            })
        }
        [a, b, c] => {
            let year = parse_year_component(c, s)?;
            let first = parse_small(a, s)?;
            let second = parse_small(b, s)?;

            // If exactly one of the two can be a month, there is nothing to
            // guess.
            let (day, month, ambiguous) = match (first <= 12, second <= 12) {
                (true, true) => match style {
                    DateStyle::DayFirst => (first, second, true),
                    DateStyle::MonthFirst => (second, first, true),
                },
                (false, true) => (first, second, false),
                (true, false) => (second, first, false),
                (false, false) => {
                    return Err(DateError::MonthOutOfRange { month: second });
                }
            };

            let date = PartialDate::from_ymd(year, month, day)?;
            let warnings = if ambiguous {
                let order = match style {
                    DateStyle::DayFirst => "day first",
                    DateStyle::MonthFirst => "month first",
                };
                vec![Warning::new(
                    WarningCode::AmbiguousDateOrder,
                    format!(
                        "`{s}` could be read either way round; read as {order} \
                         (from the case region), giving {date}"
                    ),
                )]
            } else {
                Vec::new()
            };
            Ok(ParsedDate { date, warnings })
        }
        _ => Err(DateError::Unrecognised {
            found: s.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> PartialDate {
        PartialDate::parse(s, DateStyle::DayFirst)
            .expect("should parse")
            .date
    }

    #[test]
    fn iso_prefixes_carry_precision() {
        assert_eq!(ok("2019").precision(), DatePrecision::Year);
        assert_eq!(ok("2019-03").precision(), DatePrecision::Month);
        assert_eq!(ok("2019-03-14").precision(), DatePrecision::Day);
        assert_eq!(
            ok("1978-04-12T09:00:00Z").precision(),
            DatePrecision::Instant
        );
    }

    #[test]
    fn a_year_is_never_widened_to_january_first() {
        let y = ok("2019");
        assert_eq!(y.lower_iso(), "2019-01-01T00:00:00Z");
        assert!(y.upper_iso().starts_with("2019-12-31T23:59:59"));
        assert_eq!(y.text(), "2019");
    }

    #[test]
    fn bounds_never_invert() {
        for s in ["2019", "2019-02", "2020-02-29", "1978-04-12T09:00:00Z"] {
            let d = ok(s);
            assert!(d.lower_bound() <= d.upper_bound(), "{s}");
        }
    }

    #[test]
    fn impossible_dates_are_errors() {
        assert!(matches!(
            PartialDate::parse("2019-02-29", DateStyle::DayFirst),
            Err(DateError::DayOutOfRange { .. })
        ));
        assert!(matches!(
            PartialDate::parse("2019-13", DateStyle::DayFirst),
            Err(DateError::MonthOutOfRange { month: 13 })
        ));
        assert!(matches!(
            PartialDate::parse("78", DateStyle::DayFirst),
            Err(DateError::TwoDigitYear { .. })
        ));
    }

    #[test]
    fn overlapping_dates_cannot_be_ordered() {
        let year = ok("2019");
        let day = ok("2019-03-14");
        assert_eq!(year.compare(&day), DateOrder::Overlapping);
        assert!(year.contains(&day));
        assert!(!day.contains(&year));
        assert_eq!(ok("2018").compare(&year), DateOrder::Before);
        assert_eq!(ok("2020").compare(&year), DateOrder::After);
    }

    #[test]
    fn serde_round_trip_keeps_bounds() {
        let d = ok("2019-03");
        let json = serde_json::to_string(&d).expect("serialise");
        let back: PartialDate = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(d, back);
    }

    #[test]
    fn deserialising_inverted_bounds_fails() {
        let json = r#"{"text":"2019","precision":"year",
            "lower":"2019-12-31T00:00:00Z","upper":"2019-01-01T00:00:00Z"}"#;
        assert!(serde_json::from_str::<PartialDate>(json).is_err());
    }
}
