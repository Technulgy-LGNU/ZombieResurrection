use anyhow::{bail, Result};
use tch::{Device, Kind, Tensor};

use crate::config::PipelineConfig;
use crate::types::TrainingSample;
use crate::types::TrainingSampleMetadata;

#[derive(Debug)]
pub struct SequenceTensorBatch {
    pub inputs: Tensor,
    pub targets: Tensor,
    pub weights: Tensor,
    pub mask: Tensor,
    pub occupancy_grids: Option<Tensor>,
    pub metadata: Vec<TrainingSampleMetadata>,
}

pub fn per_timestep_feature_dim(config: &PipelineConfig) -> usize {
    config.sample_feature_dim() / config.window.length.max(1)
}

pub fn samples_to_tensors(samples: &[TrainingSample], device: Device) -> Result<(Tensor, Tensor)> {
    let batch = samples.len();
    let input_dim = samples
        .first()
        .map(|sample| sample.input.len())
        .unwrap_or_default();
    let mut inputs = Vec::with_capacity(batch * input_dim);
    let mut targets = Vec::with_capacity(batch * 3);

    for sample in samples {
        inputs.extend_from_slice(&sample.input);
        targets.extend_from_slice(&sample.target);
    }

    let xs = Tensor::f_from_slice(&inputs)?
        .view([batch as i64, input_dim as i64])
        .to_device(device);
    let ys = Tensor::f_from_slice(&targets)?
        .view([batch as i64, 3])
        .to_device(device);
    Ok((xs, ys))
}

pub fn samples_to_weighted_tensors(
    samples: &[TrainingSample],
    device: Device,
) -> Result<(Tensor, Tensor, Tensor)> {
    let batch = samples.len();
    let input_dim = samples
        .first()
        .map(|sample| sample.input.len())
        .unwrap_or_default();
    let mut inputs = Vec::with_capacity(batch * input_dim);
    let mut targets = Vec::with_capacity(batch * 3);
    let mut weights = Vec::with_capacity(batch);

    for sample in samples {
        inputs.extend_from_slice(&sample.input);
        targets.extend_from_slice(&sample.target);
        weights.push(sample.metadata.sample_weight);
    }

    let xs = Tensor::f_from_slice(&inputs)?
        .view([batch as i64, input_dim as i64])
        .to_device(device);
    let ys = Tensor::f_from_slice(&targets)?
        .view([batch as i64, 3])
        .to_device(device);
    let ws = Tensor::f_from_slice(&weights)?
        .view([batch as i64, 1])
        .to_device(device);
    Ok((xs, ys, ws))
}

pub fn samples_to_sequence_tensors(
    samples: &[TrainingSample],
    config: &PipelineConfig,
    device: Device,
) -> Result<SequenceTensorBatch> {
    let batch = samples.len();
    let time = config.window.length;
    let step_dim = per_timestep_feature_dim(config);
    let input_dim = time * step_dim;
    let mut inputs = Vec::with_capacity(batch * input_dim);
    let mut targets = Vec::with_capacity(batch * 3);
    let mut weights = Vec::with_capacity(batch);
    let mut metadata = Vec::with_capacity(batch);

    let occupancy_expected = samples.iter().any(|sample| sample.occupancy_grid.is_some());
    let occupancy_plane = 3 * config.occupancy_grid_width * config.occupancy_grid_height;
    let mut occupancy_values = if occupancy_expected {
        Vec::with_capacity(batch * occupancy_plane)
    } else {
        Vec::new()
    };

    for sample in samples {
        if sample.input.len() != input_dim {
            bail!(
                "sample input length {} does not match expected {} (time {} * step {})",
                sample.input.len(),
                input_dim,
                time,
                step_dim
            );
        }

        inputs.extend_from_slice(&sample.input);
        targets.extend_from_slice(&sample.target);
        weights.push(sample.metadata.sample_weight);
        metadata.push(sample.metadata.clone());

        if occupancy_expected {
            let Some(grid) = &sample.occupancy_grid else {
                bail!("mixed occupancy grid presence in sample batch");
            };
            if grid.len() != occupancy_plane {
                bail!(
                    "occupancy grid length {} does not match expected {}",
                    grid.len(),
                    occupancy_plane
                );
            }
            occupancy_values.extend_from_slice(grid);
        }
    }

    let inputs = Tensor::f_from_slice(&inputs)?
        .view([batch as i64, time as i64, step_dim as i64])
        .to_device(device);
    let targets = Tensor::f_from_slice(&targets)?
        .view([batch as i64, 3])
        .to_device(device);
    let weights = Tensor::f_from_slice(&weights)?
        .view([batch as i64, 1])
        .to_device(device);
    let mask = Tensor::ones([batch as i64, time as i64], (Kind::Float, device));
    let occupancy_grids = if occupancy_expected {
        Some(
            Tensor::f_from_slice(&occupancy_values)?
                .view([
                    batch as i64,
                    3,
                    config.occupancy_grid_height as i64,
                    config.occupancy_grid_width as i64,
                ])
                .to_device(device),
        )
    } else {
        None
    };

    Ok(SequenceTensorBatch {
        inputs,
        targets,
        weights,
        mask,
        occupancy_grids,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::samples_to_sequence_tensors;
    use crate::{
        MatchPhase, PipelineConfig, RoleLabel, SequenceKind, TeamColor, TrainingSample,
        TrainingSampleMetadata,
    };
    use tch::Device;

    #[test]
    fn sequence_tensors_follow_window_shape() {
        let mut config = PipelineConfig::default();
        config.window.length = 2;

        let sample = TrainingSample {
            input: vec![0.5; config.sample_feature_dim()],
            target: [0.1, 0.2, 0.3],
            occupancy_grid: None,
            metadata: TrainingSampleMetadata {
                game_id: "g".into(),
                source_log: "log".into(),
                phase: MatchPhase::Unknown,
                target_team: "a".into(),
                opponent_team: "b".into(),
                target_color: TeamColor::Blue,
                target_score: 0,
                opponent_score: 0,
                sequence_index: 0,
                window_index: 0,
                ego_slot: 0,
                role_label: RoleLabel::Unknown,
                split: "train".into(),
                sample_weight: 1.0,
                sequence_kind: SequenceKind::Unknown,
                timestamp_start_s: 0.0,
                timestamp_end_s: 1.0,
                quality_flags: Vec::new(),
                quality_score: 1.0,
            },
        };

        let batch = samples_to_sequence_tensors(&[sample], &config, Device::Cpu).unwrap();
        assert_eq!(
            batch.inputs.size(),
            vec![1, 2, (config.sample_feature_dim() / 2) as i64]
        );
        assert_eq!(batch.targets.size(), vec![1, 3]);
        assert_eq!(batch.mask.size(), vec![1, 2]);
        assert_eq!(batch.metadata.len(), 1);
    }
}
