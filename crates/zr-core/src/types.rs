use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub enum TeamColor {
    Yellow,
    Blue,
}

impl TeamColor {
    pub fn opponent(self) -> Self {
        match self {
            Self::Yellow => Self::Blue,
            Self::Blue => Self::Yellow,
        }
    }

    pub fn from_proto(value: Option<i32>) -> Option<Self> {
        match value {
            Some(1) => Some(Self::Yellow),
            Some(2) => Some(Self::Blue),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Blue => "blue",
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub enum MatchPhase {
    Group,
    Elimination,
    Friendly,
    Unknown,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub enum RoleLabel {
    Goalkeeper,
    Defender,
    Midfielder,
    Forward,
    Unknown,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub enum SequenceKind {
    OpenPlay,
    SetPiece,
    Transition,
    Unknown,
}

impl MatchPhase {
    pub fn one_hot(self) -> [f32; 4] {
        match self {
            Self::Group => [1.0, 0.0, 0.0, 0.0],
            Self::Elimination => [0.0, 1.0, 0.0, 0.0],
            Self::Friendly => [0.0, 0.0, 1.0, 0.0],
            Self::Unknown => [0.0, 0.0, 0.0, 1.0],
        }
    }
}

impl Default for MatchPhase {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct GameMetadata {
    pub game_id: String,
    pub source_log: String,
    pub year: Option<u16>,
    pub phase: MatchPhase,
    pub target_team: String,
    pub opponent_team: String,
    pub target_color: TeamColor,
    pub target_score: u32,
    pub opponent_score: u32,
    pub sample_rate_hz: f32,
    pub duration_s: f64,
    pub tracker_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct AuditSummary {
    pub total_messages: usize,
    pub tracker_frames_seen: usize,
    pub tracker_frames_used: usize,
    pub duplicate_frames: usize,
    pub out_of_bounds_objects: usize,
    pub missing_ball_frames: usize,
    pub distinct_tracker_sources: Vec<String>,
    pub sample_rate_hz: f32,
    pub target_team_resolved: String,
    pub notes: Vec<String>,
    pub suspicious_identity_swaps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct EntityState {
    pub raw_id: Option<u32>,
    pub stable_id: Option<u32>,
    pub role: RoleLabel,
    pub x: f32,
    pub y: f32,
    pub theta: f32,
    pub vx: f32,
    pub vy: f32,
    pub omega: f32,
    pub ax: f32,
    pub ay: f32,
    pub visibility: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct BallState {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub ax: f32,
    pub ay: f32,
    pub visibility: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct RefereeSnapshot {
    pub stage: Option<i32>,
    pub command: Option<i32>,
    pub blue_team_on_positive_half: Option<bool>,
    pub match_type: Option<i32>,
    pub yellow_name: Option<String>,
    pub blue_name: Option<String>,
    pub yellow_score: Option<u32>,
    pub blue_score: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct FrameFlags {
    pub duplicate_timestamp: bool,
    pub carried_ball: bool,
    pub out_of_bounds_objects: usize,
    pub missing_target_robot_slots: usize,
    pub likely_identity_swap: bool,
    pub referee_live: bool,
    pub heuristic_live: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct CleanFrame {
    pub timestamp_s: f64,
    pub frame_number: u32,
    pub ball: Option<BallState>,
    pub target_team: Vec<Option<EntityState>>,
    pub opponent_team: Vec<Option<EntityState>>,
    pub referee: Option<RefereeSnapshot>,
    pub live: bool,
    pub target_attacks_positive_x: bool,
    pub sequence_kind: SequenceKind,
    pub flags: FrameFlags,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct ReviewSequenceSummary {
    pub sequence_index: usize,
    pub start_frame: u32,
    pub end_frame: u32,
    pub start_time_s: f64,
    pub end_time_s: f64,
    pub frame_count: usize,
    pub quality_score: f32,
    pub sequence_kind: SequenceKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct TrainingSampleMetadata {
    pub game_id: String,
    pub source_log: String,
    pub phase: MatchPhase,
    pub target_team: String,
    pub opponent_team: String,
    pub target_color: TeamColor,
    pub target_score: u32,
    pub opponent_score: u32,
    pub sequence_index: usize,
    pub window_index: usize,
    pub ego_slot: usize,
    pub role_label: RoleLabel,
    pub split: String,
    pub sample_weight: f32,
    pub sequence_kind: SequenceKind,
    pub timestamp_start_s: f64,
    pub timestamp_end_s: f64,
    pub quality_flags: Vec<String>,
    pub quality_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
pub struct TrainingSample {
    pub input: Vec<f32>,
    pub target: [f32; 3],
    pub occupancy_grid: Option<Vec<f32>>,
    pub metadata: TrainingSampleMetadata,
}
