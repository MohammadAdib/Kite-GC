// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

//! FC-reported flight modes: MAVLink AVAILABLE_MODES (#435), ArduPilot 4.6+.
//!
//! The vehicle-control panel used to offer a fixed per-vehicle mode table, so modes compiled into the
//! firmware but missing from the table were never offered, and modes the table listed could be absent
//! from (or unusable on) the connected build. AVAILABLE_MODES lets the FC say which modes it has, and
//! ArduPilot flags the ones that can't be entered right now (Follow without FOLL_ENABLE, FlowHold
//! without a flow sensor, …) with MAV_MODE_PROPERTY_NOT_USER_SELECTABLE.
//!
//! The list is enumerated one index at a time. ArduPilot answers a MAV_CMD_REQUEST_MESSAGE with
//! `param2 = 0` ("all modes") with a single message (index 1, verified against 4.7.2 SITL), so we read
//! `number_modes` from the first reply and request each remaining index in turn. The complete list is
//! emitted as `available-modes`; a partial list is never emitted (the frontend keeps its static table).
//! Firmware without AVAILABLE_MODES never answers: we give up after a few timeouts and the frontend
//! keeps the static table. AVAILABLE_MODES_MONITOR (#437) carries a sequence number the FC bumps when
//! its mode set changes; a change re-runs the enumeration.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ::mavlink::ardupilotmega::{MavCmd, MavMessage, AVAILABLE_MODES_DATA, COMMAND_LONG_DATA};
use serde::Serialize;

/// AVAILABLE_MODES message id (common #435).
const MSG_ID_AVAILABLE_MODES: f32 = 435.0;
/// Let the post-connect stream-rate / param burst settle before enumerating.
const START_DELAY: Duration = Duration::from_millis(1000);
/// No reply within this window → re-request the same index.
const REPLY_TIMEOUT: Duration = Duration::from_millis(1000);
/// Consecutive unanswered requests before we decide the FC doesn't support AVAILABLE_MODES.
const MAX_TIMEOUTS: u8 = 3;

/// One FC-reported mode, as sent to the frontend.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FcMode {
    /// ArduPilot: the flat Mode::Number (the value MAV_CMD_DO_SET_MODE takes).
    pub custom_mode: u32,
    /// MAV_STANDARD_MODE (0 = non-standard; ArduPilot reports 0 for every mode as of 4.7).
    pub standard_mode: u8,
    /// MAV_MODE_PROPERTY bitmask (1 = advanced, 2 = not user-selectable, 4 = auto mode).
    pub properties: u32,
    /// The FC's display name ("Altitude Hold", "Smart RTL", …).
    pub name: String,
}

impl FcMode {
    fn from_msg(m: &AVAILABLE_MODES_DATA) -> Self {
        FcMode {
            custom_mode: m.custom_mode,
            standard_mode: m.standard_mode as u8,
            properties: m.properties.bits(),
            name: m.mode_name.to_str().unwrap_or("").trim().to_string(),
        }
    }
}

/// Drives the index-by-index enumeration. Pure state machine: the handler loop calls `poll` to learn
/// whether to send a request, and feeds replies to `on_mode` / `on_monitor`.
pub struct ModeEnumerator {
    modes: BTreeMap<u8, FcMode>,
    total: Option<u8>,
    started: Instant,
    last_request: Option<Instant>,
    timeouts: u8,
    /// Complete list emitted, or gave up. A monitor sequence change re-arms it.
    done: bool,
    monitor_seq: Option<u8>,
}

impl ModeEnumerator {
    pub fn new(now: Instant) -> Self {
        ModeEnumerator {
            modes: BTreeMap::new(),
            total: None,
            started: now,
            last_request: None,
            timeouts: 0,
            done: false,
            monitor_seq: None,
        }
    }

    fn restart(&mut self, now: Instant) {
        self.modes.clear();
        self.total = None;
        self.started = now;
        self.last_request = None;
        self.timeouts = 0;
        self.done = false;
    }

    /// The mode index to request now, if a request is due (at most one in flight).
    pub fn poll(&mut self, now: Instant) -> Option<u8> {
        if self.done || now.duration_since(self.started) < START_DELAY {
            return None;
        }
        if let Some(t) = self.last_request {
            if now.duration_since(t) < REPLY_TIMEOUT {
                return None;
            }
            self.timeouts += 1;
            if self.timeouts >= MAX_TIMEOUTS {
                self.done = true;
                log::info!(
                    "AVAILABLE_MODES: no reply after {} requests ({} of {:?} received) — using the static mode table",
                    MAX_TIMEOUTS, self.modes.len(), self.total
                );
                return None;
            }
        }
        self.last_request = Some(now);
        Some(self.next_missing())
    }

    fn next_missing(&self) -> u8 {
        let total = self.total.unwrap_or(1);
        (1..=total).find(|i| !self.modes.contains_key(i)).unwrap_or(1)
    }

    /// Feed one AVAILABLE_MODES reply. Returns the full list (FC order) once every index has arrived.
    pub fn on_mode(&mut self, m: &AVAILABLE_MODES_DATA) -> Option<Vec<FcMode>> {
        if self.done || m.number_modes == 0 || m.mode_index == 0 || m.mode_index > m.number_modes {
            return None;
        }
        if self.total != Some(m.number_modes) {
            // First reply, or the FC's count changed mid-enumeration → (re)start collecting.
            self.modes.clear();
            self.total = Some(m.number_modes);
        }
        self.modes.insert(m.mode_index, FcMode::from_msg(m));
        self.timeouts = 0;
        self.last_request = None; // reply in → request the next index on the next poll

        if self.modes.len() == m.number_modes as usize {
            self.done = true;
            return Some(self.modes.values().cloned().collect());
        }
        None
    }

    /// Feed one AVAILABLE_MODES_MONITOR. A sequence change means the FC's mode set changed (e.g. a
    /// mode was enabled by a parameter) → enumerate again. Returns true when a re-enumeration starts.
    pub fn on_monitor(&mut self, seq: u8, now: Instant) -> bool {
        let changed = matches!(self.monitor_seq, Some(prev) if prev != seq);
        self.monitor_seq = Some(seq);
        if changed {
            log::info!("AVAILABLE_MODES_MONITOR seq changed to {} — re-reading the mode list", seq);
            self.restart(now);
        }
        changed
    }

    /// MAV_CMD_REQUEST_MESSAGE for one AVAILABLE_MODES index.
    pub fn request_msg(fc_sysid: u8, index: u8) -> MavMessage {
        MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
            target_system: fc_sysid,
            target_component: 1, // MAV_COMP_ID_AUTOPILOT1
            command: MavCmd::MAV_CMD_REQUEST_MESSAGE,
            confirmation: 0,
            param1: MSG_ID_AVAILABLE_MODES,
            param2: index as f32,
            param3: 0.0,
            param4: 0.0,
            param5: 0.0,
            param6: 0.0,
            param7: 0.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::mavlink::ardupilotmega::{MavModeProperty, MavStandardMode};

    fn reply(index: u8, total: u8, custom: u32, props: u32, name: &str) -> AVAILABLE_MODES_DATA {
        AVAILABLE_MODES_DATA {
            custom_mode: custom,
            properties: MavModeProperty::from_bits_truncate(props),
            number_modes: total,
            mode_index: index,
            standard_mode: MavStandardMode::MAV_STANDARD_MODE_NON_STANDARD,
            mode_name: name.into(),
        }
    }

    #[test]
    fn waits_for_start_delay_then_requests_index_one() {
        let t0 = Instant::now();
        let mut e = ModeEnumerator::new(t0);
        assert_eq!(e.poll(t0), None);
        assert_eq!(e.poll(t0 + START_DELAY), Some(1));
        // One request in flight — nothing more until a reply or the timeout.
        assert_eq!(e.poll(t0 + START_DELAY + Duration::from_millis(100)), None);
    }

    #[test]
    fn walks_the_indices_and_emits_the_complete_list_once() {
        let t0 = Instant::now();
        let mut e = ModeEnumerator::new(t0);
        let t = t0 + START_DELAY;
        assert_eq!(e.poll(t), Some(1));
        assert_eq!(e.on_mode(&reply(1, 3, 27, 0, "Auto RTL")), None);
        assert_eq!(e.poll(t), Some(2));
        assert_eq!(e.on_mode(&reply(2, 3, 3, 0, "Auto")), None);
        assert_eq!(e.poll(t), Some(3));
        let list = e.on_mode(&reply(3, 3, 23, 2, "Follow")).expect("complete");
        assert_eq!(list.iter().map(|m| m.custom_mode).collect::<Vec<_>>(), vec![27, 3, 23]);
        assert_eq!(list[2].properties, 2);
        assert_eq!(list[0].name, "Auto RTL");
        // Done: no further requests, late duplicates are ignored.
        assert_eq!(e.poll(t + Duration::from_secs(5)), None);
        assert_eq!(e.on_mode(&reply(1, 3, 27, 0, "Auto RTL")), None);
    }

    #[test]
    fn retries_a_lost_reply_then_gives_up() {
        let t0 = Instant::now();
        let mut e = ModeEnumerator::new(t0);
        let mut t = t0 + START_DELAY;
        assert_eq!(e.poll(t), Some(1));
        for _ in 1..MAX_TIMEOUTS {
            t += REPLY_TIMEOUT;
            assert_eq!(e.poll(t), Some(1), "re-request after a timeout");
        }
        t += REPLY_TIMEOUT;
        assert_eq!(e.poll(t), None, "gave up");
        assert_eq!(e.poll(t + Duration::from_secs(10)), None);
    }

    #[test]
    fn a_count_change_mid_enumeration_restarts_collection() {
        let t0 = Instant::now();
        let mut e = ModeEnumerator::new(t0);
        assert_eq!(e.on_mode(&reply(1, 3, 27, 0, "Auto RTL")), None);
        assert_eq!(e.on_mode(&reply(2, 3, 3, 0, "Auto")), None);
        // FC now reports 2 modes → earlier entries are discarded.
        assert_eq!(e.on_mode(&reply(1, 2, 0, 0, "Stabilize")), None);
        let list = e.on_mode(&reply(2, 2, 1, 0, "Acro")).expect("complete");
        assert_eq!(list.iter().map(|m| m.custom_mode).collect::<Vec<_>>(), vec![0, 1]);
    }

    #[test]
    fn monitor_sequence_change_rearms_the_enumeration() {
        let t0 = Instant::now();
        let mut e = ModeEnumerator::new(t0);
        assert!(e.on_mode(&reply(1, 1, 0, 0, "Stabilize")).is_some());
        assert!(!e.on_monitor(4, t0), "first sequence seen is the baseline");
        assert!(!e.on_monitor(4, t0), "unchanged");
        let t1 = t0 + Duration::from_secs(30);
        assert!(e.on_monitor(5, t1), "changed → restart");
        assert_eq!(e.poll(t1), None, "start delay applies again");
        assert_eq!(e.poll(t1 + START_DELAY), Some(1));
    }
}
