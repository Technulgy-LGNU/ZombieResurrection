use std::collections::BTreeMap;
use std::f32::consts::PI;
use std::path::Path;

use anyhow::Result;
use rand::rng;
use rand_distr::{Distribution, Normal};

use crate::archive::{NormalizationStats, SplitAssignment, SplitBundle};
use crate::config::PipelineConfig;
use crate::raw::{load_raw_game, RawGame};
use crate::review::{ReviewStore, ReviewVerdict};
use crate::types::{
    AuditSummary, BallState, CleanFrame, EntityState, GameMetadata, MatchPhase,
    ReviewSequenceSummary, RoleLabel, SequenceKind, TrainingSample, TrainingSampleMetadata,
};

#[derive(Debug, Clone)]
pub struct PipelineOutput {
    pub metadata: GameMetadata,
    pub audit: AuditSummary,
    pub review_game: ReviewGame,
    pub samples: Vec<TrainingSample>,
    pub normalization: NormalizationStats,
}

#[derive(Debug, Clone)]
pub struct ReviewGame {
    pub metadata: GameMetadata,
    pub frames: Vec<CleanFrame>,
    pub sequence_summaries: Vec<ReviewSequenceSummary>,
}

#[derive(Debug, Clone)]
struct Sequence {
    index: usize,
    frames: Vec<CleanFrame>,
    quality_score: f32,
    quality_flags: Vec<String>,
    kind: SequenceKind,
}

pub fn audit_log(path: &Path, config: &PipelineConfig) -> Result<AuditSummary> {
    Ok(load_raw_game(path, config)?.audit)
}

pub fn preprocess_log(
    path: &Path,
    config: &PipelineConfig,
    review: Option<&ReviewStore>,
) -> Result<PipelineOutput> {
    let raw = load_raw_game(path, config)?;
    Ok(run_pipeline(raw, config, review, None))
}

pub fn preprocess_log_with_raw(
    path: &Path,
    config: &PipelineConfig,
    review: Option<&ReviewStore>,
) -> Result<(PipelineOutput, Vec<CleanFrame>)> {
    let raw = load_raw_game(path, config)?;
    let raw_frames = raw.frames.clone();
    let output = run_pipeline(raw, config, review, None);
    Ok((output, raw_frames))
}

pub fn preprocess_logs_with_splits(
    raws: Vec<RawGame>,
    config: &PipelineConfig,
    review: Option<&ReviewStore>,
) -> (Vec<PipelineOutput>, SplitBundle) {
    let assignments = assign_splits(&raws, config);
    let mut outputs = Vec::with_capacity(raws.len());
    let mut train_samples = Vec::new();

    for raw in raws {
        let split = assignments
            .iter()
            .find(|entry| entry.game_id == raw.metadata.game_id)
            .map(|entry| entry.split.clone())
            .unwrap_or_else(|| "train".to_string());
        let output = run_pipeline(raw, config, review, Some(split.clone()));
        if split == "train" {
            train_samples.extend(output.samples.iter().cloned());
        }
        outputs.push(output);
    }

    let normalization = compute_normalization_stats(&train_samples);
    let elimination_weight = config.split.elimination_weight;
    let split_bundle = SplitBundle {
        assignments,
        normalization,
        elimination_sample_weight: elimination_weight,
    };
    apply_split_weights(&mut outputs, &split_bundle);
    (outputs, split_bundle)
}

fn run_pipeline(
    raw: RawGame,
    config: &PipelineConfig,
    review: Option<&ReviewStore>,
    split: Option<String>,
) -> PipelineOutput {
    let resampled = resample_frames(raw.frames, config);
    let filtered = filter_live_frames(resampled, config);
    let canonical = canonicalize_attack_direction(filtered);
    let sequences = segment_sequences(&raw.metadata, canonical, config, review);
    let review_game = ReviewGame {
        metadata: raw.metadata.clone(),
        frames: sequences
            .iter()
            .flat_map(|sequence| sequence.frames.clone())
            .collect(),
        sequence_summaries: sequences
            .iter()
            .map(|sequence| ReviewSequenceSummary {
                sequence_index: sequence.index,
                start_frame: sequence
                    .frames
                    .first()
                    .map(|frame| frame.frame_number)
                    .unwrap_or_default(),
                end_frame: sequence
                    .frames
                    .last()
                    .map(|frame| frame.frame_number)
                    .unwrap_or_default(),
                start_time_s: sequence
                    .frames
                    .first()
                    .map(|frame| frame.timestamp_s)
                    .unwrap_or_default(),
                end_time_s: sequence
                    .frames
                    .last()
                    .map(|frame| frame.timestamp_s)
                    .unwrap_or_default(),
                frame_count: sequence.frames.len(),
                quality_score: sequence.quality_score,
                sequence_kind: sequence.kind,
            })
            .collect(),
    };
    let normalization = compute_normalization_stats_from_sequences(&sequences);
    let samples = build_samples(
        &raw.metadata,
        &sequences,
        config,
        split.unwrap_or_else(|| "train".to_string()),
    );

    PipelineOutput {
        metadata: raw.metadata,
        audit: raw.audit,
        review_game,
        samples,
        normalization,
    }
}

fn resample_frames(frames: Vec<CleanFrame>, _config: &PipelineConfig) -> Vec<CleanFrame> {
    if frames.len() < 3 {
        return frames;
    }
    let mut deltas = Vec::new();
    for pair in frames.windows(2) {
        let delta = pair[1].timestamp_s - pair[0].timestamp_s;
        if delta > 0.0 {
            deltas.push(delta);
        }
    }
    if deltas.is_empty() {
        return frames;
    }
    let mean = deltas.iter().sum::<f64>() / deltas.len() as f64;
    let max = deltas.iter().copied().fold(f64::MIN, f64::max);
    let min = deltas.iter().copied().fold(f64::MAX, f64::min);
    if max - min <= mean * 0.25 {
        return frames;
    }
    let mut resampled = Vec::new();
    let mut carry_time = frames
        .first()
        .map(|frame| frame.timestamp_s)
        .unwrap_or_default();
    let end = frames
        .last()
        .map(|frame| frame.timestamp_s)
        .unwrap_or_default();
    let mut index = 0usize;
    while carry_time <= end && index < frames.len() {
        while index + 1 < frames.len() && frames[index + 1].timestamp_s < carry_time {
            index += 1;
        }
        resampled.push(frames[index].clone());
        carry_time += mean;
    }
    resampled
}

fn filter_live_frames(frames: Vec<CleanFrame>, config: &PipelineConfig) -> Vec<CleanFrame> {
    let mut grace = 0usize;
    frames
        .into_iter()
        .filter(|frame| frame.flags.out_of_bounds_objects == 0)
        .filter(|frame| frame.flags.missing_target_robot_slots < config.max_team_size)
        .filter(|frame| {
            if frame.flags.referee_live {
                grace = config.live_play.grace_frames_after_live_command;
                return true;
            }
            if frame.flags.heuristic_live && grace > 0 {
                grace -= 1;
                return true;
            }
            frame.flags.heuristic_live
        })
        .collect()
}

fn canonicalize_attack_direction(frames: Vec<CleanFrame>) -> Vec<CleanFrame> {
    frames
        .into_iter()
        .map(|mut frame| {
            if frame.target_attacks_positive_x {
                return frame;
            }
            flip_frame_x(&mut frame);
            frame.target_attacks_positive_x = true;
            frame
        })
        .collect()
}

fn flip_frame_x(frame: &mut CleanFrame) {
    if let Some(ball) = frame.ball.as_mut() {
        ball.x = -ball.x;
        ball.vx = -ball.vx;
        ball.ax = -ball.ax;
    }
    for team in [&mut frame.target_team, &mut frame.opponent_team] {
        for robot in team.iter_mut().flatten() {
            robot.x = -robot.x;
            robot.vx = -robot.vx;
            robot.ax = -robot.ax;
            robot.theta = wrap_angle(PI - robot.theta);
            robot.omega = -robot.omega;
        }
    }
}

fn segment_sequences(
    metadata: &GameMetadata,
    frames: Vec<CleanFrame>,
    config: &PipelineConfig,
    review: Option<&ReviewStore>,
) -> Vec<Sequence> {
    let mut sequences = Vec::new();
    let mut current = Vec::new();
    let mut next_index = 0usize;

    for frame in frames {
        let should_break = current
            .last()
            .map(|previous: &CleanFrame| {
                (frame.timestamp_s - previous.timestamp_s) > config.max_frame_gap_s
                    || possession_break(previous, &frame, config)
            })
            .unwrap_or(false);

        if should_break {
            if let Some(sequence) = finalize_sequence(
                metadata,
                next_index,
                std::mem::take(&mut current),
                config,
                review,
            ) {
                sequences.push(sequence);
                next_index += 1;
            }
        }
        current.push(frame);
    }

    if let Some(sequence) = finalize_sequence(metadata, next_index, current, config, review) {
        sequences.push(sequence);
    }

    sequences
}

fn possession_break(previous: &CleanFrame, current: &CleanFrame, config: &PipelineConfig) -> bool {
    let Some(previous_ball) = &previous.ball else {
        return true;
    };
    let Some(current_ball) = &current.ball else {
        return true;
    };
    let dx = current_ball.x - previous_ball.x;
    let dy = current_ball.y - previous_ball.y;
    let move_dist = (dx * dx + dy * dy).sqrt();
    let carried_flip = previous.flags.carried_ball != current.flags.carried_ball;
    let live_flip = previous.live != current.live;
    move_dist > config.possession_radius_m * 3.0 || carried_flip || live_flip
}

fn finalize_sequence(
    metadata: &GameMetadata,
    index: usize,
    frames: Vec<CleanFrame>,
    config: &PipelineConfig,
    review: Option<&ReviewStore>,
) -> Option<Sequence> {
    if frames.len() < config.min_sequence_frames || frames.len() > config.max_sequence_frames {
        return None;
    }
    let kind = classify_sequence(&frames);
    let mut quality_flags = Vec::new();
    let quality_score = score_sequence(&frames, &mut quality_flags, kind);
    if let Some(review) = review {
        match review.verdict_for(&metadata.game_id, index) {
            ReviewVerdict::Drop => return None,
            ReviewVerdict::NeedsAttention => {
                quality_flags.push("manual-needs-attention".to_string())
            }
            ReviewVerdict::Keep | ReviewVerdict::Unreviewed => {}
        }
    }
    Some(Sequence {
        index,
        frames,
        quality_score,
        quality_flags,
        kind,
    })
}

fn classify_sequence(frames: &[CleanFrame]) -> SequenceKind {
    let carried = frames
        .iter()
        .filter(|frame| frame.flags.carried_ball)
        .count();
    let identity = frames
        .iter()
        .filter(|frame| frame.flags.likely_identity_swap)
        .count();
    if identity > 0 {
        SequenceKind::Transition
    } else if carried > frames.len() / 3 {
        SequenceKind::OpenPlay
    } else {
        SequenceKind::Unknown
    }
}

fn score_sequence(
    frames: &[CleanFrame],
    quality_flags: &mut Vec<String>,
    kind: SequenceKind,
) -> f32 {
    let mut score = 0.45;

    let missing_ball_ratio =
        frames.iter().filter(|frame| frame.ball.is_none()).count() as f32 / frames.len() as f32;
    score += (1.0 - missing_ball_ratio) * 0.20;
    if missing_ball_ratio > 0.1 {
        quality_flags.push("ball-missing".to_string());
    }

    let duplicate_ratio = frames
        .iter()
        .filter(|frame| frame.flags.duplicate_timestamp)
        .count() as f32
        / frames.len() as f32;
    score += (1.0 - duplicate_ratio) * 0.10;
    if duplicate_ratio > 0.03 {
        quality_flags.push("duplicate-timestamps".to_string());
    }

    let missing_robot_ratio = frames
        .iter()
        .map(|frame| frame.flags.missing_target_robot_slots as f32)
        .sum::<f32>()
        / (frames.len() as f32 * frames[0].target_team.len() as f32);
    score += (1.0 - missing_robot_ratio) * 0.15;
    if missing_robot_ratio > 0.10 {
        quality_flags.push("missing-robots".to_string());
    }

    let identity_ratio = frames
        .iter()
        .filter(|frame| frame.flags.likely_identity_swap)
        .count() as f32
        / frames.len() as f32;
    score += (1.0 - identity_ratio) * 0.20;
    if identity_ratio > 0.0 {
        quality_flags.push("identity-instability".to_string());
    }

    let live_ratio = frames.iter().filter(|frame| frame.live).count() as f32 / frames.len() as f32;
    score += live_ratio * 0.10;
    if live_ratio < 0.8 {
        quality_flags.push("mixed-live-state".to_string());
    }

    if matches!(kind, SequenceKind::OpenPlay) {
        score += 0.05;
    }

    score.clamp(0.0, 1.0)
}

fn build_samples(
    metadata: &GameMetadata,
    sequences: &[Sequence],
    config: &PipelineConfig,
    split: String,
) -> Vec<TrainingSample> {
    let mut samples = Vec::new();

    for sequence in sequences {
        if sequence.frames.len() <= config.window.length {
            continue;
        }

        for stretched in
            stretch_sequence(&sequence.frames, &config.augmentation.time_stretch_factors)
        {
            if stretched.len() <= config.window.length {
                continue;
            }

            for window_start in
                (0..=stretched.len() - config.window.length - 1).step_by(config.window.stride)
            {
                let window = &stretched[window_start..window_start + config.window.length];
                let next_frame = &stretched[window_start + config.window.length];
                for ego_slot in 0..config.max_team_size {
                    let Some(ego_current) = window
                        .last()
                        .and_then(|frame| frame.target_team.get(ego_slot))
                        .and_then(|slot| slot.as_ref())
                    else {
                        continue;
                    };
                    let Some(ego_next) = next_frame
                        .target_team
                        .get(ego_slot)
                        .and_then(|slot| slot.as_ref())
                    else {
                        continue;
                    };

                    let base_input = build_feature_window(window, ego_slot, metadata.phase, config);
                    let target = [
                        ego_next.x - ego_current.x,
                        ego_next.y - ego_current.y,
                        wrap_angle(ego_next.theta - ego_current.theta),
                    ];
                    let occupancy_grid = if config.augmentation.include_occupancy_grid {
                        Some(build_occupancy_grid(window, config))
                    } else {
                        None
                    };
                    let role_label = ego_current.role;
                    let sample_weight = if matches!(metadata.phase, MatchPhase::Elimination) {
                        config.split.elimination_weight
                    } else {
                        1.0
                    };
                    let metadata = TrainingSampleMetadata {
                        game_id: metadata.game_id.clone(),
                        source_log: metadata.source_log.clone(),
                        phase: metadata.phase,
                        target_team: metadata.target_team.clone(),
                        opponent_team: metadata.opponent_team.clone(),
                        target_color: metadata.target_color,
                        target_score: metadata.target_score,
                        opponent_score: metadata.opponent_score,
                        sequence_index: sequence.index,
                        window_index: window_start,
                        ego_slot,
                        role_label,
                        split: split.clone(),
                        sample_weight,
                        sequence_kind: sequence.kind,
                        timestamp_start_s: window
                            .first()
                            .map(|frame| frame.timestamp_s)
                            .unwrap_or_default(),
                        timestamp_end_s: window
                            .last()
                            .map(|frame| frame.timestamp_s)
                            .unwrap_or_default(),
                        quality_flags: sequence.quality_flags.clone(),
                        quality_score: sequence.quality_score,
                    };

                    samples.push(TrainingSample {
                        input: base_input.clone(),
                        target,
                        occupancy_grid: occupancy_grid.clone(),
                        metadata: metadata.clone(),
                    });

                    if config.augmentation.mirror_y {
                        samples.push(mirror_y_sample(
                            base_input.clone(),
                            target,
                            occupancy_grid.clone(),
                            metadata.clone(),
                        ));
                    }
                    if config.augmentation.mirror_x {
                        samples.push(mirror_x_sample(
                            base_input.clone(),
                            target,
                            occupancy_grid.clone(),
                            metadata.clone(),
                        ));
                    }
                    if config.augmentation.gaussian_noise_std_m > 0.0 {
                        samples.push(noisy_sample(
                            base_input,
                            target,
                            occupancy_grid,
                            metadata,
                            config,
                        ));
                    }
                }
            }
        }
    }

    samples
}

fn stretch_sequence(frames: &[CleanFrame], factors: &[f32]) -> Vec<Vec<CleanFrame>> {
    let mut variants = vec![frames.to_vec()];
    for factor in factors {
        if *factor <= 0.0 {
            continue;
        }
        let target_len = ((frames.len() as f32) * *factor).round().max(2.0) as usize;
        let mut stretched = Vec::with_capacity(target_len);
        for index in 0..target_len {
            let source =
                ((index as f32 / target_len as f32) * frames.len() as f32).floor() as usize;
            stretched.push(frames[source.min(frames.len() - 1)].clone());
        }
        variants.push(stretched);
    }
    variants
}

fn build_feature_window(
    window: &[CleanFrame],
    ego_slot: usize,
    phase: MatchPhase,
    config: &PipelineConfig,
) -> Vec<f32> {
    let mut features = Vec::with_capacity(config.sample_feature_dim());
    let phase_one_hot = phase.one_hot();
    for frame in window {
        let Some(ego) = frame
            .target_team
            .get(ego_slot)
            .and_then(|slot| slot.as_ref())
        else {
            features.extend(std::iter::repeat_n(
                0.0,
                8 + 10 + 4 + 7 + ((config.max_team_size - 1) + config.max_team_size) * 14,
            ));
            continue;
        };
        let ball = frame.ball.as_ref();
        features.extend_from_slice(&normalize_pose(ego, config));
        features.extend_from_slice(&relative_ball_features(ego, ball, config));
        features.extend_from_slice(&phase_one_hot);
        features.extend_from_slice(&score_features(frame));

        for (slot, robot) in frame.target_team.iter().enumerate() {
            if slot == ego_slot {
                continue;
            }
            encode_robot_features(&mut features, ego, robot.as_ref(), config);
        }
        for robot in &frame.opponent_team {
            encode_robot_features(&mut features, ego, robot.as_ref(), config);
        }
    }
    features
}

fn normalize_pose(robot: &EntityState, config: &PipelineConfig) -> [f32; 8] {
    [
        robot.x / config.field_half_length_m,
        robot.y / config.field_half_width_m,
        robot.vx / config.max_speed_m_s,
        robot.vy / config.max_speed_m_s,
        robot.ax / config.max_acceleration_m_s2,
        robot.ay / config.max_acceleration_m_s2,
        robot.theta.sin(),
        robot.theta.cos(),
    ]
}

fn relative_ball_features(
    ego: &EntityState,
    ball: Option<&BallState>,
    config: &PipelineConfig,
) -> [f32; 10] {
    let Some(ball) = ball else {
        return [0.0; 10];
    };
    let dx = ball.x - ego.x;
    let dy = ball.y - ego.y;
    let distance = (dx * dx + dy * dy).sqrt();
    let angle = wrap_angle(dy.atan2(dx) - ego.theta);
    [
        dx / config.field_half_length_m,
        dy / config.field_half_width_m,
        ball.vx / config.max_speed_m_s,
        ball.vy / config.max_speed_m_s,
        ball.ax / config.max_acceleration_m_s2,
        ball.ay / config.max_acceleration_m_s2,
        distance / (config.field_half_length_m * 2.0),
        angle.sin(),
        angle.cos(),
        ball.visibility,
    ]
}

fn score_features(frame: &CleanFrame) -> [f32; 7] {
    let referee = frame.referee.as_ref();
    let blue_score = referee
        .and_then(|value| value.blue_score)
        .unwrap_or_default() as f32;
    let yellow_score = referee
        .and_then(|value| value.yellow_score)
        .unwrap_or_default() as f32;
    [
        blue_score,
        yellow_score,
        frame.flags.carried_ball as u8 as f32,
        frame.flags.duplicate_timestamp as u8 as f32,
        frame.flags.missing_target_robot_slots as f32,
        frame.flags.out_of_bounds_objects as f32,
        frame.live as u8 as f32,
    ]
}

fn encode_robot_features(
    features: &mut Vec<f32>,
    ego: &EntityState,
    robot: Option<&EntityState>,
    config: &PipelineConfig,
) {
    let Some(robot) = robot else {
        features.extend(std::iter::repeat_n(0.0, 14));
        return;
    };
    let dx = robot.x - ego.x;
    let dy = robot.y - ego.y;
    let distance = (dx * dx + dy * dy).sqrt();
    let angle = wrap_angle(dy.atan2(dx) - ego.theta);
    features.extend_from_slice(&[
        dx / config.field_half_length_m,
        dy / config.field_half_width_m,
        robot.vx / config.max_speed_m_s,
        robot.vy / config.max_speed_m_s,
        robot.ax / config.max_acceleration_m_s2,
        robot.ay / config.max_acceleration_m_s2,
        robot.theta.sin(),
        robot.theta.cos(),
        distance / (config.field_half_length_m * 2.0),
        angle.sin(),
        angle.cos(),
        robot.visibility,
        robot.stable_id.unwrap_or_default() as f32 / 15.0,
        role_to_float(robot.role),
    ]);
}

fn build_occupancy_grid(window: &[CleanFrame], config: &PipelineConfig) -> Vec<f32> {
    let channels = 3;
    let plane_len = config.occupancy_grid_width * config.occupancy_grid_height;
    let mut grid = vec![0.0; plane_len * channels];
    for (history_index, frame) in window.iter().enumerate() {
        let intensity = (history_index + 1) as f32 / window.len() as f32;
        for robot in frame.target_team.iter().flatten() {
            stamp_grid(&mut grid, 0, robot.x, robot.y, intensity, config);
        }
        for robot in frame.opponent_team.iter().flatten() {
            stamp_grid(&mut grid, 1, robot.x, robot.y, intensity, config);
        }
        if let Some(ball) = &frame.ball {
            stamp_grid(&mut grid, 2, ball.x, ball.y, intensity, config);
        }
    }
    grid
}

fn stamp_grid(
    grid: &mut [f32],
    channel: usize,
    x: f32,
    y: f32,
    intensity: f32,
    config: &PipelineConfig,
) {
    let nx =
        ((x + config.field_half_length_m) / (config.field_half_length_m * 2.0)).clamp(0.0, 0.9999);
    let ny =
        ((y + config.field_half_width_m) / (config.field_half_width_m * 2.0)).clamp(0.0, 0.9999);
    let gx = (nx * config.occupancy_grid_width as f32) as usize;
    let gy = (ny * config.occupancy_grid_height as f32) as usize;
    let plane_len = config.occupancy_grid_width * config.occupancy_grid_height;
    let index = channel * plane_len + gy * config.occupancy_grid_width + gx;
    grid[index] = grid[index].max(intensity);
}

fn mirror_y_sample(
    mut input: Vec<f32>,
    target: [f32; 3],
    occupancy_grid: Option<Vec<f32>>,
    metadata: TrainingSampleMetadata,
) -> TrainingSample {
    for value in input.iter_mut().skip(1).step_by(8) {
        *value = -*value;
    }
    TrainingSample {
        input,
        target: [target[0], -target[1], -target[2]],
        occupancy_grid,
        metadata,
    }
}

fn mirror_x_sample(
    mut input: Vec<f32>,
    target: [f32; 3],
    occupancy_grid: Option<Vec<f32>>,
    metadata: TrainingSampleMetadata,
) -> TrainingSample {
    for value in input.iter_mut().step_by(8) {
        *value = -*value;
    }
    TrainingSample {
        input,
        target: [-target[0], target[1], -target[2]],
        occupancy_grid,
        metadata,
    }
}

fn noisy_sample(
    mut input: Vec<f32>,
    target: [f32; 3],
    occupancy_grid: Option<Vec<f32>>,
    metadata: TrainingSampleMetadata,
    config: &PipelineConfig,
) -> TrainingSample {
    let mut rng = rng();
    let sigma = (config.augmentation.gaussian_noise_std_m / config.field_half_length_m) as f64;
    if let Ok(normal) = Normal::new(0.0, sigma.max(1e-6)) {
        for value in &mut input {
            *value += normal.sample(&mut rng) as f32;
        }
    }
    TrainingSample {
        input,
        target,
        occupancy_grid,
        metadata,
    }
}

fn assign_splits(raws: &[RawGame], config: &PipelineConfig) -> Vec<SplitAssignment> {
    let mut sorted = raws
        .iter()
        .map(|raw| raw.metadata.clone())
        .collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.game_id.cmp(&right.game_id));
    let total = sorted.len().max(1);
    let train_cut = ((total as f32) * config.split.train_ratio).round() as usize;
    let val_cut = train_cut + ((total as f32) * config.split.validation_ratio).round() as usize;
    sorted
        .into_iter()
        .enumerate()
        .map(|(index, metadata)| SplitAssignment {
            game_id: metadata.game_id,
            split: if index < train_cut {
                "train"
            } else if index < val_cut {
                "validation"
            } else {
                "test"
            }
            .to_string(),
            phase: metadata.phase,
            sample_count: 0,
        })
        .collect()
}

fn compute_normalization_stats(samples: &[TrainingSample]) -> NormalizationStats {
    let mut values = Vec::new();
    for sample in samples {
        for chunk in sample.input.chunks(8) {
            if let [_, _, vx, vy, ax, ay, _, _] = chunk {
                values.push((vx * vx + vy * vy).sqrt());
                values.push((ax * ax + ay * ay).sqrt());
            }
        }
    }
    compute_stats_from_values(&values)
}

fn compute_normalization_stats_from_sequences(sequences: &[Sequence]) -> NormalizationStats {
    let mut values = Vec::new();
    for sequence in sequences {
        for frame in &sequence.frames {
            for robot in frame
                .target_team
                .iter()
                .chain(frame.opponent_team.iter())
                .flatten()
            {
                values.push((robot.vx * robot.vx + robot.vy * robot.vy).sqrt());
                values.push((robot.ax * robot.ax + robot.ay * robot.ay).sqrt());
            }
        }
    }
    compute_stats_from_values(&values)
}

fn compute_stats_from_values(values: &[f32]) -> NormalizationStats {
    if values.is_empty() {
        return NormalizationStats {
            velocity_mean: 0.0,
            velocity_std: 1.0,
            acceleration_mean: 0.0,
            acceleration_std: 1.0,
            sample_count: 0,
        };
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let variance = values
        .iter()
        .map(|value| {
            let delta = *value - mean;
            delta * delta
        })
        .sum::<f32>()
        / values.len() as f32;
    let std = variance.sqrt().max(1e-6);
    NormalizationStats {
        velocity_mean: mean,
        velocity_std: std,
        acceleration_mean: mean,
        acceleration_std: std,
        sample_count: values.len(),
    }
}

fn apply_split_weights(outputs: &mut [PipelineOutput], bundle: &SplitBundle) {
    let split_map = bundle
        .assignments
        .iter()
        .map(|entry| (entry.game_id.clone(), entry.split.clone()))
        .collect::<BTreeMap<_, _>>();
    for output in outputs {
        let split = split_map
            .get(&output.metadata.game_id)
            .cloned()
            .unwrap_or_else(|| "train".to_string());
        for sample in &mut output.samples {
            sample.metadata.split = split.clone();
            sample.metadata.sample_weight =
                if matches!(sample.metadata.phase, MatchPhase::Elimination) {
                    bundle.elimination_sample_weight
                } else {
                    1.0
                };
        }
    }
}

fn role_to_float(role: RoleLabel) -> f32 {
    match role {
        RoleLabel::Goalkeeper => 0.0,
        RoleLabel::Defender => 0.25,
        RoleLabel::Midfielder => 0.5,
        RoleLabel::Forward => 0.75,
        RoleLabel::Unknown => 1.0,
    }
}

fn wrap_angle(value: f32) -> f32 {
    let mut angle = value;
    while angle > PI {
        angle -= 2.0 * PI;
    }
    while angle < -PI {
        angle += 2.0 * PI;
    }
    angle
}

pub fn summarize_games_by_phase(outputs: &[PipelineOutput]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for output in outputs {
        let key = match output.metadata.phase {
            MatchPhase::Group => "group",
            MatchPhase::Elimination => "elimination",
            MatchPhase::Friendly => "friendly",
            MatchPhase::Unknown => "unknown",
        };
        *counts.entry(key.to_string()).or_default() += 1;
    }
    counts
}
