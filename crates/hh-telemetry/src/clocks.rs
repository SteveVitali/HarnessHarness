//! Units and clocks (§5h.1 §2.6; ADR-0043 D2): `ts` orders nothing (ordering is
//! `seq`); durations are integer ms from the *measuring component's* monotonic
//! clock, stamped `measured_at`; `clock_skew_flag` fires when a payload duration
//! and the `ts` difference disagree beyond `clock_tolerance_ms`.

use crate::errors::TelemetryError;

/// `measured_at ∈ {adapter, external_gateway, interceptor, executor, runtime,
/// participant}` — which component's monotonic clock produced a `*_ms`
/// measurement (§5h.1 §2.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MeasuredAt {
    /// The model/gateway adapter.
    Adapter,
    /// An external gateway.
    ExternalGateway,
    /// An interceptor (hosted/grey-box path).
    Interceptor,
    /// The tool/effect executor.
    Executor,
    /// The runtime (kernel-side measurement).
    Runtime,
    /// The participant (self-reported — `participant_reported` provenance).
    Participant,
}

impl MeasuredAt {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MeasuredAt::Adapter => "adapter",
            MeasuredAt::ExternalGateway => "external_gateway",
            MeasuredAt::Interceptor => "interceptor",
            MeasuredAt::Executor => "executor",
            MeasuredAt::Runtime => "runtime",
            MeasuredAt::Participant => "participant",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Result<MeasuredAt, TelemetryError> {
        Ok(match s {
            "adapter" => MeasuredAt::Adapter,
            "external_gateway" => MeasuredAt::ExternalGateway,
            "interceptor" => MeasuredAt::Interceptor,
            "executor" => MeasuredAt::Executor,
            "runtime" => MeasuredAt::Runtime,
            "participant" => MeasuredAt::Participant,
            _ => {
                return Err(TelemetryError::UnknownMeasuredAt {
                    spelling: s.to_string(),
                })
            }
        })
    }
}

/// The default `clock_tolerance_ms` — the manifest constant's Stage-1 value
/// (§5h.1 §2.6). A run manifest may override it via `extra["clock_tolerance_ms"]`;
/// [`clock_tolerance`] is the one read path.
pub const DEFAULT_CLOCK_TOLERANCE_MS: i64 = 5;

/// The manifest's `clock_tolerance_ms` — `extra["clock_tolerance_ms"]` when the
/// manifest declares one, else [`DEFAULT_CLOCK_TOLERANCE_MS`].
pub fn clock_tolerance(manifest: &hh_ledger::manifest::RunManifest) -> i64 {
    manifest
        .extra
        .get("clock_tolerance_ms")
        .and_then(hh_wire::json::Json::as_int)
        .unwrap_or(DEFAULT_CLOCK_TOLERANCE_MS)
}

/// `|payload_ms − ts_span_ms| > tolerance` — the skew predicate (§5h.1 §2.6).
/// `ts_span_ms` is signed: a stepped-back wall clock yields a negative span,
/// which any sane payload duration disagrees with.
pub fn skewed(payload_ms: i64, ts_span_ms: i64, tolerance_ms: i64) -> bool {
    (payload_ms - ts_span_ms).abs() > tolerance_ms
}

/// Parse the ledger's canonical `ts` (`YYYY-MM-DDTHH:MM:SS.mmmZ`, RFC 3339 UTC,
/// millisecond precision) into ms since the epoch. `None` on any deviation —
/// a malformed `ts` never yields a guessed instant.
pub fn ts_ms(ts: &str) -> Option<i64> {
    // Fixed canonical shape: 24 bytes — "YYYY-MM-DDTHH:MM:SS.mmmZ".
    let b = ts.as_bytes();
    if b.len() != 24
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'.'
        || b[23] != b'Z'
    {
        return None;
    }
    let num = |lo: usize, hi: usize| -> Option<i64> {
        let s = std::str::from_utf8(&b[lo..hi]).ok()?;
        if !s.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        s.parse::<i64>().ok()
    };
    let year = num(0, 4)?;
    let month = num(5, 7)?;
    let day = num(8, 10)?;
    let hour = num(11, 13)?;
    let min = num(14, 16)?;
    let sec = num(17, 19)?;
    let ms = num(20, 23)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || min > 59
        || sec > 59
        || ms > 999
    {
        return None;
    }
    // Days since epoch — civil-from-days (Howard Hinnant's algorithm, the
    // standard branch-free civil conversion).
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (month + 9) % 12; // [0, 11]
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some((((days * 24 + hour) * 60 + min) * 60 + sec) * 1000 + ms)
}

/// The `ts` difference `end − start` in ms, or `None` when either fails to
/// parse. Signed — a stepped wall clock yields a negative span.
pub fn ts_span_ms(start_ts: &str, end_ts: &str) -> Option<i64> {
    Some(ts_ms(end_ts)? - ts_ms(start_ts)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_at_spellings_round_trip() {
        for (m, s) in [
            (MeasuredAt::Adapter, "adapter"),
            (MeasuredAt::ExternalGateway, "external_gateway"),
            (MeasuredAt::Interceptor, "interceptor"),
            (MeasuredAt::Executor, "executor"),
            (MeasuredAt::Runtime, "runtime"),
            (MeasuredAt::Participant, "participant"),
        ] {
            assert_eq!(m.as_str(), s);
            assert_eq!(MeasuredAt::parse(s).unwrap(), m);
        }
        assert!(MeasuredAt::parse("bogus").is_err());
    }

    #[test]
    fn ts_ms_parses_the_canonical_form() {
        assert_eq!(ts_ms("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(ts_ms("1970-01-01T00:00:01.250Z"), Some(1250));
        // 2026-09-17T12:00:00.000Z — sanity-checked against a known offset.
        let t = ts_ms("2026-09-17T12:00:00.000Z").unwrap();
        assert_eq!(t % 86_400_000, 43_200_000); // noon UTC
        assert!(t > 1_700_000_000_000 && t < 1_900_000_000_000);
        for bad in [
            "2026-09-17 12:00:00.000Z",
            "2026-09-17T12:00:00Z",
            "not-a-ts",
            "2026-13-17T12:00:00.000Z",
        ] {
            assert_eq!(ts_ms(bad), None, "{bad}");
        }
    }

    #[test]
    fn skewed_is_the_tolerance_predicate() {
        assert!(!skewed(100, 104, 5));
        assert!(skewed(100, 106, 5));
        // A stepped-back wall clock: negative ts span, sane payload duration.
        assert!(skewed(250, -500, 5));
    }
}
