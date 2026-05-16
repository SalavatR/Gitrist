//! Time formatting (absolute RFC3339 + relative "3h ago") and a small
//! `sleep_ms` helper that maps to `gloo_timers` in wasm and a never-
//! ending pending future on native (the auto-refresh loops only matter
//! in the browser).

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub(crate) fn format_time(unix: i64) -> String {
    OffsetDateTime::from_unix_timestamp(unix)
        .ok()
        .and_then(|dt| dt.format(&Rfc3339).ok())
        .unwrap_or_else(|| unix.to_string())
}

pub(crate) fn format_time_relative(unix: i64) -> String {
    let now = now_unix();
    let delta = now - unix;
    if delta < 0 {
        return "in the future".to_string();
    }
    if delta < 60 {
        return "just now".to_string();
    }
    if delta < 3600 {
        return format!("{}m ago", delta / 60);
    }
    if delta < 86400 {
        return format!("{}h ago", delta / 3600);
    }
    let days = delta / 86400;
    if days < 7 {
        return format!("{}d ago", days);
    }
    if days < 30 {
        return format!("{}w ago", days / 7);
    }
    if days < 365 {
        return format!("{}mo ago", days / 30);
    }
    format!("{}y ago", days / 365)
}

#[cfg(target_arch = "wasm32")]
fn now_unix() -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}

#[cfg(not(target_arch = "wasm32"))]
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn sleep_ms(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn sleep_ms(_ms: u32) {
    // No-op on native — auto-refresh only matters in the browser.
    std::future::pending::<()>().await
}
