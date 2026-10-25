//! What TelosWeaver means by an endpoint, a receipt and their times (Spec v1.5 §12.2, PH0-RN-05). The contract types
//! are `telos_schema`'s; this module maps this engine's sessions and clock onto them.
//!
//! - An endpoint is a session by the id its agent gave it, never by its address: an address is a name a later session
//!   can take. Its fingerprint hashes the host, the provider and that id (not its folder: a session that moves to
//!   another folder is the same conversation). Sessions known only by their process (`proc:<pid>`) are no endpoint.
//! - A receipt says `delivered` only once the recipient was shown the message (TelosWeaver decision, 2026-10-25):
//!   handing it to an agent's own inbox or queue leaves it `queued`, with how it was handed off as its reason.

use crate::util;
pub use telos_schema::{DeliveryReceiptStatus as Status, rn::EndpointRef};

/// The longest a message may wait for its recipient: an `expiresAt` further out is refused, and a message sent without
/// one expires after this.
pub const MAX_TTL_MS: i64 = 7 * 86_400_000;
/// How long a message, its receipt and its idempotency key are kept once its receipt is final.
pub const RETAIN_MS: i64 = 7 * 86_400_000;
/// Database copies kept in `backups/` from before a schema migration.
pub const BACKUPS_KEPT: usize = 3;

const FINGERPRINT_CONTEXT: &str = "telos-rn endpoint fingerprint v1";

/// This host as TelosWeaver names it: `TELOS_RN_HOST_ID` (which Core sets when it starts the engine), else the
/// machine's host name.
pub fn host_id() -> String {
    crate::paths::env_var_os("HOST_ID")
        .and_then(|v| v.into_string().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(util::hostname)
}

/// The provider id TelosWeaver uses for one of this engine's agents (Core's ids where it has one).
pub fn provider(agent: &str) -> &str {
    match agent {
        crate::agent::CLAUDE_CODE => "claude",
        other => other,
    }
}

/// The agents whose sessions report as `provider`.
fn agents_for(provider: &str) -> Vec<&str> {
    match provider {
        "claude" => vec![crate::agent::CLAUDE_CODE],
        // Core's id for Claude Code is `claude`: the engine's own name for it is not a second provider.
        crate::agent::CLAUDE_CODE => Vec::new(),
        other => vec![other],
    }
}

pub fn fingerprint(host_id: &str, provider: &str, native_session_id: &str) -> String {
    let mut h = blake3::Hasher::new_derive_key(FINGERPRINT_CONTEXT);
    for part in [host_id, provider, native_session_id] {
        h.update(&(part.len() as u64).to_le_bytes());
        h.update(part.as_bytes());
    }
    h.finalize().to_hex().to_string()
}

/// The endpoint a session key (`<agent>:<its id>`) stands for on `host_id`; none for one known only by its process.
pub fn endpoint(host_id: &str, key: &str) -> Option<EndpointRef> {
    let (agent, native) = key.split_once(':')?;
    if agent == "proc" || agent.is_empty() || native.is_empty() {
        return None;
    }
    let provider = provider(agent).to_string();
    Some(EndpointRef {
        fingerprint: fingerprint(host_id, &provider, native),
        host_id: host_id.to_string(),
        provider,
        native_session_id: native.to_string(),
    })
}

/// The session keys an endpoint could be here.
pub fn keys(e: &EndpointRef) -> Vec<String> {
    agents_for(&e.provider).into_iter().map(|a| format!("{a}:{}", e.native_session_id)).collect()
}

pub fn status_str(s: Status) -> &'static str {
    match s {
        Status::Accepted => "accepted",
        Status::Queued => "queued",
        Status::Delivered => "delivered",
        Status::Held => "held",
        Status::Refused => "refused",
        Status::Expired => "expired",
        Status::Failed => "failed",
    }
}

pub fn parse_status(s: &str) -> Option<Status> {
    Some(match s {
        "accepted" => Status::Accepted,
        "queued" => Status::Queued,
        "delivered" => Status::Delivered,
        "held" => Status::Held,
        "refused" => Status::Refused,
        "expired" => Status::Expired,
        "failed" => Status::Failed,
        _ => return None,
    })
}

/// `ms` since the epoch as RFC 3339 UTC with milliseconds.
pub fn rfc3339(ms: i64) -> String {
    let (secs, milli) = (ms.div_euclid(1000), ms.rem_euclid(1000));
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{milli:03}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// An RFC 3339 time (`2026-10-25T08:00:00Z`, with optional fraction and a `Z` or `±hh:mm` offset) as ms since the
/// epoch.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let part = s.get(from..from + len)?;
        part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse().ok())?
    };
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (y, mo, d, h, mi, sec) = (num(0, 4)?, num(5, 2)?, num(8, 2)?, num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&mo) || d < 1 || d > days_in(y, mo) || h > 23 || mi > 59 || sec > 59 {
        return None;
    }
    let mut i = 19;
    let mut milli = 0;
    if b.get(i) == Some(&b'.') {
        let digits = b[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        let frac = &s[i + 1..i + 1 + digits.min(3)];
        milli = format!("{frac:0<3}").parse().ok()?;
        i += 1 + digits;
    }
    let offset = match b.get(i..)? {
        [b'Z' | b'z'] => 0,
        [sign @ (b'+' | b'-'), _, _, b':', _, _] => {
            let (oh, om) = (num(i + 1, 2)?, num(i + 4, 2)?);
            if oh > 23 || om > 59 {
                return None;
            }
            let o = (oh * 60 + om) * 60_000;
            if *sign == b'+' { o } else { -o }
        }
        _ => return None,
    };
    let days = days_from_civil(y, mo, d);
    Some((days * 86_400 + h * 3600 + mi * 60 + sec) * 1000 + milli - offset)
}

fn days_in(y: i64, m: i64) -> i64 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 to the civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_endpoint_is_its_agents_session_id_on_this_host_and_its_folder_is_no_part_of_it() {
        let e = endpoint("pc-1", "claude-code:4f1c").unwrap();
        assert_eq!((e.host_id.as_str(), e.provider.as_str(), e.native_session_id.as_str()), ("pc-1", "claude", "4f1c"));
        assert_eq!(e.fingerprint, fingerprint("pc-1", "claude", "4f1c"));
        // Another host, provider or id is another endpoint; the fields can't be shifted into each other.
        let others = [
            fingerprint("pc-2", "claude", "4f1c"),
            fingerprint("pc-1", "codex", "4f1c"),
            fingerprint("pc-1", "claude", "4f1d"),
            fingerprint("pc-1c", "laude", "4f1c"),
        ];
        assert!(others.iter().all(|f| *f != e.fingerprint));
        assert_eq!(keys(&e), ["claude-code:4f1c"]);
        assert_eq!(endpoint("pc-1", "cli:terminal").unwrap().provider, "cli");
        // A session known only by its process could be any later process: it is no endpoint.
        assert!(endpoint("pc-1", "proc:4312").is_none());
        assert!(endpoint("pc-1", "codex:").is_none());
        // The engine's own name for Claude Code isn't a second way to reach the same session.
        let alias = EndpointRef { provider: "claude-code".into(), ..e };
        assert!(keys(&alias).is_empty());
    }

    #[test]
    fn times_go_both_ways_and_odd_ones_are_refused() {
        let t = parse_rfc3339("2026-10-25T08:30:00Z").unwrap();
        assert_eq!(rfc3339(t), "2026-10-25T08:30:00.000Z");
        assert_eq!(parse_rfc3339("2026-10-25T17:30:00.5+09:00"), Some(t + 500));
        assert_eq!(parse_rfc3339("2026-10-25T08:30:00.123456Z"), Some(t + 123));
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2024-02-29T00:00:00Z").map(rfc3339).as_deref(), Some("2024-02-29T00:00:00.000Z"));
        for bad in [
            "2026-10-25 08:30:00Z",
            "2026-10-25T08:30:00",
            "2025-02-29T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-10-25T08:30:00.Z",
            "2026-10-25T08:30:00+9:00",
            "+2026-10-25T08:30:00Z",
            "2026-10-25T08:30:60Z",
        ] {
            assert_eq!(parse_rfc3339(bad), None, "{bad}");
        }
        let now = util::now_ms();
        assert_eq!(parse_rfc3339(&rfc3339(now)), Some(now));
    }

    #[test]
    fn receipt_states_are_the_contracts_strings() {
        for s in [
            Status::Accepted,
            Status::Queued,
            Status::Held,
            Status::Delivered,
            Status::Refused,
            Status::Expired,
            Status::Failed,
        ] {
            assert_eq!(parse_status(status_str(s)), Some(s));
            assert_eq!(serde_json::to_value(s).unwrap(), status_str(s));
        }
        assert_eq!(parse_status("pushed"), None);
    }
}
