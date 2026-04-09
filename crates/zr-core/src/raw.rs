use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result, bail};
use loguna::LogReader;
use loguna::MessageId;
use loguna::proto::{Referee, TrackerWrapperPacket};
use prost::Message;

use crate::config::{PipelineConfig, TeamSelector};
use crate::types::{
    AuditSummary, BallState, CleanFrame, EntityState, FrameFlags, GameMetadata, MatchPhase,
    RefereeSnapshot, RoleLabel, SequenceKind, TeamColor,
};

#[derive(Debug, Clone)]
pub struct RawGame {
    pub metadata: GameMetadata,
    pub audit: AuditSummary,
    pub frames: Vec<CleanFrame>,
}

#[derive(Debug, Clone)]
struct RawFrame {
    timestamp_s: f64,
    frame_number: u32,
    ball: Option<BallState>,
    yellow: Vec<EntityState>,
    blue: Vec<EntityState>,
}

#[derive(Debug, Clone, Default)]
struct RefereeState {
    latest: Option<RefereeSnapshot>,
    phase: MatchPhase,
    yellow_score: u32,
    blue_score: u32,
    yellow_name: String,
    blue_name: String,
}

#[derive(Debug, Clone)]
struct TeamTracker {
    next_stable_id: u32,
    slots: Vec<TrackSlot>,
}

#[derive(Debug, Clone)]
struct TrackSlot {
    stable_id: u32,
    role: RoleLabel,
    state: EntityState,
    missing_frames: usize,
}

impl TeamTracker {
    fn new() -> Self {
        Self {
            next_stable_id: 0,
            slots: Vec::new(),
        }
    }
}

pub fn load_raw_game(path: &Path, config: &PipelineConfig) -> Result<RawGame> {
    let mut reader = LogReader::open(path)
        .with_context(|| format!("failed to open log file {}", path.display()))?;

    let mut messages = 0usize;
    let mut tracker_frames = Vec::new();
    let mut distinct_tracker_sources = BTreeMap::<String, usize>::new();
    let mut referee_state = RefereeState::default();
    let mut duplicate_frames = 0usize;
    let mut last_tracker_key = None;

    while let Some(msg) = reader.next_message()? {
        messages += 1;
        match msg.message_id {
            MessageId::Referee2013 => {
                if let Ok(referee) = Referee::decode(msg.payload.as_slice()) {
                    referee_state = update_referee_state(&referee);
                }
            }
            MessageId::VisionTracker2020 => {
                let wrapper = match TrackerWrapperPacket::decode(msg.payload.as_slice()) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let source_name = wrapper.source_name.unwrap_or_else(|| "unknown".to_string());
                if let Some(required) = &config.tracker_source
                    && source_name != *required
                {
                    continue;
                }
                let Some(frame) = wrapper.tracked_frame else {
                    continue;
                };
                let key = (
                    frame.frame_number,
                    frame.timestamp.to_bits(),
                    source_name.clone(),
                );
                if last_tracker_key.as_ref() == Some(&key) {
                    duplicate_frames += 1;
                    continue;
                }
                last_tracker_key = Some(key);
                *distinct_tracker_sources.entry(source_name).or_default() += 1;
                tracker_frames.push(raw_frame_from_tracker(frame));
            }
            _ => {}
        }
    }

    if tracker_frames.is_empty() {
        bail!("no VisionTracker2020 frames found in {}", path.display());
    }

    tracker_frames.sort_by(|left, right| {
        left.timestamp_s
            .partial_cmp(&right.timestamp_s)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.frame_number.cmp(&right.frame_number))
    });

    let target_color = resolve_target_color(&referee_state, config)?;
    let target_team_name = match target_color {
        TeamColor::Yellow => referee_state.yellow_name.clone(),
        TeamColor::Blue => referee_state.blue_name.clone(),
    };
    let opponent_team_name = match target_color {
        TeamColor::Yellow => referee_state.blue_name.clone(),
        TeamColor::Blue => referee_state.yellow_name.clone(),
    };

    let TrackingResult {
        mut clean_frames,
        suspicious_identity_swaps,
    } = build_clean_frames(&tracker_frames, &referee_state, target_color, config);
    estimate_dynamics(&mut clean_frames, config);

    let out_of_bounds_objects = clean_frames
        .iter()
        .map(|frame| frame.flags.out_of_bounds_objects)
        .sum();
    let missing_ball_frames = clean_frames
        .iter()
        .filter(|frame| frame.ball.is_none())
        .count();

    let duration_s = clean_frames
        .last()
        .map(|frame| frame.timestamp_s)
        .unwrap_or_default()
        - clean_frames
            .first()
            .map(|frame| frame.timestamp_s)
            .unwrap_or_default();
    let sample_rate_hz = estimate_sample_rate_hz(&clean_frames);
    let year = path
        .file_name()
        .and_then(|value| value.to_str())
        .and_then(parse_year_from_name);

    let tracker_source = distinct_tracker_sources
        .iter()
        .max_by_key(|entry| entry.1)
        .map(|entry| entry.0.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let metadata = GameMetadata {
        game_id: game_id_from_path(path),
        source_log: path.display().to_string(),
        year,
        phase: referee_state.phase,
        target_team: if target_team_name.is_empty() {
            target_color.as_str().to_string()
        } else {
            target_team_name.clone()
        },
        opponent_team: if opponent_team_name.is_empty() {
            target_color.opponent().as_str().to_string()
        } else {
            opponent_team_name.clone()
        },
        target_color,
        target_score: match target_color {
            TeamColor::Yellow => referee_state.yellow_score,
            TeamColor::Blue => referee_state.blue_score,
        },
        opponent_score: match target_color {
            TeamColor::Yellow => referee_state.blue_score,
            TeamColor::Blue => referee_state.yellow_score,
        },
        sample_rate_hz,
        duration_s,
        tracker_source: tracker_source.clone(),
    };

    let mut notes = Vec::new();
    if duplicate_frames > 0 {
        notes.push(format!(
            "dropped {duplicate_frames} duplicate tracker frames"
        ));
    }
    if missing_ball_frames > 0 {
        notes.push(format!(
            "{missing_ball_frames} frames missing a primary ball"
        ));
    }
    if suspicious_identity_swaps > 0 {
        notes.push(format!(
            "flagged {suspicious_identity_swaps} likely identity swaps"
        ));
    }

    let audit = AuditSummary {
        total_messages: messages,
        tracker_frames_seen: tracker_frames.len() + duplicate_frames,
        tracker_frames_used: clean_frames.len(),
        duplicate_frames,
        out_of_bounds_objects,
        missing_ball_frames,
        distinct_tracker_sources: distinct_tracker_sources.into_keys().collect(),
        sample_rate_hz,
        target_team_resolved: metadata.target_team.clone(),
        notes,
        suspicious_identity_swaps,
    };

    Ok(RawGame {
        metadata,
        audit,
        frames: clean_frames,
    })
}

fn update_referee_state(referee: &Referee) -> RefereeState {
    let phase = match referee.match_type.unwrap_or_default() {
        1 => MatchPhase::Group,
        2 => MatchPhase::Elimination,
        3 => MatchPhase::Friendly,
        _ => MatchPhase::Unknown,
    };
    RefereeState {
        latest: Some(RefereeSnapshot {
            stage: Some(referee.stage),
            command: Some(referee.command),
            blue_team_on_positive_half: referee.blue_team_on_positive_half,
            match_type: referee.match_type,
            yellow_name: Some(referee.yellow.name.clone()),
            blue_name: Some(referee.blue.name.clone()),
            yellow_score: Some(referee.yellow.score),
            blue_score: Some(referee.blue.score),
        }),
        phase,
        yellow_score: referee.yellow.score,
        blue_score: referee.blue.score,
        yellow_name: referee.yellow.name.clone(),
        blue_name: referee.blue.name.clone(),
    }
}

fn raw_frame_from_tracker(frame: loguna::proto::TrackedFrame) -> RawFrame {
    let mut yellow = Vec::new();
    let mut blue = Vec::new();
    for robot in frame.robots {
        let state = EntityState {
            raw_id: robot.robot_id.id,
            stable_id: None,
            role: RoleLabel::Unknown,
            x: robot.pos.x as f32,
            y: robot.pos.y as f32,
            theta: robot.orientation,
            vx: robot
                .vel
                .as_ref()
                .map(|value| value.x as f32)
                .unwrap_or_default(),
            vy: robot
                .vel
                .as_ref()
                .map(|value| value.y as f32)
                .unwrap_or_default(),
            omega: robot.vel_angular.unwrap_or_default(),
            ax: 0.0,
            ay: 0.0,
            visibility: robot.visibility.unwrap_or(1.0),
        };
        match TeamColor::from_proto(robot.robot_id.team) {
            Some(TeamColor::Yellow) => yellow.push(state),
            Some(TeamColor::Blue) => blue.push(state),
            None => {}
        }
    }

    let ball = frame.balls.into_iter().next().map(|ball| BallState {
        x: ball.pos.x as f32,
        y: ball.pos.y as f32,
        z: ball.pos.z as f32,
        vx: ball
            .vel
            .as_ref()
            .map(|value| value.x as f32)
            .unwrap_or_default(),
        vy: ball
            .vel
            .as_ref()
            .map(|value| value.y as f32)
            .unwrap_or_default(),
        ax: 0.0,
        ay: 0.0,
        visibility: ball.visibility.unwrap_or(1.0),
    });

    RawFrame {
        timestamp_s: frame.timestamp,
        frame_number: frame.frame_number,
        ball,
        yellow,
        blue,
    }
}

fn resolve_target_color(state: &RefereeState, config: &PipelineConfig) -> Result<TeamColor> {
    match &config.target_team {
        TeamSelector::Color(color) => Ok(*color),
        TeamSelector::Name(name) => {
            if state.yellow_name == *name {
                Ok(TeamColor::Yellow)
            } else if state.blue_name == *name {
                Ok(TeamColor::Blue)
            } else {
                bail!(
                    "team name '{}' not found in referee metadata (yellow='{}', blue='{}')",
                    name,
                    state.yellow_name,
                    state.blue_name
                )
            }
        }
    }
}

struct TrackingResult {
    clean_frames: Vec<CleanFrame>,
    suspicious_identity_swaps: usize,
}

fn build_clean_frames(
    raw_frames: &[RawFrame],
    referee_state: &RefereeState,
    target_color: TeamColor,
    config: &PipelineConfig,
) -> TrackingResult {
    let mut frames = Vec::with_capacity(raw_frames.len());
    let mut carried_counter = 0usize;
    let mut previous_timestamp: Option<f64> = None;
    let mut suspicious_identity_swaps = 0usize;
    let mut yellow_tracker = TeamTracker::new();
    let mut blue_tracker = TeamTracker::new();

    for raw in raw_frames {
        let TrackedTeamResult {
            slots: yellow_slots,
            swap_detected: yellow_swap,
        } = assign_team_tracks(&mut yellow_tracker, &raw.yellow, config);
        let TrackedTeamResult {
            slots: blue_slots,
            swap_detected: blue_swap,
        } = assign_team_tracks(&mut blue_tracker, &raw.blue, config);

        suspicious_identity_swaps += yellow_swap as usize + blue_swap as usize;

        let mut target_team = match target_color {
            TeamColor::Yellow => yellow_slots.clone(),
            TeamColor::Blue => blue_slots.clone(),
        };
        let mut opponent_team = match target_color {
            TeamColor::Yellow => blue_slots,
            TeamColor::Blue => yellow_slots,
        };

        let mut out_of_bounds_objects = 0usize;
        filter_slots_in_bounds(&mut target_team, config, &mut out_of_bounds_objects);
        filter_slots_in_bounds(&mut opponent_team, config, &mut out_of_bounds_objects);
        let ball = raw
            .ball
            .clone()
            .filter(|ball| within_bounds(ball.x, ball.y, config, &mut out_of_bounds_objects));

        let carried_ball = ball
            .as_ref()
            .and_then(|ball| {
                target_team
                    .iter()
                    .chain(opponent_team.iter())
                    .flatten()
                    .map(|robot| {
                        let dx = robot.x - ball.x;
                        let dy = robot.y - ball.y;
                        (dx * dx + dy * dy).sqrt()
                    })
                    .reduce(f32::min)
            })
            .map(|dist| dist <= config.possession_radius_m)
            .unwrap_or(false);

        carried_counter = if carried_ball { carried_counter + 1 } else { 0 };
        let positive = target_attacks_positive(referee_state, target_color);
        let duplicate_timestamp = previous_timestamp
            .map(|prev| (raw.timestamp_s - prev).abs() <= f64::EPSILON)
            .unwrap_or(false);
        previous_timestamp = Some(raw.timestamp_s);

        let missing_target_robot_slots = target_team.iter().filter(|robot| robot.is_none()).count();
        let referee_live = referee_command_live(referee_state, config);
        let heuristic_live = motion_looks_live(&target_team, &opponent_team, ball.as_ref(), config);

        frames.push(CleanFrame {
            timestamp_s: raw.timestamp_s,
            frame_number: raw.frame_number,
            ball,
            target_team,
            opponent_team,
            referee: referee_state.latest.clone(),
            live: referee_live || heuristic_live,
            target_attacks_positive_x: positive,
            sequence_kind: SequenceKind::Unknown,
            flags: FrameFlags {
                duplicate_timestamp,
                carried_ball: carried_counter >= config.carried_ball_frames,
                out_of_bounds_objects,
                missing_target_robot_slots,
                likely_identity_swap: yellow_swap || blue_swap,
                referee_live,
                heuristic_live,
            },
        });
    }

    TrackingResult {
        clean_frames: frames,
        suspicious_identity_swaps,
    }
}

struct TrackedTeamResult {
    slots: Vec<Option<EntityState>>,
    swap_detected: bool,
}

fn assign_team_tracks(
    tracker: &mut TeamTracker,
    observations: &[EntityState],
    config: &PipelineConfig,
) -> TrackedTeamResult {
    if tracker.slots.is_empty() {
        for observation in observations.iter().take(config.max_team_size) {
            let mut state = observation.clone();
            state.stable_id = Some(tracker.next_stable_id);
            tracker.slots.push(TrackSlot {
                stable_id: tracker.next_stable_id,
                role: RoleLabel::Unknown,
                state,
                missing_frames: 0,
            });
            tracker.next_stable_id += 1;
        }
        assign_roles(&mut tracker.slots);
        return TrackedTeamResult {
            slots: slotify_tracker(tracker, config.max_team_size),
            swap_detected: false,
        };
    }

    let assignment = minimum_cost_assignment(
        tracker,
        observations,
        config.identity.max_match_distance_m,
        config.identity.jump_penalty_m,
    );

    let mut matched_slots = HashMap::new();
    let mut swap_detected = false;
    for (slot_index, obs_index, cost) in assignment {
        if cost > config.identity.max_match_distance_m {
            continue;
        }
        let mut state = observations[obs_index].clone();
        let stable_id = tracker.slots[slot_index].stable_id;
        let prev_raw_id = tracker.slots[slot_index].state.raw_id;
        if prev_raw_id.is_some() && state.raw_id.is_some() && prev_raw_id != state.raw_id {
            swap_detected = true;
        }
        state.stable_id = Some(stable_id);
        state.role = tracker.slots[slot_index].role;
        tracker.slots[slot_index].state = state;
        tracker.slots[slot_index].missing_frames = 0;
        matched_slots.insert(slot_index, obs_index);
    }

    for (slot_index, slot) in tracker.slots.iter_mut().enumerate() {
        if !matched_slots.contains_key(&slot_index) {
            slot.missing_frames += 1;
        }
    }
    tracker
        .slots
        .retain(|slot| slot.missing_frames <= config.identity.max_unmatched_frames);

    for (obs_index, observation) in observations.iter().enumerate() {
        if matched_slots.values().any(|matched| *matched == obs_index) {
            continue;
        }
        let mut state = observation.clone();
        state.stable_id = Some(tracker.next_stable_id);
        tracker.slots.push(TrackSlot {
            stable_id: tracker.next_stable_id,
            role: RoleLabel::Unknown,
            state,
            missing_frames: 0,
        });
        tracker.next_stable_id += 1;
    }

    tracker.slots.sort_by(|left, right| {
        left.state
            .x
            .partial_cmp(&right.state.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    assign_roles(&mut tracker.slots);

    TrackedTeamResult {
        slots: slotify_tracker(tracker, config.max_team_size),
        swap_detected,
    }
}

fn minimum_cost_assignment(
    tracker: &TeamTracker,
    observations: &[EntityState],
    max_distance: f32,
    jump_penalty: f32,
) -> Vec<(usize, usize, f32)> {
    let slot_count = tracker.slots.len();
    let obs_count = observations.len();
    let mut best = Vec::new();
    let mut best_cost = f32::INFINITY;
    let mut used_obs = vec![false; obs_count];
    let mut current = Vec::new();

    fn search(
        tracker: &TeamTracker,
        observations: &[EntityState],
        max_distance: f32,
        jump_penalty: f32,
        slot_index: usize,
        used_obs: &mut [bool],
        current: &mut Vec<(usize, usize, f32)>,
        current_cost: f32,
        best: &mut Vec<(usize, usize, f32)>,
        best_cost: &mut f32,
    ) {
        if slot_index == tracker.slots.len() {
            if current_cost < *best_cost {
                *best_cost = current_cost;
                *best = current.clone();
            }
            return;
        }
        search(
            tracker,
            observations,
            max_distance,
            jump_penalty,
            slot_index + 1,
            used_obs,
            current,
            current_cost + jump_penalty,
            best,
            best_cost,
        );
        for obs_index in 0..observations.len() {
            if used_obs[obs_index] {
                continue;
            }
            let cost = motion_cost(&tracker.slots[slot_index].state, &observations[obs_index]);
            if cost > max_distance || current_cost + cost >= *best_cost {
                continue;
            }
            used_obs[obs_index] = true;
            current.push((slot_index, obs_index, cost));
            search(
                tracker,
                observations,
                max_distance,
                jump_penalty,
                slot_index + 1,
                used_obs,
                current,
                current_cost + cost,
                best,
                best_cost,
            );
            current.pop();
            used_obs[obs_index] = false;
        }
    }

    search(
        tracker,
        observations,
        max_distance,
        jump_penalty,
        0,
        &mut used_obs,
        &mut current,
        0.0,
        &mut best,
        &mut best_cost,
    );

    let _ = slot_count;
    let _ = obs_count;
    best
}

fn motion_cost(previous: &EntityState, current: &EntityState) -> f32 {
    let dx = current.x - previous.x;
    let dy = current.y - previous.y;
    let position_cost = (dx * dx + dy * dy).sqrt();
    let dtheta = (current.theta - previous.theta).abs() * 0.05;
    position_cost + dtheta
}

fn assign_roles(slots: &mut [TrackSlot]) {
    if slots.is_empty() {
        return;
    }
    let goalkeeper_index = slots
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            left.state
                .x
                .partial_cmp(&right.state.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|entry| entry.0)
        .unwrap_or(0);
    for (index, slot) in slots.iter_mut().enumerate() {
        slot.role = if index == goalkeeper_index {
            RoleLabel::Goalkeeper
        } else if index <= 2 {
            RoleLabel::Defender
        } else if index <= 4 {
            RoleLabel::Midfielder
        } else {
            RoleLabel::Forward
        };
        slot.state.role = slot.role;
        slot.state.stable_id = Some(slot.stable_id);
    }
}

fn slotify_tracker(tracker: &TeamTracker, max_team_size: usize) -> Vec<Option<EntityState>> {
    let mut slots = vec![None; max_team_size];
    for (index, track) in tracker.slots.iter().take(max_team_size).enumerate() {
        slots[index] = Some(track.state.clone());
    }
    slots
}

fn filter_slots_in_bounds(
    team: &mut [Option<EntityState>],
    config: &PipelineConfig,
    counter: &mut usize,
) {
    for slot in team.iter_mut() {
        if let Some(robot) = slot
            && !within_bounds(robot.x, robot.y, config, counter)
        {
            *slot = None;
        }
    }
}

fn within_bounds(x: f32, y: f32, config: &PipelineConfig, counter: &mut usize) -> bool {
    let in_bounds = x.abs() <= config.field_half_length_m + config.out_of_bounds_margin_m
        && y.abs() <= config.field_half_width_m + config.out_of_bounds_margin_m;
    if !in_bounds {
        *counter += 1;
    }
    in_bounds
}

fn referee_command_live(state: &RefereeState, config: &PipelineConfig) -> bool {
    state
        .latest
        .as_ref()
        .and_then(|snapshot| snapshot.command)
        .map(|command| config.live_play.referee_live_commands.contains(&command))
        .unwrap_or(false)
}

fn motion_looks_live(
    target_team: &[Option<EntityState>],
    opponent_team: &[Option<EntityState>],
    ball: Option<&BallState>,
    config: &PipelineConfig,
) -> bool {
    let ball_live = ball
        .map(|ball| {
            (ball.vx * ball.vx + ball.vy * ball.vy).sqrt() >= config.live_play.min_ball_speed_m_s
        })
        .unwrap_or(false);
    let robot_live = target_team
        .iter()
        .chain(opponent_team.iter())
        .flatten()
        .any(|robot| {
            (robot.vx * robot.vx + robot.vy * robot.vy).sqrt()
                >= config.live_play.min_robot_speed_m_s
        });
    ball_live || robot_live
}

fn target_attacks_positive(state: &RefereeState, target_color: TeamColor) -> bool {
    let blue_positive = state
        .latest
        .as_ref()
        .and_then(|snapshot| snapshot.blue_team_on_positive_half)
        .unwrap_or(false);
    match target_color {
        TeamColor::Blue => !blue_positive,
        TeamColor::Yellow => blue_positive,
    }
}

fn estimate_dynamics(frames: &mut [CleanFrame], config: &PipelineConfig) {
    for index in 0..frames.len() {
        let prev = index.checked_sub(1);
        let next = if index + 1 < frames.len() {
            Some(index + 1)
        } else {
            None
        };
        let dt = match (prev, next) {
            (Some(prev_index), Some(next_index)) => {
                (frames[next_index].timestamp_s - frames[prev_index].timestamp_s) as f32
            }
            _ => continue,
        };
        if dt <= 0.0 {
            continue;
        }

        estimate_ball_dynamics(frames, index, prev.unwrap(), next.unwrap(), dt, config);
        estimate_team_dynamics(
            frames,
            index,
            prev.unwrap(),
            next.unwrap(),
            dt,
            true,
            config,
        );
        estimate_team_dynamics(
            frames,
            index,
            prev.unwrap(),
            next.unwrap(),
            dt,
            false,
            config,
        );
    }
}

fn estimate_ball_dynamics(
    frames: &mut [CleanFrame],
    index: usize,
    prev_index: usize,
    next_index: usize,
    dt: f32,
    config: &PipelineConfig,
) {
    let Some(prev_ball) = frames[prev_index].ball.clone() else {
        return;
    };
    let Some(next_ball) = frames[next_index].ball.clone() else {
        return;
    };
    if let Some(ball) = frames[index].ball.as_mut() {
        ball.vx =
            ((next_ball.x - prev_ball.x) / dt).clamp(-config.max_speed_m_s, config.max_speed_m_s);
        ball.vy =
            ((next_ball.y - prev_ball.y) / dt).clamp(-config.max_speed_m_s, config.max_speed_m_s);
        let prev_vx = prev_ball.vx;
        let prev_vy = prev_ball.vy;
        ball.ax = ((ball.vx - prev_vx) / (dt / 2.0))
            .clamp(-config.max_acceleration_m_s2, config.max_acceleration_m_s2);
        ball.ay = ((ball.vy - prev_vy) / (dt / 2.0))
            .clamp(-config.max_acceleration_m_s2, config.max_acceleration_m_s2);
    }
}

fn estimate_team_dynamics(
    frames: &mut [CleanFrame],
    index: usize,
    prev_index: usize,
    next_index: usize,
    dt: f32,
    target: bool,
    config: &PipelineConfig,
) {
    let prev_team = if target {
        frames[prev_index].target_team.clone()
    } else {
        frames[prev_index].opponent_team.clone()
    };
    let next_team = if target {
        frames[next_index].target_team.clone()
    } else {
        frames[next_index].opponent_team.clone()
    };
    let current_team = if target {
        &mut frames[index].target_team
    } else {
        &mut frames[index].opponent_team
    };

    for slot in 0..current_team.len() {
        let Some(prev_robot) = prev_team.get(slot).and_then(|value| value.clone()) else {
            continue;
        };
        let Some(next_robot) = next_team.get(slot).and_then(|value| value.clone()) else {
            continue;
        };
        let Some(robot) = current_team.get_mut(slot).and_then(|value| value.as_mut()) else {
            continue;
        };
        robot.vx =
            ((next_robot.x - prev_robot.x) / dt).clamp(-config.max_speed_m_s, config.max_speed_m_s);
        robot.vy =
            ((next_robot.y - prev_robot.y) / dt).clamp(-config.max_speed_m_s, config.max_speed_m_s);
        robot.omega = ((next_robot.theta - prev_robot.theta) / dt).clamp(
            -config.max_angular_speed_rad_s,
            config.max_angular_speed_rad_s,
        );
        let prev_vx = prev_robot.vx;
        let prev_vy = prev_robot.vy;
        robot.ax = ((robot.vx - prev_vx) / (dt / 2.0))
            .clamp(-config.max_acceleration_m_s2, config.max_acceleration_m_s2);
        robot.ay = ((robot.vy - prev_vy) / (dt / 2.0))
            .clamp(-config.max_acceleration_m_s2, config.max_acceleration_m_s2);
    }
}

fn estimate_sample_rate_hz(frames: &[CleanFrame]) -> f32 {
    let mut deltas = Vec::new();
    for pair in frames.windows(2) {
        let delta = pair[1].timestamp_s - pair[0].timestamp_s;
        if delta > 0.0 {
            deltas.push(delta as f32);
        }
    }
    if deltas.is_empty() {
        return 0.0;
    }
    let mean_delta = deltas.iter().sum::<f32>() / deltas.len() as f32;
    if mean_delta <= 0.0 {
        0.0
    } else {
        1.0 / mean_delta
    }
}

fn parse_year_from_name(name: &str) -> Option<u16> {
    name.get(0..4)?.parse().ok()
}

fn game_id_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("game")
        .replace('.', "_")
}
