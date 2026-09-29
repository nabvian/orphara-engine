//! The patient graph the Core consumes — de-identified, concept-tagged, temporal.
//!
//! Produced by the extractor + the concept adapter. It contains no patient
//! identifiers: only resolved concept ids, states,
//! timing, raw measurements for explanation, and source provenance.

use crate::ids::NodeId;

/// The five evidence states that must never be collapsed.
/// In particular `Unknown != Absent` and `NotAssessed != Absent`; conflating
/// them manufactures false contradictions and false negatives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EvidenceState {
    Present,
    Absent,
    Unknown,
    NotAssessed,
    Indeterminate,
}

/// Treatment context of an observation. A finding measured under treatment
/// is not equivalent to the untreated finding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TreatmentState {
    Unknown,
    Baseline,
    BeforeTreatment,
    DuringTreatment,
    AfterTreatment,
    Crisis,
}

/// Where a fact came from — carried through unchanged from the extractor to the
/// explanation layer. The Core never reasons on this; it passes it through so a
/// finding can be traced back to the exact source text.
#[derive(Clone, Debug)]
pub struct Provenance {
    pub document: String,
    /// Page / section / line — provider-defined locator.
    pub locator: String,
    pub exact_text: String,
}

/// Temporal placement of an observation. Any field may be absent.
#[derive(Clone, Copy, Debug, Default)]
pub struct Temporal {
    /// Epoch day (days since 1970-01-01), or `None` if undated.
    pub day: Option<i64>,
    pub age_years: Option<f32>,
    pub episode: Option<u32>,
    pub treatment_state: Option<TreatmentState>,
}

/// A raw measurement retained for evidence/explanation alongside the resolved
/// concept. The Core reasons over the concept, not over `value`.
#[derive(Clone, Debug, Default)]
pub struct Measurement {
    pub value: Option<f64>,
    pub unit: Option<String>,
    /// Display label such as "LOW"; carried for presentation, not reasoned on.
    pub interpretation: Option<String>,
}

/// One observation in the patient graph.
#[derive(Clone, Debug)]
pub struct Observation {
    /// Resolved Global Concept Registry id — same namespace as the DSD.
    pub concept: NodeId,
    pub state: EvidenceState,
    pub temporal: Temporal,
    pub measurement: Option<Measurement>,
    pub provenance: Option<Provenance>,
}

impl Observation {
    /// Minimal present observation of a concept.
    pub fn present(concept: NodeId) -> Self {
        Observation {
            concept,
            state: EvidenceState::Present,
            temporal: Temporal::default(),
            measurement: None,
            provenance: None,
        }
    }
}

/// The temporal patient graph the Core consumes.
#[derive(Clone, Debug, Default)]
pub struct PatientGraph {
    pub observations: Vec<Observation>,
}

impl PatientGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, o: Observation) {
        self.observations.push(o);
    }

    /// Present findings only — a common Core query. Absent/Unknown/NotAssessed
    /// are excluded here; the Core handles them on their own paths.
    pub fn present(&self) -> impl Iterator<Item = &Observation> {
        self.observations
            .iter()
            .filter(|o| o.state == EvidenceState::Present)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_states_are_distinct() {
        assert_ne!(EvidenceState::Unknown, EvidenceState::Absent);
        assert_ne!(EvidenceState::NotAssessed, EvidenceState::Absent);
        assert_ne!(EvidenceState::Indeterminate, EvidenceState::Present);
    }

    #[test]
    fn present_filter_excludes_non_present() {
        let mut g = PatientGraph::new();
        g.push(Observation::present(NodeId(10)));
        let mut absent = Observation::present(NodeId(11));
        absent.state = EvidenceState::Absent;
        g.push(absent);
        let mut unknown = Observation::present(NodeId(12));
        unknown.state = EvidenceState::Unknown;
        g.push(unknown);

        let present: Vec<_> = g.present().map(|o| o.concept).collect();
        assert_eq!(present, vec![NodeId(10)]);
    }
}
