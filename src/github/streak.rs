use crate::github::types::ContributionDay;
use chrono::{Duration, NaiveDate};

/// The "active recently" rule: contributed on `as_of` or the day before.
/// One definition — `calculate_current_streak` and `is_active` both express it
/// through this helper.
pub(crate) fn has_recent_contribution(days: &[ContributionDay], as_of: NaiveDate) -> bool {
    let today = as_of.format("%Y-%m-%d").to_string();
    let yesterday = (as_of - Duration::days(1)).format("%Y-%m-%d").to_string();

    days.iter().rev().any(|d| {
        (d.date == today || d.date == yesterday) && d.contribution_count > 0
    })
}

pub(crate) fn calculate_current_streak(days: &[ContributionDay], as_of: NaiveDate) -> usize {
    if days.is_empty() || !has_recent_contribution(days, as_of) {
        return 0;
    }

    let today = as_of.format("%Y-%m-%d").to_string();

    let mut streak = 0;
    let mut skipped_today = false;

    for day in days.iter().rev() {
        let day_date = match NaiveDate::parse_from_str(&day.date, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => continue,
        };

        if day_date > as_of {
            continue;
        }

        // A zero-contribution today doesn't break a streak that ends yesterday.
        if day.date == today && day.contribution_count == 0 && !skipped_today {
            skipped_today = true;
            continue;
        }

        if day.contribution_count > 0 {
            streak += 1;
        } else {
            break;
        }
    }

    streak
}

pub(crate) fn calculate_highest_streak(days: &[ContributionDay]) -> usize {
    let mut highest = 0;
    let mut current = 0;

    for day in days {
        if day.contribution_count > 0 {
            current += 1;
            if current > highest {
                highest = current;
            }
        } else {
            current = 0;
        }
    }

    highest
}

pub(crate) fn is_active(days: &[ContributionDay], as_of: NaiveDate) -> bool {
    has_recent_contribution(days, as_of)
}
