use crate::github::types::ContributionDay;
use chrono::{Duration, NaiveDate, Utc};

pub(crate) fn calculate_current_streak(days: &[ContributionDay]) -> usize {
    if days.is_empty() {
        return 0;
    }

    let now = Utc::now();
    let today = now.format("%Y-%m-%d").to_string();
    let yesterday = (now - Duration::days(1)).format("%Y-%m-%d").to_string();

    let has_recent = days.iter().rev().any(|d| {
        (d.date == today || d.date == yesterday) && d.contribution_count > 0
    });

    if !has_recent {
        return 0;
    }

    let mut streak = 0;
    let mut skipped_today = false;

    for day in days.iter().rev() {
        let day_date = match NaiveDate::parse_from_str(&day.date, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => continue,
        };

        if day_date > now.naive_utc().date() {
            continue;
        }

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

pub(crate) fn is_active(days: &[ContributionDay]) -> bool {
    let now = Utc::now();
    let today = now.format("%Y-%m-%d").to_string();
    let yesterday = (now - Duration::days(1)).format("%Y-%m-%d").to_string();

    days.iter().rev().any(|d| {
        (d.date == today || d.date == yesterday) && d.contribution_count > 0
    })
}
