// The networks (math in design.md, "The Math").
//
// JointModel: embedding → bidirectional GRU → two heads.
//   intent head: mean of the GRU outputs over the sentence → linear → (softmax) one of N intents
//   slot head:   each word's GRU output → linear → (softmax) one of T tags
// BagOfWords: the baseline. Word counts → linear → ReLU → linear, intent only.
//
// The heads return raw scores (logits); softmax is applied by the loss during training and by
// `Predictor` at prediction time.

use burn::nn::{BiGru, BiGruConfig, Embedding, EmbeddingConfig, Linear, LinearConfig, Relu};
use burn::prelude::*;
use serde::{Deserialize, Serialize};

/// Sizes needed to rebuild the network before loading saved weights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelConfig {
    pub vocab_size: usize,
    pub n_intents: usize,
    pub n_tags: usize,
    pub d_embed: usize,
    pub d_hidden: usize,
}

#[derive(Module, Debug)]
pub struct JointModel {
    embed: Embedding,
    encoder: BiGru,
    intent_head: Linear,
    slot_head: Linear,
}

impl JointModel {
    pub fn new(config: &ModelConfig, device: &Device) -> Self {
        // Each GRU direction has d_hidden units; the two are concatenated.
        let d_out = 2 * config.d_hidden;
        JointModel {
            embed: EmbeddingConfig::new(config.vocab_size, config.d_embed).init(device),
            encoder: BiGruConfig::new(config.d_embed, config.d_hidden, true).init(device),
            intent_head: LinearConfig::new(d_out, config.n_intents).init(device),
            slot_head: LinearConfig::new(d_out, config.n_tags).init(device),
        }
    }

    /// ids: [batch, len] → (intent logits [batch, n_intents], slot logits [batch, len, n_tags])
    pub fn forward(&self, ids: Tensor<2, Int>) -> (Tensor<2>, Tensor<3>) {
        let embedded = self.embed.forward(ids); // [b, n, d_embed]
        let (states, _) = self.encoder.forward(embedded, None); // [b, n, 2·d_hidden]
        // Mean pooling: one vector for the whole sentence. Batches have no padding
        // (equal-length buckets), so a plain mean is exact.
        let sentence = states.clone().mean_dim(1).squeeze_dim::<2>(1); // [b, 2·d_hidden]
        (
            self.intent_head.forward(sentence),
            self.slot_head.forward(states),
        )
    }
}

#[derive(Module, Debug)]
pub struct BagOfWords {
    hidden: Linear,
    relu: Relu,
    output: Linear,
}

impl BagOfWords {
    pub fn new(vocab_size: usize, d_hidden: usize, n_intents: usize, device: &Device) -> Self {
        BagOfWords {
            hidden: LinearConfig::new(vocab_size, d_hidden).init(device),
            relu: Relu::new(),
            output: LinearConfig::new(d_hidden, n_intents).init(device),
        }
    }

    /// counts: [batch, vocab] → intent logits [batch, n_intents]
    pub fn forward(&self, counts: Tensor<2>) -> Tensor<2> {
        self.output
            .forward(self.relu.forward(self.hidden.forward(counts)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn::tensor::activation::softmax;

    #[test]
    fn test_joint_output_shapes() {
        let device = Device::flex();
        let config = ModelConfig {
            vocab_size: 20,
            n_intents: 51,
            n_tags: 8,
            d_embed: 8,
            d_hidden: 8,
        };
        let model = JointModel::new(&config, &device);
        let ids = Tensor::<2, Int>::from_data(
            TensorData::new(vec![1i64, 2, 3, 4, 5, 6], [2, 3]),
            &device,
        );
        let (intents, slots) = model.forward(ids);
        assert_eq!(intents.dims(), [2, 51]);
        assert_eq!(slots.dims(), [2, 3, 8]);
        let probs: Vec<f32> = softmax(intents, 1).into_data().iter::<f32>().collect();
        for row in probs.chunks(51) {
            assert!((row.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn test_bag_of_words_shape() {
        let device = Device::flex();
        let model = BagOfWords::new(10, 4, 3, &device);
        assert_eq!(
            model.forward(Tensor::zeros([5, 10], &device)).dims(),
            [5, 3]
        );
    }
}
