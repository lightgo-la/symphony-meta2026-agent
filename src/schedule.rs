//! Start scheduling: schedule a batch run at a date/time, repeating once, daily, or on an interval.

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, Timelike};

/// Repeat mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchedRepeat {
    /// Run once at the scheduled time.
    #[default]
    Once,
    /// Run daily at the same time.
    Daily,
    /// Run every N minutes after the last run.
    Interval,
}

impl SchedRepeat {
    /// Korean source label. Wrapped with t() on screen.
    pub fn label(&self) -> &'static str {
        match self {
            SchedRepeat::Once => "한 번만",
            SchedRepeat::Daily => "매일",
            SchedRepeat::Interval => "간격 반복",
        }
    }
}

/// Schedule settings. Saved to the config file as a whole (except `last_run`).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Schedule {
    #[serde(default)]
    pub enabled: bool,
    /// "YYYY-MM-DD". Treated as today when empty.
    #[serde(default)]
    pub date: String,
    /// "HH:MM" (24-hour).
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub repeat: SchedRepeat,
    /// Minute part of the interval repeat (1 or more).
    #[serde(default = "default_interval_mins")]
    pub interval_mins: u32,
    /// Hour part of the interval repeat. Combined with minutes.
    #[serde(default)]
    pub interval_hours: u32,
    /// Last fired time. Memory-only; not persisted.
    #[serde(skip)]
    pub last_run: Option<DateTime<Local>>,
}

fn default_interval_mins() -> u32 {
    60
}

impl Schedule {
    /// Next run time. None when disabled or malformed.
    pub fn next_due(&self, now: DateTime<Local>) -> Option<DateTime<Local>> {
        if !self.enabled {
            return None;
        }
        let base = parse_target(&self.date, &self.time, now)?;
        match self.repeat {
            SchedRepeat::Once => {
                if self.last_run.is_some() {
                    None
                } else {
                    Some(base)
                }
            }
            SchedRepeat::Daily => {
                let start_naive = match self.last_run {
                    Some(last) => {
                        let d = last.date_naive().succ_opt()?;
                        NaiveDateTime::new(d, base.time())
                    }
                    None => NaiveDateTime::new(base.date_naive(), base.time()),
                };
                let mut next = start_naive.and_local_timezone(Local).single()?;
                while next <= now {
                    next = next.checked_add_signed(TimeDelta::days(1))?;
                }
                Some(next)
            }
            SchedRepeat::Interval => {
                let mins = (self.interval_hours as i64 * 60 + self.interval_mins as i64).max(1);
                match self.last_run {
                    Some(last) => last.checked_add_signed(TimeDelta::minutes(mins)),
                    None => Some(base),
                }
            }
        }
    }

    /// Checks whether it is due now. Stamps last_run and returns true when due.
    pub fn tick(&mut self, now: DateTime<Local>) -> bool {
        let Some(due) = self.next_due(now) else {
            return false;
        };
        if due <= now {
            self.last_run = Some(now);
            if self.repeat == SchedRepeat::Once {
                self.enabled = false;
            }
            true
        } else {
            false
        }
    }
}

/// Per-node schedule: runs only this node after N minutes/hours.
/// Can combine a loop count with stop conditions (output text, success).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct NodeSched {
    #[serde(default)]
    pub enabled: bool,
    /// Minute part of the first-run/loop interval.
    #[serde(default)]
    pub wait_mins: u32,
    /// Hour part of the first-run/loop interval. Combined with minutes.
    #[serde(default)]
    pub wait_hours: u32,
    /// Total run count. 1 means run once then turn off.
    #[serde(default = "default_loop_count")]
    pub loop_count: u32,
    /// Stops the loop when the output contains this text. Unused when empty.
    #[serde(default)]
    pub stop_when_contains: String,
    /// Stops when a non-error output appears.
    #[serde(default)]
    pub stop_on_success: bool,
    /// Next run time. Memory-only; not persisted.
    #[serde(skip)]
    pub next_due: Option<DateTime<Local>>,
    /// Runs done by this schedule. Memory-only; not persisted.
    #[serde(skip)]
    pub runs_done: u32,
}

fn default_loop_count() -> u32 {
    1
}

impl NodeSched {
    /// Wait duration. Raised to 1 minute when 0 to avoid runaway.
    pub fn wait(&self) -> TimeDelta {
        let mins = self.wait_hours as i64 * 60 + self.wait_mins as i64;
        TimeDelta::minutes(mins.max(1))
    }

    /// Arms the wait from now when enabled but no time is set.
    pub fn arm(&mut self, now: DateTime<Local>) {
        if self.enabled && self.next_due.is_none() {
            self.next_due = now.checked_add_signed(self.wait());
        }
    }

    /// Checks whether it is due now (without firing).
    pub fn due(&self, now: DateTime<Local>) -> bool {
        self.enabled && self.next_due.is_some_and(|d| d <= now)
    }

    /// When due, evaluates stop conditions and returns whether to run.
    /// Conditions are judged from the previous output, so the first run always fires.
    /// After firing or hitting a condition, schedules the next time or turns off.
    pub fn fire(&mut self, now: DateTime<Local>, latest_output: &str) -> bool {
        if !self.due(now) {
            return false;
        }
        if self.runs_done > 0 {
            if !self.stop_when_contains.is_empty()
                && latest_output.contains(&self.stop_when_contains)
            {
                self.enabled = false;
                self.next_due = None;
                return false;
            }
            if self.stop_on_success && is_ok_output(latest_output) {
                self.enabled = false;
                self.next_due = None;
                return false;
            }
        }
        self.runs_done = self.runs_done.saturating_add(1);
        if self.runs_done >= self.loop_count.max(1) {
            self.enabled = false;
            self.next_due = None;
        } else {
            self.next_due = now.checked_add_signed(self.wait());
        }
        true
    }
}

/// Treats non-empty, non-error-shaped output as success.
fn is_ok_output(s: &str) -> bool {
    !s.trim().is_empty() && !crate::node::is_error_output(s)
}

/// Converts "YYYY-MM-DD"+"HH:MM" to local time.
/// Empty date means today; past times are returned as-is (due immediately).
pub fn parse_target(date: &str, time: &str, now: DateTime<Local>) -> Option<DateTime<Local>> {
    let t = NaiveTime::parse_from_str(time.trim(), "%H:%M").ok()?;
    let d = if date.trim().is_empty() {
        now.date_naive()
    } else {
        NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d").ok()?
    };
    NaiveDateTime::new(d, t).and_local_timezone(Local).single()
}

/// "YYYY-MM-DD HH:MM" for display.
pub fn fmt_when(dt: DateTime<Local>) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Local> {
        Local::now()
    }

    #[test]
    fn parse_accepts_time_only_as_today() {
        let n = now();
        let t = parse_target("", "23:59", n).unwrap();
        assert_eq!(t.date_naive(), n.date_naive());
        assert_eq!((t.hour(), t.minute()), (23, 59));
    }

    #[test]
    fn parse_rejects_bad_shapes() {
        let n = now();
        assert!(parse_target("", "9시", n).is_none());
        assert!(parse_target("2026-13-40", "10:00", n).is_none());
        assert!(parse_target("", "", n).is_none());
    }

    #[test]
    fn once_fires_then_disables() {
        let n = now();
        let mut s = Schedule {
            enabled: true,
            date: String::new(),
            time: "00:00".to_string(),
            repeat: SchedRepeat::Once,
            interval_mins: 60,
            interval_hours: 0,
            last_run: None,
        };
        // Today 00:00 has already passed, so it is due immediately.
        assert!(s.tick(n));
        assert!(!s.enabled);
        assert!(s.last_run.is_some());
        assert!(!s.tick(n));
    }

    #[test]
    fn daily_always_points_to_future() {
        let n = now();
        let s = Schedule {
            enabled: true,
            date: String::new(),
            time: "00:00".to_string(),
            repeat: SchedRepeat::Daily,
            interval_mins: 60,
            interval_hours: 0,
            last_run: None,
        };
        let due = s.next_due(n).unwrap();
        assert!(due > n);
        // Must be tomorrow 00:00.
        assert_eq!((due.hour(), due.minute()), (0, 0));
    }

    #[test]
    fn interval_counts_from_last_run() {
        let n = now();
        let s = Schedule {
            enabled: true,
            date: String::new(),
            time: "00:00".to_string(),
            repeat: SchedRepeat::Interval,
            interval_mins: 30,
            interval_hours: 0,
            last_run: Some(n),
        };
        let due = s.next_due(n).unwrap();
        assert_eq!((due - n).num_minutes(), 30);
    }

    #[test]
    fn disabled_never_fires() {
        let n = now();
        let mut s = Schedule::default();
        assert!(!s.tick(n));
        assert!(s.next_due(n).is_none());
    }

    #[test]
    fn interval_hours_add_to_mins() {
        let n = now();
        let s = Schedule {
            enabled: true,
            date: String::new(),
            time: "00:00".to_string(),
            repeat: SchedRepeat::Interval,
            interval_mins: 30,
            interval_hours: 1,
            last_run: Some(n),
        };
        let due = s.next_due(n).unwrap();
        assert_eq!((due - n).num_minutes(), 90);
    }

    fn node_sched() -> NodeSched {
        NodeSched {
            enabled: true,
            wait_mins: 30,
            ..NodeSched::default()
        }
    }

    #[test]
    fn node_fires_once_after_wait_then_disables() {
        let n = now();
        let mut s = node_sched();
        s.arm(n);
        assert!(!s.due(n));
        let later = n + TimeDelta::minutes(31);
        assert!(s.due(later));
        assert!(s.fire(later, ""));
        assert_eq!(s.runs_done, 1);
        // loop_count defaults to 1, so it turns off after one run.
        assert!(!s.enabled);
        assert!(!s.fire(later, ""));
    }

    #[test]
    fn node_wait_hours_add_up_and_zero_becomes_one_minute() {
        let s = NodeSched {
            wait_hours: 1,
            wait_mins: 30,
            ..NodeSched::default()
        };
        assert_eq!(s.wait(), TimeDelta::minutes(90));
        let zero = NodeSched::default();
        assert_eq!(zero.wait(), TimeDelta::minutes(1));
    }

    #[test]
    fn node_loops_count_times() {
        let n = now();
        let mut s = node_sched();
        s.loop_count = 3;
        s.arm(n);
        for i in 0..3 {
            let at = n + TimeDelta::minutes(31 * (i + 1));
            assert!(s.fire(at, ""), "fire {i} should run");
        }
        assert_eq!(s.runs_done, 3);
        assert!(!s.enabled);
    }

    #[test]
    fn node_stops_when_output_contains_marker() {
        let n = now();
        let mut s = node_sched();
        s.loop_count = 99;
        s.stop_when_contains = "완료".to_string();
        s.arm(n);
        assert!(s.fire(n + TimeDelta::minutes(31), ""));
        // Stops at the next turn when the marker appears in the previous output.
        assert!(!s.fire(n + TimeDelta::minutes(62), "작업 완료"));
        assert!(!s.enabled);
        assert_eq!(s.runs_done, 1);
    }

    #[test]
    fn node_stops_on_success_but_retries_errors() {
        let n = now();
        let mut s = node_sched();
        s.loop_count = 99;
        s.stop_on_success = true;
        s.arm(n);
        assert!(s.fire(n + TimeDelta::minutes(31), ""));
        // Keeps running on error output, stops on normal output.
        assert!(s.fire(n + TimeDelta::minutes(62), "[오류] 키 없음"));
        assert!(!s.fire(n + TimeDelta::minutes(93), "정상 결과"));
        assert!(!s.enabled);
        assert_eq!(s.runs_done, 2);
    }
}
