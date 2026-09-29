//! A small graph neural network over the knowledge graph, std-only.
//!
//! One message-passing layer, patient-conditioned:
//! ```text
//!   x_v   = [ present?, is_disease, is_observable, bias ]   (per node, per patient)
//!   agg_v = mean over undirected neighbours of x_u
//!   z_v   = W1 · x_v + Wn · agg_v + b1
//!   h_v   = relu(z_v)
//!   s_d   = w2 · h_d + b2                                   (score per disease node)
//! ```
//! Trained by softmax cross-entropy; back-propagation is exact (single layer,
//! neighbours are input features). The GNN produces a ranking signal only — the
//! rules decide acceptance. Its weights are data, so the core stays disease-blind.

#![allow(clippy::needless_range_loop)] // dense matrix math indexes rows/cols directly

use std::collections::HashSet;

use core_types::{KnowledgeProvider, NodeId, NodeKind};

pub const FIN: usize = 4;
pub const HID: usize = 8;

/// A trained (or freshly initialised) model. Plain data.
#[derive(Clone, Debug)]
pub struct GnnModel {
    pub w1: [[f32; FIN]; HID],
    pub wn: [[f32; FIN]; HID],
    pub b1: [f32; HID],
    pub w2: [f32; HID],
    pub b2: f32,
}

/// Deterministic small-weight initialiser (a hand-rolled LCG — reproducible,
/// std-only). Never use for anything but demo determinism.
pub fn init(seed: u64) -> GnnModel {
    let mut s = seed | 1;
    let mut next = || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (((s >> 40) as f32) / ((1u64 << 24) as f32) - 0.5) * 0.6
    };
    let mut w1 = [[0.0; FIN]; HID];
    let mut wn = [[0.0; FIN]; HID];
    let mut b1 = [0.0; HID];
    let mut w2 = [0.0; HID];
    for i in 0..HID {
        for j in 0..FIN {
            w1[i][j] = next();
            wn[i][j] = next();
        }
        b1[i] = next();
        w2[i] = next();
    }
    GnnModel {
        w1,
        wn,
        b1,
        w2,
        b2: next(),
    }
}

fn relu(x: f32) -> f32 {
    if x > 0.0 {
        x
    } else {
        0.0
    }
}

fn node_features(present: &HashSet<NodeId>, k: &dyn KnowledgeProvider, node: NodeId) -> [f32; FIN] {
    let kind = k.node_kind(node);
    let is_disease = matches!(kind, Some(NodeKind::Disease));
    let is_observable = matches!(
        kind,
        Some(NodeKind::Finding) | Some(NodeKind::Phenotype) | Some(NodeKind::Test) | Some(NodeKind::Trigger)
    );
    [
        if present.contains(&node) { 1.0 } else { 0.0 },
        if is_disease { 1.0 } else { 0.0 },
        if is_observable { 1.0 } else { 0.0 },
        1.0, // bias
    ]
}

/// Mean of neighbour features over the undirected graph (forward ∪ reverse).
fn aggregate(present: &HashSet<NodeId>, k: &dyn KnowledgeProvider, node: NodeId) -> [f32; FIN] {
    let mut sum = [0.0f32; FIN];
    let mut count = 0.0f32;
    for e in k.neighbors(node).iter().chain(k.reverse_neighbors(node)) {
        let f = node_features(present, k, e.target);
        for (s, v) in sum.iter_mut().zip(f.iter()) {
            *s += *v;
        }
        count += 1.0;
    }
    if count > 0.0 {
        for s in &mut sum {
            *s /= count;
        }
    }
    sum
}

/// Cached forward values for one disease node (enough for exact backprop).
struct Act {
    disease: NodeId,
    x: [f32; FIN],
    agg: [f32; FIN],
    z: [f32; HID],
    h: [f32; HID],
    score: f32,
}

impl GnnModel {
    fn forward_disease(&self, present: &HashSet<NodeId>, k: &dyn KnowledgeProvider, d: NodeId) -> Act {
        let x = node_features(present, k, d);
        let agg = aggregate(present, k, d);
        let mut z = [0.0f32; HID];
        let mut h = [0.0f32; HID];
        for i in 0..HID {
            let mut zi = self.b1[i];
            for j in 0..FIN {
                zi += self.w1[i][j] * x[j] + self.wn[i][j] * agg[j];
            }
            z[i] = zi;
            h[i] = relu(zi);
        }
        let mut score = self.b2;
        for i in 0..HID {
            score += self.w2[i] * h[i];
        }
        Act {
            disease: d,
            x,
            agg,
            z,
            h,
            score,
        }
    }

    /// Inference: a score for every disease node given the present concepts.
    pub fn score_diseases(&self, k: &dyn KnowledgeProvider, present: &HashSet<NodeId>) -> Vec<(NodeId, f32)> {
        k.diseases()
            .iter()
            .map(|&d| (d, self.forward_disease(present, k, d).score))
            .collect()
    }

    /// Train by softmax cross-entropy over disease scores. Returns the loss at
    /// each epoch (so a caller/test can see it go down).
    pub fn train(
        &mut self,
        examples: &[(HashSet<NodeId>, NodeId)],
        k: &dyn KnowledgeProvider,
        epochs: usize,
        lr: f32,
    ) -> Vec<f32> {
        let mut history = Vec::with_capacity(epochs);
        for _ in 0..epochs {
            let mut epoch_loss = 0.0f32;
            for (present, correct) in examples {
                let acts: Vec<Act> = k
                    .diseases()
                    .iter()
                    .map(|&d| self.forward_disease(present, k, d))
                    .collect();
                if acts.is_empty() {
                    continue;
                }
                // softmax over disease scores
                let maxs = acts.iter().fold(f32::MIN, |m, a| m.max(a.score));
                let exps: Vec<f32> = acts.iter().map(|a| (a.score - maxs).exp()).collect();
                let sum: f32 = exps.iter().sum();
                let probs: Vec<f32> = exps.iter().map(|e| e / sum).collect();

                let correct_idx = acts.iter().position(|a| a.disease == *correct);
                if let Some(ci) = correct_idx {
                    epoch_loss += -(probs[ci].max(1e-9)).ln();
                }

                // gradients
                let mut g_w1 = [[0.0f32; FIN]; HID];
                let mut g_wn = [[0.0f32; FIN]; HID];
                let mut g_b1 = [0.0f32; HID];
                let mut g_w2 = [0.0f32; HID];
                let mut g_b2 = 0.0f32;

                for (idx, a) in acts.iter().enumerate() {
                    let target = if a.disease == *correct { 1.0 } else { 0.0 };
                    let gs = probs[idx] - target; // dL/ds_d
                    g_b2 += gs;
                    for i in 0..HID {
                        g_w2[i] += gs * a.h[i];
                        let gh = gs * self.w2[i];
                        let gz = if a.z[i] > 0.0 { gh } else { 0.0 };
                        g_b1[i] += gz;
                        for j in 0..FIN {
                            g_w1[i][j] += gz * a.x[j];
                            g_wn[i][j] += gz * a.agg[j];
                        }
                    }
                }

                // SGD step
                self.b2 -= lr * g_b2;
                for i in 0..HID {
                    self.w2[i] -= lr * g_w2[i];
                    self.b1[i] -= lr * g_b1[i];
                    for j in 0..FIN {
                        self.w1[i][j] -= lr * g_w1[i][j];
                        self.wn[i][j] -= lr * g_wn[i][j];
                    }
                }
            }
            history.push(epoch_loss);
        }
        history
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_types::{Edge, EdgeType, MemKnowledge};

    /// Three diseases, each with two distinctive findings. Train the GNN on one
    /// labelled case per disease and assert it (a) drives the loss down and
    /// (b) then ranks the correct disease top for each case. That is a GNN
    /// genuinely learning over the knowledge graph.
    #[test]
    fn gnn_learns_to_rank_the_right_disease() {
        let mut k = MemKnowledge::new();
        // diseases 0,1,2
        for d in 0..3u32 {
            k.add_node(NodeId(d), NodeKind::Disease, "d");
        }
        // findings 10..16, two per disease
        for f in 10..16u32 {
            k.add_node(NodeId(f), NodeKind::Finding, "f");
        }
        let links = [
            (0u32, [10u32, 11u32]),
            (1, [12, 13]),
            (2, [14, 15]),
        ];
        for (d, fs) in links {
            for f in fs {
                k.add_edge(NodeId(d), Edge::new(NodeId(f), EdgeType::HasPhenotype));
            }
        }

        let examples: Vec<(HashSet<NodeId>, NodeId)> = links
            .iter()
            .map(|(d, fs)| {
                let present: HashSet<NodeId> = fs.iter().map(|&f| NodeId(f)).collect();
                (present, NodeId(*d))
            })
            .collect();

        let mut model = init(42);
        let loss = model.train(&examples, &k, 300, 0.2);

        // loss went down substantially
        assert!(loss[0] > loss[loss.len() - 1], "loss did not decrease: {loss:?}");
        assert!(loss[loss.len() - 1] < 0.1, "final loss too high: {}", loss[loss.len() - 1]);

        // after training, the correct disease is top-ranked for each case
        for (present, correct) in &examples {
            let mut scores = model.score_diseases(&k, present);
            scores.sort_by(|a, b| b.1.total_cmp(&a.1));
            assert_eq!(scores[0].0, *correct, "wrong top disease for {correct:?}: {scores:?}");
        }
    }
}
