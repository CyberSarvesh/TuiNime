//! Services — background IO that `App` spawns. No widget code.
//!
//! Every network call runs on its own task and reports back through one channel, so the
//! render loop never waits on the network (it keeps drawing "searching…" etc. meanwhile).

use tokio::sync::mpsc::UnboundedSender;

use crate::backend::{Anime, Catalog, StreamSource};

/// Results delivered from background tasks to `App::on_background`.
pub enum BgEvent {
    Trending(Vec<Anime>),
    Seasonal { label: String, result: Result<Vec<Anime>, String> },
    Search { seq: u64, query: String, result: Result<Vec<Anime>, String> },
    Resolved { anime: Anime, result: Result<Vec<StreamSource>, String> },
}

pub fn spawn_trending(catalog: Catalog, tx: UnboundedSender<BgEvent>) {
    tokio::spawn(async move {
        let list = catalog.trending(20).await;
        let _ = tx.send(BgEvent::Trending(list));
    });
}

pub fn spawn_seasonal(catalog: Catalog, tx: UnboundedSender<BgEvent>) {
    let (year, season) = current_season();
    tokio::spawn(async move {
        let result = catalog.seasonal(year, season, 20).await.map_err(|e| format!("{e:#}"));
        let label = format!("{} {year}", season.to_lowercase());
        let _ = tx.send(BgEvent::Seasonal { label, result });
    });
}

pub fn spawn_search(catalog: Catalog, tx: UnboundedSender<BgEvent>, seq: u64, query: String) {
    tokio::spawn(async move {
        let result = catalog.search(&query, 20).await.map_err(|e| format!("{e:#}"));
        let _ = tx.send(BgEvent::Search { seq, query, result });
    });
}

pub fn spawn_resolve(catalog: Catalog, tx: UnboundedSender<BgEvent>, anime: Anime) {
    tokio::spawn(async move {
        let result = catalog.resolve(&anime).await.map_err(|e| format!("{e:#}"));
        let _ = tx.send(BgEvent::Resolved { anime, result });
    });
}

/// AniList season for the current UTC date.
fn current_season() -> (u16, &'static str) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (year, month) = year_month_from_days((secs / 86_400) as i64);
    (year, season_for_month(month))
}

/// AniList seasons: WINTER = Jan–Mar, SPRING = Apr–Jun, SUMMER = Jul–Sep, FALL = Oct–Dec.
fn season_for_month(month: u32) -> &'static str {
    match month {
        1..=3 => "WINTER",
        4..=6 => "SPRING",
        7..=9 => "SUMMER",
        _ => "FALL",
    }
}

/// Days since 1970-01-01 → (year, month). Howard Hinnant's `civil_from_days`.
fn year_month_from_days(days: i64) -> (u16, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year as u16, month)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_date_conversion() {
        assert_eq!(year_month_from_days(0), (1970, 1));
        // 2026-11-16 is day 20_773
        assert_eq!(year_month_from_days(20_773), (2026, 11));
        // 2024-02-29 (leap day) is day 19_782
        assert_eq!(year_month_from_days(19_782), (2024, 2));
    }

    #[test]
    fn seasons() {
        assert_eq!(season_for_month(1), "WINTER");
        assert_eq!(season_for_month(6), "SPRING");
        assert_eq!(season_for_month(9), "SUMMER");
        assert_eq!(season_for_month(11), "FALL");
    }
}
