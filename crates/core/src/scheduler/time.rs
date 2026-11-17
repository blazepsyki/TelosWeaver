//! RFC 3339 UTC 초 단위 시각과 Unix 초 사이의 변환입니다. 기한 비교에 씁니다.

use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

pub(super) fn format(seconds: u64) -> String {
    let (days, rem) = ((seconds / 86_400) as i64, seconds % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// `YYYY-MM-DDTHH:MM:SS`로 시작하는 UTC 시각을 읽습니다. 소수 초와 `Z` 밖의 Offset은 받지 않습니다.
pub(super) fn parse(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let tail = &text[19..];
    if !(tail == "Z" || (tail.starts_with('.') && tail.ends_with('Z'))) {
        return None;
    }
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + hour * 3_600 + minute * 60 + second).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_and_parse_round_trip() {
        for seconds in [0, 86_399, 951_782_400, 1_794_787_200, 4_102_444_799] {
            assert_eq!(parse(&format(seconds)), Some(seconds), "{seconds}");
        }
        assert_eq!(format(1_794_787_200), "2026-11-16T00:00:00Z");
        assert_eq!(parse("2026-11-16T00:00:00.250Z"), Some(1_794_787_200));
    }

    #[test]
    fn other_formats_are_not_guessed() {
        assert_eq!(parse("2026-11-16"), None);
        assert_eq!(parse("2026-11-16T00:00:00+09:00"), None);
        assert_eq!(parse("2026-13-16T00:00:00Z"), None);
    }
}
