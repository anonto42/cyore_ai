// Batching: group examples of equal length so every batch is a rectangle with no padding
// (design.md, decision 5), and turn them into Burn tensors.

use std::collections::BTreeMap;

use burn::prelude::*;

use crate::data::Example;
use crate::random::Rng;
use crate::vocab::{Labels, Vocab};

/// An example as numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct Encoded {
    pub ids: Vec<usize>,
    pub intent: usize,
    pub tags: Vec<usize>,
}

pub fn encode(
    examples: &[Example],
    vocab: &Vocab,
    labels: &Labels,
) -> Result<Vec<Encoded>, String> {
    examples
        .iter()
        .map(|e| {
            let intent = labels
                .intent_id(&e.intent)
                .ok_or_else(|| format!("\"{}\": unknown intent {}", e.text, e.intent))?;
            let tags = e
                .tags
                .iter()
                .map(|t| {
                    labels
                        .tag_id(t)
                        .ok_or_else(|| format!("\"{}\": unknown tag {t}", e.text))
                })
                .collect::<Result<_, _>>()?;
            if e.tokens.is_empty() {
                return Err(format!("\"{}\": no tokens", e.text));
            }
            Ok(Encoded {
                ids: vocab.encode(&e.tokens),
                intent,
                tags,
            })
        })
        .collect()
}

/// Group example indices into batches of equal length, at most `batch_size` each.
/// With an `rng`, examples within each length and the batch order are shuffled; without one,
/// the order is fixed (for evaluation).
pub fn batches(data: &[Encoded], batch_size: usize, rng: Option<&mut Rng>) -> Vec<Vec<usize>> {
    let mut by_len: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, e) in data.iter().enumerate() {
        by_len.entry(e.ids.len()).or_default().push(i);
    }
    let mut out = Vec::new();
    match rng {
        Some(rng) => {
            for group in by_len.values_mut() {
                rng.shuffle(group);
                out.extend(group.chunks(batch_size).map(|c| c.to_vec()));
            }
            rng.shuffle(&mut out);
        }
        None => {
            for group in by_len.values() {
                out.extend(group.chunks(batch_size).map(|c| c.to_vec()));
            }
        }
    }
    out
}

/// Tensors for one batch. `ids` is [batch, len]; `tags` is flattened to [batch * len] so it
/// lines up with slot logits reshaped to [batch * len, n_tags].
pub struct Batch {
    pub ids: Tensor<2, Int>,
    pub intents: Tensor<1, Int>,
    pub tags: Tensor<1, Int>,
}

/// Build a batch. With `dropout = Some((p, rng))`, each context word (tag `O`, real vocabulary
/// word) is replaced by `<word>` with probability p: word dropout (design.md, decision 4).
pub fn make_batch(
    data: &[Encoded],
    indices: &[usize],
    device: &Device,
    dropout: Option<(f64, &mut Rng)>,
) -> Batch {
    let len = data[indices[0]].ids.len();
    let mut ids = Vec::with_capacity(indices.len() * len);
    let mut tags = Vec::with_capacity(indices.len() * len);
    let mut intents = Vec::with_capacity(indices.len());
    let mut dropout = dropout;
    for &i in indices {
        let e = &data[i];
        debug_assert_eq!(e.ids.len(), len, "batches must have one length");
        for (&id, &tag) in e.ids.iter().zip(&e.tags) {
            let drop = match dropout.as_mut() {
                Some((p, rng)) => tag == 0 && id >= Vocab::first_word_id() && rng.chance(*p),
                None => false,
            };
            ids.push(if drop { Vocab::word_id() } else { id } as i64);
            tags.push(tag as i64);
        }
        intents.push(e.intent as i64);
    }
    let n = indices.len();
    Batch {
        ids: Tensor::from_data(TensorData::new(ids, [n, len]), device),
        intents: Tensor::from_data(TensorData::new(intents, [n]), device),
        tags: Tensor::from_data(TensorData::new(tags, [n * len]), device),
    }
}

/// Bag of words for the baseline: [batch, vocab] word counts (order is thrown away).
pub fn bag_of_words(
    data: &[Encoded],
    indices: &[usize],
    vocab_len: usize,
    device: &Device,
) -> Tensor<2> {
    let mut counts = vec![0f32; indices.len() * vocab_len];
    for (row, &i) in indices.iter().enumerate() {
        for &id in &data[i].ids {
            counts[row * vocab_len + id] += 1.0;
        }
    }
    Tensor::from_data(TensorData::new(counts, [indices.len(), vocab_len]), device)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(len: usize, intent: usize) -> Encoded {
        Encoded {
            ids: vec![Vocab::first_word_id(); len],
            intent,
            tags: vec![0; len],
        }
    }

    fn data() -> Vec<Encoded> {
        (0..40).map(|i| enc(1 + i % 4, i % 3)).collect()
    }

    #[test]
    fn test_batches_have_one_length_and_cover_everything() {
        let data = data();
        let all = batches(&data, 4, Some(&mut Rng::new(1)));
        let mut seen: Vec<usize> = all.iter().flatten().copied().collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..40).collect::<Vec<_>>());
        for b in &all {
            assert!(b.len() <= 4);
            assert!(b.iter().all(|&i| data[i].ids.len() == data[b[0]].ids.len()));
        }
    }

    #[test]
    fn test_batch_order_fixed_by_seed() {
        let data = data();
        let a = batches(&data, 4, Some(&mut Rng::new(5)));
        let b = batches(&data, 4, Some(&mut Rng::new(5)));
        let c = batches(&data, 4, Some(&mut Rng::new(6)));
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(batches(&data, 4, None), batches(&data, 4, None));
    }

    #[test]
    fn test_make_batch_shapes_and_dropout() {
        let device = Device::flex();
        let data = vec![enc(3, 1), enc(3, 2)];
        let batch = make_batch(&data, &[0, 1], &device, None);
        assert_eq!(batch.ids.dims(), [2, 3]);
        assert_eq!(batch.intents.dims(), [2]);
        assert_eq!(batch.tags.dims(), [6]);
        let ids: Vec<i64> = batch.ids.into_data().iter::<i64>().collect();
        assert!(ids.iter().all(|&id| id == Vocab::first_word_id() as i64));

        // p = 0 changes nothing; p = 1 replaces every context word with <word>.
        let mut rng = Rng::new(1);
        let none = make_batch(&data, &[0, 1], &device, Some((0.0, &mut rng)));
        let none: Vec<i64> = none.ids.into_data().iter::<i64>().collect();
        assert_eq!(none, ids);
        let all = make_batch(&data, &[0, 1], &device, Some((1.0, &mut rng)));
        let all: Vec<i64> = all.ids.into_data().iter::<i64>().collect();
        assert!(all.iter().all(|&id| id == Vocab::word_id() as i64));
    }

    #[test]
    fn test_bag_of_words_counts() {
        let device = Device::flex();
        let data = vec![Encoded {
            ids: vec![1, 6, 6],
            intent: 0,
            tags: vec![0; 3],
        }];
        let bow: Vec<f32> = bag_of_words(&data, &[0], 8, &device)
            .into_data()
            .iter::<f32>()
            .collect();
        assert_eq!(bow, vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0]);
    }
}
