use anyhow::Result;
use tch::{Device, Tensor};

use crate::types::TrainingSample;

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
