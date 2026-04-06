use serde::{Deserialize, Serialize};

use crate::types::TeamColor;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TeamSelector {
    Color(TeamColor),
    Name(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    pub length: usize,
    pub stride: usize,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            length: 16,
            stride: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AugmentationConfig {
    pub mirror_y: bool,
    pub mirror_x: bool,
    pub gaussian_noise_std_m: f32,
    pub time_stretch_factors: Vec<f32>,
    pub include_occupancy_grid: bool,
}

impl Default for AugmentationConfig {
    fn default() -> Self {
        Self {
            mirror_y: true,
            mirror_x: false,
            gaussian_noise_std_m: 0.01,
            time_stretch_factors: Vec::new(),
            include_occupancy_grid: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitConfig {
    pub train_ratio: f32,
    pub validation_ratio: f32,
    pub test_ratio: f32,
    pub elimination_weight: f32,
}

impl Default for SplitConfig {
    fn default() -> Self {
        Self {
            train_ratio: 0.7,
            validation_ratio: 0.15,
            test_ratio: 0.15,
            elimination_weight: 1.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    pub max_match_distance_m: f32,
    pub max_unmatched_frames: usize,
    pub jump_penalty_m: f32,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            max_match_distance_m: 0.9,
            max_unmatched_frames: 8,
            jump_penalty_m: 1.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivePlayConfig {
    pub referee_live_commands: Vec<i32>,
    pub min_ball_speed_m_s: f32,
    pub min_robot_speed_m_s: f32,
    pub grace_frames_after_live_command: usize,
}

impl Default for LivePlayConfig {
    fn default() -> Self {
        Self {
            referee_live_commands: vec![2, 3],
            min_ball_speed_m_s: 0.05,
            min_robot_speed_m_s: 0.05,
            grace_frames_after_live_command: 12,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub target_team: TeamSelector,
    pub tracker_source: Option<String>,
    pub field_half_length_m: f32,
    pub field_half_width_m: f32,
    pub out_of_bounds_margin_m: f32,
    pub max_team_size: usize,
    pub max_speed_m_s: f32,
    pub max_acceleration_m_s2: f32,
    pub max_angular_speed_rad_s: f32,
    pub min_sequence_frames: usize,
    pub max_sequence_frames: usize,
    pub min_possession_frames: usize,
    pub max_frame_gap_s: f64,
    pub possession_radius_m: f32,
    pub carried_ball_frames: usize,
    pub window: WindowConfig,
    pub augmentation: AugmentationConfig,
    pub split: SplitConfig,
    pub identity: IdentityConfig,
    pub live_play: LivePlayConfig,
    pub occupancy_grid_width: usize,
    pub occupancy_grid_height: usize,
}

impl PipelineConfig {
    pub fn sample_feature_dim(&self) -> usize {
        let per_robot = 14;
        let robot_count = (self.max_team_size - 1) + self.max_team_size;
        let per_timestep = 8 + 10 + 4 + 7 + robot_count * per_robot;
        per_timestep * self.window.length
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            target_team: TeamSelector::Color(TeamColor::Blue),
            tracker_source: None,
            field_half_length_m: 4.5,
            field_half_width_m: 3.0,
            out_of_bounds_margin_m: 0.2,
            max_team_size: 6,
            max_speed_m_s: 4.0,
            max_acceleration_m_s2: 3.0,
            max_angular_speed_rad_s: 12.0,
            min_sequence_frames: 50,
            max_sequence_frames: 300,
            min_possession_frames: 8,
            max_frame_gap_s: 0.25,
            possession_radius_m: 0.6,
            carried_ball_frames: 3,
            window: WindowConfig::default(),
            augmentation: AugmentationConfig::default(),
            split: SplitConfig::default(),
            identity: IdentityConfig::default(),
            live_play: LivePlayConfig::default(),
            occupancy_grid_width: 40,
            occupancy_grid_height: 28,
        }
    }
}
