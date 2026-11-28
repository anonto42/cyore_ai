// The training loop: forward pass, loss, backward pass (Burn's autodiff), Adam step.
// After each epoch the model is scored on the validation split (held out from the generated
// training data) and on the hand-written development set. The epoch kept is the one with the
// best development exact match, and training stops once that stops improving (design.md,
// decisions 6 and 13).
//
// Why development, not validation: validation comes from the same grammar as training and
// scores ~99% on every epoch, so it cannot tell epochs apart. Why exact match (intent AND every
// slot tag right): that is what yields a correct command; intent accuracy alone peaks long
// before the slot tagger has learned anything.

use burn::nn::loss::{CrossEntropyLoss, CrossEntropyLossConfig};
use burn::optim::{AdamConfig, GradientsParams};
use burn::prelude::*;

use crate::batch::{Encoded, bag_of_words, batches, make_batch};
use crate::model::{BagOfWords, JointModel, ModelConfig};
use crate::random::Rng;

#[derive(Debug, Clone, PartialEq)]
pub struct TrainConfig {
    pub epochs: usize,
    pub batch_size: usize,
    pub learning_rate: f64,
    pub word_dropout: f64,
    /// Stop after this many epochs without a better validation accuracy.
    pub patience: usize,
    pub seed: u64,
}

impl Default for TrainConfig {
    fn default() -> Self {
        TrainConfig {
            epochs: 40,
            batch_size: 32,
            learning_rate: 2e-3,
            word_dropout: 0.05,
            patience: 8,
            seed: 42,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EpochLog {
    pub epoch: usize,
    /// Mean training loss per example over the epoch.
    pub loss: f64,
    pub val_intent_accuracy: f64,
    pub dev_intent_accuracy: f64,
    /// Development sentences with the intent and all slot tags right: the selection score.
    /// Equals intent accuracy for the baseline, which has no slots.
    pub dev_exact: f64,
}

pub struct Trained<M> {
    /// The best epoch's model, already switched to inference mode (no autodiff).
    pub model: M,
    pub best_epoch: usize,
    pub best: EpochLog,
    pub log: Vec<EpochLog>,
}

/// The device training runs on: Burn's pure-Rust CPU backend with gradients enabled.
fn training_device(seed: u64) -> Device {
    let device = Device::flex().autodiff();
    // Seeds Burn's own randomness (weight initialisation).
    device.seed(seed);
    device
}

fn correct(predicted: Tensor<2>, targets: Tensor<1, Int>) -> usize {
    let predicted: Vec<i64> = predicted.argmax(1).into_data().iter::<i64>().collect();
    let targets: Vec<i64> = targets.into_data().iter::<i64>().collect();
    predicted
        .iter()
        .zip(&targets)
        .filter(|(p, t)| p == t)
        .count()
}

/// (intent accuracy, exact-match accuracy) of the joint model on `data`.
pub fn joint_accuracy(model: &JointModel, data: &[Encoded], batch_size: usize) -> (f64, f64) {
    let device = Device::flex();
    let (mut intent_right, mut exact_right) = (0, 0);
    for indices in batches(data, batch_size, None) {
        let batch = make_batch(data, &indices, &device, None);
        let (intent_logits, slot_logits) = model.forward(batch.ids);
        let intents: Vec<i64> = intent_logits.argmax(1).into_data().iter::<i64>().collect();
        let tags: Vec<i64> = slot_logits.argmax(2).into_data().iter::<i64>().collect();
        let len = data[indices[0]].ids.len();
        for (row, &i) in indices.iter().enumerate() {
            let e = &data[i];
            let intent_ok = intents[row] == e.intent as i64;
            let tags_ok = e
                .tags
                .iter()
                .zip(&tags[row * len..(row + 1) * len])
                .all(|(&gold, &pred)| gold as i64 == pred);
            intent_right += intent_ok as usize;
            exact_right += (intent_ok && tags_ok) as usize;
        }
    }
    let n = data.len().max(1) as f64;
    (intent_right as f64 / n, exact_right as f64 / n)
}

/// Fraction of examples whose intent the baseline gets right.
pub fn bow_accuracy(model: &BagOfWords, data: &[Encoded], vocab_size: usize) -> f64 {
    let device = Device::flex();
    let indices: Vec<usize> = (0..data.len()).collect();
    let mut right = 0;
    for chunk in indices.chunks(256) {
        let counts = bag_of_words(data, chunk, vocab_size, &device);
        let targets: Vec<i64> = chunk.iter().map(|&i| data[i].intent as i64).collect();
        let targets = Tensor::from_data(TensorData::new(targets, [chunk.len()]), &device);
        right += correct(model.forward(counts), targets);
    }
    right as f64 / data.len().max(1) as f64
}

/// Keep the best model so far; returns true when training should stop early.
fn track_best<M: Module>(
    best: &mut Option<Trained<M>>,
    model: &M,
    log: EpochLog,
    patience: usize,
) -> bool {
    // Ties go to the later epoch: same score, more training.
    let improved = best
        .as_ref()
        .is_none_or(|b| log.dev_exact >= b.best.dev_exact);
    let epoch = log.epoch;
    match best {
        Some(b) if !improved => {
            b.log.push(log);
            epoch - b.best_epoch >= patience
        }
        _ => {
            let mut history = best.take().map(|b| b.log).unwrap_or_default();
            history.push(log.clone());
            *best = Some(Trained {
                model: model.valid(),
                best_epoch: epoch,
                best: log,
                log: history,
            });
            false
        }
    }
}

pub fn train_joint(
    config: &ModelConfig,
    train: &[Encoded],
    val: &[Encoded],
    dev: &[Encoded],
    tc: &TrainConfig,
    mut on_epoch: impl FnMut(&EpochLog),
) -> Trained<JointModel> {
    let device = training_device(tc.seed);
    let mut model = JointModel::new(config, &device);
    let mut optimizer = AdamConfig::new().init();
    let loss_fn: CrossEntropyLoss = CrossEntropyLossConfig::new().init(&device);
    let mut rng = Rng::new(tc.seed);
    let mut best = None;

    for epoch in 1..=tc.epochs {
        let mut total_loss = 0.0;
        for indices in batches(train, tc.batch_size, Some(&mut rng)) {
            let batch = make_batch(train, &indices, &device, Some((tc.word_dropout, &mut rng)));
            let (intent_logits, slot_logits) = model.forward(batch.ids);
            let [b, n, n_tags] = slot_logits.dims();
            // L = CE(intent) + mean over words of CE(tag): one sentence loss, one per word.
            let loss = loss_fn.forward(intent_logits, batch.intents)
                + loss_fn.forward(slot_logits.reshape([b * n, n_tags]), batch.tags);
            total_loss += loss.clone().into_scalar::<f32>() as f64 * b as f64;

            let grads = GradientsParams::from_grads(loss.backward(), &model);
            model = optimizer.step(tc.learning_rate, model, grads);
        }
        let inference = model.valid();
        let (val_intent_accuracy, _) = joint_accuracy(&inference, val, tc.batch_size);
        let (dev_intent_accuracy, dev_exact) = joint_accuracy(&inference, dev, tc.batch_size);
        let log = EpochLog {
            epoch,
            loss: total_loss / train.len().max(1) as f64,
            val_intent_accuracy,
            dev_intent_accuracy,
            dev_exact,
        };
        on_epoch(&log);
        if track_best(&mut best, &model, log, tc.patience) {
            break;
        }
    }
    best.expect("at least one epoch runs")
}

pub fn train_bow(
    vocab_size: usize,
    n_intents: usize,
    train: &[Encoded],
    val: &[Encoded],
    dev: &[Encoded],
    tc: &TrainConfig,
) -> Trained<BagOfWords> {
    let device = training_device(tc.seed);
    let mut model = BagOfWords::new(vocab_size, 64, n_intents, &device);
    let mut optimizer = AdamConfig::new().init();
    let loss_fn: CrossEntropyLoss = CrossEntropyLossConfig::new().init(&device);
    let mut rng = Rng::new(tc.seed);
    let mut order: Vec<usize> = (0..train.len()).collect();
    let mut best = None;

    for epoch in 1..=tc.epochs {
        rng.shuffle(&mut order);
        let mut total_loss = 0.0;
        for chunk in order.chunks(tc.batch_size) {
            let counts = bag_of_words(train, chunk, vocab_size, &device);
            let targets: Vec<i64> = chunk.iter().map(|&i| train[i].intent as i64).collect();
            let targets = Tensor::from_data(TensorData::new(targets, [chunk.len()]), &device);
            let loss = loss_fn.forward(model.forward(counts), targets);
            total_loss += loss.clone().into_scalar::<f32>() as f64 * chunk.len() as f64;
            let grads = GradientsParams::from_grads(loss.backward(), &model);
            model = optimizer.step(tc.learning_rate, model, grads);
        }
        let inference = model.valid();
        let dev_accuracy = bow_accuracy(&inference, dev, vocab_size);
        let log = EpochLog {
            epoch,
            loss: total_loss / train.len().max(1) as f64,
            val_intent_accuracy: bow_accuracy(&inference, val, vocab_size),
            dev_intent_accuracy: dev_accuracy,
            dev_exact: dev_accuracy,
        };
        if track_best(&mut best, &model, log, tc.patience) {
            break;
        }
    }
    best.expect("at least one epoch runs")
}
