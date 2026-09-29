//! Timeline validity pass.
//!
//! Reads the generic [`EdgeType::ConfoundedBy`] relation (`result -> confounding
//! state`, edge `weight` = window in days) and the observations' [`Temporal`]
//! days. When a confounded result and its confounder are both present within the
//! window — or the timing is undocumented and cannot be excluded — it raises a
//! flag to repeat the result once the confounder resolves. Disease-specificity
//! lives only in which nodes the DSD joins; this code is disease-blind. Advisory:
//! the snapshot verifier ignores `ConfoundedBy`, so this changes no ranking.

use std::collections::HashMap;

use core_types::{EdgeType, EvidenceState, KnowledgeProvider, NodeId, PatientGraph};

/// Whether the confounder's co-occurrence with the result is established by
/// dates, or merely cannot be ruled out because the timing is undocumented.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Timing {
    /// Both dated and within the window — the confounding is documented.
    WithinWindow,
    /// At least one is undated — the trap cannot be excluded.
    Undetermined,
}

/// A documented result whose reliability is questioned by its timing.
#[derive(Clone, Debug)]
pub struct TemporalFlag {
    /// The confounded observation (e.g. a normal enzyme assay).
    pub result: NodeId,
    /// The co-occurring state that undermines it (e.g. reticulocytosis).
    pub confounder: NodeId,
    pub window_days: i64,
    pub result_day: Option<i64>,
    pub confounder_day: Option<i64>,
    pub timing: Timing,
    /// Generic, source-neutral explanation (specificity lives in the DSD edge).
    pub message: String,
}

/// Present observations grouped by concept, keeping every recorded day.
fn present_days(patient: &PatientGraph) -> HashMap<NodeId, Vec<Option<i64>>> {
    let mut out: HashMap<NodeId, Vec<Option<i64>>> = HashMap::new();
    for o in &patient.observations {
        if o.state == EvidenceState::Present {
            out.entry(o.concept).or_default().push(o.temporal.day);
        }
    }
    out
}

/// Run the timeline-validity pass over a patient graph and knowledge provider.
pub fn analyze(patient: &PatientGraph, k: &dyn KnowledgeProvider) -> Vec<TemporalFlag> {
    let present = present_days(patient);
    let mut flags: Vec<TemporalFlag> = Vec::new();
    let mut seen: std::collections::HashSet<(NodeId, NodeId)> = std::collections::HashSet::new();

    for (&result, result_days) in &present {
        for e in k.neighbors(result) {
            if e.edge_type != EdgeType::ConfoundedBy {
                continue;
            }
            let confounder = e.target;
            let cf_days = match present.get(&confounder) {
                Some(d) => d,
                None => continue, // the confounder was not documented
            };
            let window = e.weight.round().max(0.0) as i64;

            // Pick the timing across every (result, confounder) date pairing:
            // a within-window dated pair wins; a dated-but-apart pair is no
            // co-occurrence (contributes nothing); an undated pair on either
            // side means the trap cannot be excluded (Undetermined).
            let mut best: Option<(Timing, Option<i64>, Option<i64>)> = None;
            'pairs: for &rd in result_days {
                for &cd in cf_days {
                    match (rd, cd) {
                        (Some(r), Some(c)) if (r - c).abs() <= window => {
                            best = Some((Timing::WithinWindow, Some(r), Some(c)));
                            break 'pairs;
                        }
                        (Some(_), Some(_)) => {} // dated and apart → not a co-occurrence
                        _ if best.is_none() => best = Some((Timing::Undetermined, rd, cd)),
                        _ => {}
                    }
                }
            }

            let (timing, rday, cday) = match best {
                Some(x) => x,
                None => continue,
            };
            if !seen.insert((result, confounder)) {
                continue;
            }
            let rname = k.name(result).unwrap_or("this result");
            let cname = k.name(confounder).unwrap_or("a confounding state");
            let when = match timing {
                Timing::WithinWindow => format!("within {window} day(s) of"),
                Timing::Undetermined => "at an undocumented time relative to".to_string(),
            };
            let message = format!(
                "\u{201c}{rname}\u{201d} was documented {when} \u{201c}{cname}\u{201d}; \
                 its result is time-dependently unreliable in that setting. \
                 Consider repeating it once the confounding state has resolved."
            );
            flags.push(TemporalFlag {
                result,
                confounder,
                window_days: window,
                result_day: rday,
                confounder_day: cday,
                timing,
                message,
            });
        }
    }
    flags.sort_by_key(|f| (f.result.0, f.confounder.0));
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_types::{Edge, MemKnowledge, NodeKind, Observation, Temporal};

    fn provider() -> MemKnowledge {
        let mut k = MemKnowledge::new();
        k.add_node(NodeId(1), NodeKind::Test, "G6PD enzyme activity (normal result)");
        k.add_node(NodeId(2), NodeKind::Finding, "Reticulocytosis");
        // result --confounded_by(window=14d)--> confounder
        let mut e = Edge::new(NodeId(2), EdgeType::ConfoundedBy);
        e.weight = 14.0;
        k.add_edge(NodeId(1), e);
        k
    }

    fn present_on(concept: u32, day: Option<i64>) -> Observation {
        Observation {
            concept: NodeId(concept),
            state: EvidenceState::Present,
            temporal: Temporal { day, ..Temporal::default() },
            measurement: None,
            provenance: None,
        }
    }

    #[test]
    fn flags_assay_drawn_within_the_confounder_window() {
        let k = provider();
        let mut p = PatientGraph::new();
        p.push(present_on(1, Some(100))); // normal assay on day 100
        p.push(present_on(2, Some(103))); // reticulocytosis on day 103 (3 <= 14)
        let flags = analyze(&p, &k);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].timing, Timing::WithinWindow);
        assert_eq!(flags[0].result, NodeId(1));
        assert!(flags[0].message.contains("repeating"));
    }

    #[test]
    fn undated_cannot_be_excluded() {
        let k = provider();
        let mut p = PatientGraph::new();
        p.push(present_on(1, None));
        p.push(present_on(2, None));
        let flags = analyze(&p, &k);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].timing, Timing::Undetermined);
    }

    #[test]
    fn no_flag_when_dated_outside_the_window() {
        let k = provider();
        let mut p = PatientGraph::new();
        p.push(present_on(1, Some(100)));
        p.push(present_on(2, Some(200))); // 100 days apart, > 14
        assert!(analyze(&p, &k).is_empty());
    }

    #[test]
    fn no_flag_without_the_confounder() {
        let k = provider();
        let mut p = PatientGraph::new();
        p.push(present_on(1, Some(100))); // assay only; no reticulocytosis
        assert!(analyze(&p, &k).is_empty());
    }
}
