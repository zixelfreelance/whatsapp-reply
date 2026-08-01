//! Rate governor — the behavioural pacing that keeps the account alive.
//!
//! WhatsApp's ban signals are overwhelmingly behavioural: volume, reply speed,
//! unsolicited first contact, and recipient reports. Not input-mechanism
//! fingerprints. This module is therefore the highest-leverage safety code in
//! the system, and every send passes through it.

use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone)]
pub struct RateLimits {
    /// Hard ceiling on sends in any rolling hour.
    pub max_per_hour: usize,
    /// Minimum spacing between two consecutive sends.
    pub min_gap: Duration,
    /// Upper bound on the random padding added to every delay, so timing is
    /// never metronomic.
    pub max_jitter: Duration,
    /// Floor on how soon after arrival a reply may go out.
    pub min_age: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Send,
    Hold { until: DateTime<Utc> },
}

/// Source of the random padding. Injected so tests are deterministic — a
/// governor that reached for a global RNG could not be tested honestly.
pub trait Jitter {
    fn padding(&self, max: Duration) -> Duration;
}

/// No padding at all. Useful in tests; **not** what production should run,
/// because a perfectly regular cadence is itself a signature.
pub struct NoJitter;

impl Jitter for NoJitter {
    fn padding(&self, _max: Duration) -> Duration {
        Duration::zero()
    }
}

/// Uniform padding in `[0, max]`. This is the production source.
pub struct RandomJitter;

impl Jitter for RandomJitter {
    fn padding(&self, max: Duration) -> Duration {
        use rand::Rng;
        let seconds = max.num_seconds().max(0);
        if seconds == 0 {
            return Duration::zero();
        }
        Duration::seconds(rand::thread_rng().gen_range(0..=seconds))
    }
}

pub struct Governor<J: Jitter> {
    limits: RateLimits,
    jitter: J,
    sends: Vec<DateTime<Utc>>,
}

impl<J: Jitter> Governor<J> {
    pub fn new(limits: RateLimits, jitter: J) -> Self {
        Self {
            limits,
            jitter,
            sends: Vec::new(),
        }
    }

    /// Record that a reply actually went out. Call this only on success — a
    /// failed send consumed no quota.
    ///
    /// Prunes anything older than the rolling window on the way through: this
    /// process runs for weeks, and only the last hour can affect a decision.
    pub fn record_send(&mut self, at: DateTime<Utc>) {
        self.sends.push(at);
        let cutoff = at - Duration::hours(1);
        self.sends.retain(|sent| *sent > cutoff);
    }

    /// How many sends are still inside the rolling window. Exposed so the
    /// memory bound is observable.
    pub fn tracked_sends(&self) -> usize {
        self.sends.len()
    }

    /// Decide whether a reply to a message that arrived at `arrived_at` may be
    /// sent at `now`.
    ///
    /// Every constraint is evaluated and the latest release time wins, so a
    /// message blocked by two rules is not released early by the looser one.
    pub fn decide(&self, arrived_at: DateTime<Utc>, now: DateTime<Utc>) -> Decision {
        let mut release = arrived_at + self.limits.min_age;

        if let Some(last) = self.sends.iter().max() {
            let after_gap = *last + self.limits.min_gap + self.jitter.padding(self.limits.max_jitter);
            release = release.max(after_gap);
        }

        // Rolling window, not calendar hour: count what is still inside the
        // last hour and, if we are at the ceiling, wait for the oldest of
        // those to age out.
        let window_start = now - Duration::hours(1);
        let mut in_window: Vec<_> = self
            .sends
            .iter()
            .filter(|sent| **sent > window_start)
            .copied()
            .collect();

        if in_window.len() >= self.limits.max_per_hour {
            in_window.sort();
            let oldest = in_window[in_window.len() - self.limits.max_per_hour];
            release = release.max(oldest + Duration::hours(1));
        }

        if now < release {
            Decision::Hold { until: release }
        } else {
            Decision::Send
        }
    }
}

/// Fixed padding, for tests that need a deterministic non-zero jitter.
#[cfg(test)]
pub struct FixedJitter(pub Duration);

#[cfg(test)]
impl Jitter for FixedJitter {
    fn padding(&self, _max: Duration) -> Duration {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, TimeZone, Utc};

    /// A time `secs` after an arbitrary fixed epoch. Nothing here reads the
    /// wall clock — `decide` takes `now` explicitly, same as the queue threads
    /// arrival time rather than stamping it.
    fn t(secs: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, 3, 0, 0).unwrap() + Duration::seconds(secs)
    }

    fn limits() -> RateLimits {
        RateLimits {
            max_per_hour: 10,
            min_gap: Duration::seconds(90),
            max_jitter: Duration::seconds(45),
            min_age: Duration::seconds(60),
        }
    }

    #[test]
    fn a_message_that_just_arrived_is_held_until_it_has_aged() {
        // Instant replies are the clearest bot signal there is — nobody
        // answers in 900ms at 4am.
        let g = Governor::new(limits(), NoJitter);

        let decision = g.decide(t(0), t(10));

        assert_eq!(decision, Decision::Hold { until: t(60) });
    }

    #[test]
    fn an_aged_message_sends_when_nothing_has_gone_out_recently() {
        let g = Governor::new(limits(), NoJitter);

        assert_eq!(g.decide(t(0), t(60)), Decision::Send);
    }

    #[test]
    fn a_second_reply_waits_for_the_minimum_gap_after_the_last_send() {
        // Two replies back to back read as a machine even when each one is
        // individually well-timed.
        let mut g = Governor::new(limits(), NoJitter);
        g.record_send(t(100));

        // Message is old enough, but the previous send was only 10s ago.
        let decision = g.decide(t(0), t(110));

        assert_eq!(decision, Decision::Hold { until: t(190) });
    }

    #[test]
    fn jitter_pads_the_hold_so_timing_is_never_metronomic() {
        // A governor that always released at exactly min_gap would produce a
        // perfectly regular cadence — itself a signature.
        let mut g = Governor::new(limits(), FixedJitter(Duration::seconds(30)));
        g.record_send(t(100));

        let decision = g.decide(t(0), t(110));

        assert_eq!(decision, Decision::Hold { until: t(220) });
    }

    #[test]
    fn the_hourly_ceiling_holds_until_the_oldest_send_ages_out() {
        // The backlog case: laptop asleep overnight, 40 messages queued. This
        // is what stops them draining as a burst.
        let mut g = Governor::new(
            RateLimits {
                max_per_hour: 3,
                ..limits()
            },
            NoJitter,
        );
        g.record_send(t(0));
        g.record_send(t(100));
        g.record_send(t(200));

        // Well past min_age and min_gap, but three sends already happened
        // inside the rolling hour. The oldest ages out at t(0)+3600.
        let decision = g.decide(t(0), t(400));

        assert_eq!(decision, Decision::Hold { until: t(3600) });
    }

    #[test]
    fn sends_that_have_aged_out_of_the_hour_no_longer_count() {
        let mut g = Governor::new(
            RateLimits {
                max_per_hour: 1,
                ..limits()
            },
            NoJitter,
        );
        g.record_send(t(0));

        assert_eq!(g.decide(t(0), t(3600)), Decision::Send);
    }

    #[test]
    fn an_overnight_backlog_drains_over_hours_instead_of_bursting() {
        // The scenario the governor exists for: laptop asleep eight hours,
        // forty messages queued, lid opens. Approving them all must not
        // produce forty sends in two minutes — the single most machine-shaped
        // pattern the system could emit.
        let mut g = Governor::new(limits(), NoJitter);

        // Forty messages that all arrived overnight, so every one is well
        // past min_age by the time we start.
        let backlog: Vec<_> = (0..40).map(|i| t(-30_000 + i * 60)).collect();
        let mut sent_at = Vec::new();
        let mut pending = backlog.into_iter().peekable();

        // Walk a simulated clock forward one minute at a time for six hours,
        // sending whenever the governor allows it.
        for minute in 0..360 {
            let now = t(minute * 60);
            let Some(arrived) = pending.peek().copied() else {
                break;
            };
            if g.decide(arrived, now) == Decision::Send {
                g.record_send(now);
                sent_at.push(now);
                pending.next();
            }
        }

        // max_per_hour is 10, so six hours can carry at most 60 — the forty
        // all get out, but paced.
        assert_eq!(sent_at.len(), 40, "the whole backlog should eventually drain");

        let first_hour = sent_at.iter().filter(|s| **s < t(3600)).count();
        assert!(
            first_hour <= 10,
            "first hour sent {first_hour}, ceiling is 10"
        );

        // And no two sends closer together than min_gap.
        for pair in sent_at.windows(2) {
            assert!(
                pair[1] - pair[0] >= Duration::seconds(90),
                "sends {} and {} are too close",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn send_history_does_not_grow_without_bound() {
        // This process runs for weeks. Only the rolling hour can ever affect a
        // decision, so anything older is dead weight.
        let mut g = Governor::new(limits(), NoJitter);

        for i in 0..1_000 {
            g.record_send(t(i * 300)); // one send every 5 minutes, ~83 hours
        }

        // An hour holds at most 12 sends at this spacing.
        assert!(
            g.tracked_sends() <= 13,
            "history holds {} entries; only the last hour is relevant",
            g.tracked_sends()
        );
    }

    #[test]
    fn random_jitter_stays_within_bounds() {
        let j = RandomJitter;
        let max = Duration::seconds(45);

        for _ in 0..200 {
            let p = j.padding(max);
            assert!(p >= Duration::zero() && p <= max, "padding {p} out of range");
        }
    }

    #[test]
    fn random_jitter_actually_varies() {
        // A constant "random" padding would leave the cadence metronomic,
        // which is the thing jitter exists to break.
        let j = RandomJitter;
        let max = Duration::seconds(45);

        let samples: std::collections::HashSet<_> =
            (0..100).map(|_| j.padding(max).num_seconds()).collect();

        assert!(samples.len() > 1, "jitter produced a single value every time");
    }
}
