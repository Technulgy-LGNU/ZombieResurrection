use serde::{Deserialize, Serialize};

use crate::pipeline::PipelineOutput;
use crate::review::{ReviewStore, ReviewVerdict};
use crate::types::{CleanFrame, ReviewSequenceSummary, TeamColor};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSequenceListItem {
    pub summary: ReviewSequenceSummary,
    pub verdict: ReviewVerdict,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFramePayload {
    pub timestamp_s: f64,
    pub frame_number: u32,
    pub live: bool,
    pub target_attacks_positive_x: bool,
    pub target_team: Vec<ReviewRobotPayload>,
    pub opponent_team: Vec<ReviewRobotPayload>,
    pub ball: Option<ReviewBallPayload>,
    pub flags: ReviewFlagsPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRobotPayload {
    pub slot: usize,
    pub raw_id: Option<u32>,
    pub stable_id: Option<u32>,
    pub role: String,
    pub x: f32,
    pub y: f32,
    pub theta: f32,
    pub vx: f32,
    pub vy: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewBallPayload {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFlagsPayload {
    pub duplicate_timestamp: bool,
    pub carried_ball: bool,
    pub likely_identity_swap: bool,
    pub referee_live: bool,
    pub heuristic_live: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSequencePayload {
    pub summary: ReviewSequenceSummary,
    pub verdict: ReviewVerdict,
    pub note: String,
    pub warnings: Vec<String>,
    pub cleaned_frames: Vec<ReviewFramePayload>,
    pub raw_frames: Vec<ReviewFramePayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewGamePayload {
    pub game_id: String,
    pub source_log: String,
    pub target_team: String,
    pub opponent_team: String,
    pub target_color: TeamColor,
    pub phase: String,
    pub audit_notes: Vec<String>,
    pub sequences: Vec<ReviewSequenceListItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSequenceQueryPayload {
    pub game_id: String,
    pub source_log: String,
    pub sequence: ReviewSequencePayload,
}

pub fn build_review_payload(
    output: &PipelineOutput,
    _raw_frames: &[CleanFrame],
    review: &ReviewStore,
) -> ReviewGamePayload {
    let sequences = output
        .review_game
        .sequence_summaries
        .iter()
        .map(|summary| {
            let sequence_index = summary.sequence_index;
            ReviewSequenceListItem {
                summary: summary.clone(),
                verdict: review.verdict_for(&output.metadata.game_id, sequence_index),
                note: review.note_for(&output.metadata.game_id, sequence_index),
            }
        })
        .collect();

    ReviewGamePayload {
        game_id: output.metadata.game_id.clone(),
        source_log: output.metadata.source_log.clone(),
        target_team: output.metadata.target_team.clone(),
        opponent_team: output.metadata.opponent_team.clone(),
        target_color: output.metadata.target_color,
        phase: format!("{:?}", output.metadata.phase),
        audit_notes: output.audit.notes.clone(),
        sequences,
    }
}

pub fn build_review_sequence_payload(
    output: &PipelineOutput,
    raw_frames: &[CleanFrame],
    review: &ReviewStore,
    sequence_index: usize,
) -> Option<ReviewSequenceQueryPayload> {
    let summary = output
        .review_game
        .sequence_summaries
        .iter()
        .find(|summary| summary.sequence_index == sequence_index)?;
    let cleaned_frames = frames_for_range(
        &output.review_game.frames,
        summary.start_frame,
        summary.end_frame,
    )
    .into_iter()
    .map(frame_to_payload)
    .collect::<Vec<_>>();
    let raw_frames = frames_for_range(raw_frames, summary.start_frame, summary.end_frame)
        .into_iter()
        .map(frame_to_payload)
        .collect::<Vec<_>>();
    Some(ReviewSequenceQueryPayload {
        game_id: output.metadata.game_id.clone(),
        source_log: output.metadata.source_log.clone(),
        sequence: ReviewSequencePayload {
            summary: summary.clone(),
            verdict: review.verdict_for(&output.metadata.game_id, sequence_index),
            note: review.note_for(&output.metadata.game_id, sequence_index),
            warnings: summary.warnings.clone(),
            cleaned_frames,
            raw_frames,
        },
    })
}

fn frames_for_range(frames: &[CleanFrame], start_frame: u32, end_frame: u32) -> Vec<&CleanFrame> {
    frames
        .iter()
        .filter(|frame| frame.frame_number >= start_frame && frame.frame_number <= end_frame)
        .collect()
}

fn frame_to_payload(frame: &CleanFrame) -> ReviewFramePayload {
    ReviewFramePayload {
        timestamp_s: frame.timestamp_s,
        frame_number: frame.frame_number,
        live: frame.live,
        target_attacks_positive_x: frame.target_attacks_positive_x,
        target_team: frame
            .target_team
            .iter()
            .enumerate()
            .filter_map(|(slot, robot)| robot.as_ref().map(|robot| robot_to_payload(slot, robot)))
            .collect(),
        opponent_team: frame
            .opponent_team
            .iter()
            .enumerate()
            .filter_map(|(slot, robot)| robot.as_ref().map(|robot| robot_to_payload(slot, robot)))
            .collect(),
        ball: frame.ball.as_ref().map(|ball| ReviewBallPayload {
            x: ball.x,
            y: ball.y,
            vx: ball.vx,
            vy: ball.vy,
        }),
        flags: ReviewFlagsPayload {
            duplicate_timestamp: frame.flags.duplicate_timestamp,
            carried_ball: frame.flags.carried_ball,
            likely_identity_swap: frame.flags.likely_identity_swap,
            referee_live: frame.flags.referee_live,
            heuristic_live: frame.flags.heuristic_live,
        },
    }
}

fn robot_to_payload(slot: usize, robot: &crate::types::EntityState) -> ReviewRobotPayload {
    ReviewRobotPayload {
        slot,
        raw_id: robot.raw_id,
        stable_id: robot.stable_id,
        role: format!("{:?}", robot.role),
        x: robot.x,
        y: robot.y,
        theta: robot.theta,
        vx: robot.vx,
        vy: robot.vy,
    }
}
