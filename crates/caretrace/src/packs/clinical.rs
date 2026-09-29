//! Clinical Findings Pack — qualitative findings a clinician writes in free text
//! ("Doctors Txt"): jaundice, dark urine, Heinz bodies, cyanosis, a low enzyme
//! assay, and so on.
//!
//! This is the patient-side vocabulary that lets a doctor's note drive the
//! whole loop. Its status-concept KEYS are the SAME keys the Red
//! Cell Enzyme DSD uses (`jaundice`, `hemoglobinuria`, `heinz_bite_cells`,
//! `cyanosis`, `g6pd_assay_low`, …), so the concept adapter resolves them to
//! DSD `NodeId`s by identity — no aliases required. Nothing numeric lives here;
//! numeric analytes stay in the laboratory pack.
//!
//! Each finding declares a PRESENT value class (and, where a clinician commonly
//! records it, an ABSENT class) so the deterministic negation guard and the
//! status-conflict engine work exactly as for imaging/molecular findings.

use super::base::{Pack, StatusConceptDef as SC, StatusValue as SV};

const M: &str = "CLINICAL";

// Each concept's value classes are written out as a 'static const slice below.
const V_JAUNDICE: &[SV] = &[
    SV { key: "present", label: "Present", positive: true,
         phrases: &["jaundice", "icterus", "jaundiced", "scleral icterus", "yellowing of the skin"] },
    SV { key: "absent", label: "Absent", positive: false,
         phrases: &["no jaundice", "anicteric", "not jaundiced"] },
];
const V_SPLENOMEGALY: &[SV] = &[
    SV { key: "present", label: "Present", positive: true,
         phrases: &["splenomegaly", "enlarged spleen", "spleen enlarged", "splenic enlargement"] },
    SV { key: "absent", label: "Absent", positive: false,
         phrases: &["no splenomegaly", "spleen not enlarged", "no hepatosplenomegaly"] },
];
const V_PALLOR: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["pallor", "pale conjunctivae", "clinically pale"] }];
const V_FATIGUE: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["fatigue", "tiredness", "lethargy"] }];
const V_DYSPNEA: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["shortness of breath", "dyspnea", "dyspnoea", "breathlessness", "exertional dyspnoea"] }];
const V_TACHYCARDIA: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["tachycardia", "rapid heart rate", "fast heart rate"] }];
const V_HEMOGLOBINURIA: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["dark tea-coloured urine", "tea-coloured urine", "tea coloured urine",
               "cola-coloured urine", "haemoglobinuria", "hemoglobinuria", "dark urine"] }];
const V_HEINZ: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["heinz bodies", "heinz body", "bite cells", "blister cells"] }];
const V_OXIDANT: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["haemolysis following oxidant exposure", "following an oxidant drug",
               "after an oxidant trigger", "after ingestion of fava beans", "after eating fava beans"] }];
const V_CYANOSIS: &[SV] = &[
    SV { key: "present", label: "Present", positive: true,
         phrases: &["cyanosis", "bluish skin", "blue lips", "bluish discolouration"] },
    SV { key: "absent", label: "Absent", positive: false, phrases: &["no cyanosis"] },
];
const V_CYANOSIS_O2: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["cyanosis unresponsive to oxygen", "cyanosis not responsive to oxygen",
               "chocolate-brown blood", "chocolate brown blood"] }];
const V_NEURO2: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["progressive neurological impairment", "neurological regression", "developmental regression"] }];
const V_G6PD_LOW: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["low g6pd", "reduced g6pd activity", "g6pd activity low", "g6pd assay low",
               "low glucose-6-phosphate dehydrogenase"] }];
const V_G6PD_NORMAL: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["g6pd assay normal", "g6pd normal", "normal g6pd", "g6pd activity normal",
               "g6pd within normal limits"] }];
const V_RETICULOCYTOSIS: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["reticulocytosis", "reticulocyte response", "raised reticulocytes",
               "reticulocytes are high", "high reticulocyte count", "brisk reticulocytosis"] }];
const V_PK_LOW: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["low pyruvate kinase activity", "reduced pk activity", "low pk activity"] }];
const V_METHB: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["elevated methaemoglobin", "raised methaemoglobin", "elevated methemoglobin",
               "high methaemoglobin level"] }];
// Pyruvate kinase deficiency — the discriminators the dossier lists, so a
// note/report can drive PK to full support. (Pigment gallstones live in the
// imaging pack; the molecular calls live in the molecular pack.)
const V_NEONATAL_HYPERBILI: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["neonatal jaundice", "neonatal hyperbilirubinaemia", "neonatal hyperbilirubinemia"] }];
const V_IRON_OVERLOAD: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["iron overload without transfusion", "transfusion-independent iron overload",
               "iron overload despite no transfusion", "iron overload without transfusions"] }];
const V_APLASTIC_PARVO: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["aplastic crisis", "parvovirus b19", "aplastic crisis triggered by parvovirus"] }];
const V_CRISIS_INFECTION: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["crisis triggered by infection", "infection-triggered crisis",
               "haemolytic crisis with infection"] }];
const V_DAT_NEG: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["dat-negative", "dat negative", "direct antiglobulin test negative",
               "non-spherocytic", "nonspherocytic", "normal osmotic fragility",
               "osmotic fragility normal"] }];
// Age / sex context (sourced from each dossier): G6PD is X-linked (males
// fully affected); neonatal presentation is major for all three. Ethnicity is
// CAPTURED but NOT scored — the dossiers flag population prevalence as an open,
// unsourced gap (EG-004), so no weighted edge is invented for it.
const V_MALE: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["male", "man", "boy", "gentleman", "male patient"] }];
const V_FEMALE: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["female", "woman", "girl", "lady", "female patient"] }];
const V_NEONATE: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["neonate", "newborn", "neonatal", "day-old", "days old", "day of life"] }];
const V_ETHNICITY: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["mediterranean descent", "african descent", "southeast asian descent",
               "middle eastern descent", "consanguineous community"] }];
const V_CONSANGUINITY: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["consanguineous", "consanguinity", "parents are related", "related parents",
               "first cousins", "born of a consanguineous"] }];
const V_FHX: &[SV] = &[SV { key: "present", label: "Present", positive: true,
    phrases: &["family history of anaemia", "family history of jaundice",
               "family history of a blood disorder", "family history of haemolysis",
               "affected sibling", "affected maternal uncle", "maternal uncle",
               "brother is affected", "known family history"] }];

const STATUS_CONCEPTS: &[SC] = &[
    SC { key: "jaundice", label: "Jaundice / icterus", values: V_JAUNDICE, modality: M },
    SC { key: "splenomegaly", label: "Splenomegaly", values: V_SPLENOMEGALY, modality: M },
    SC { key: "pallor", label: "Pallor", values: V_PALLOR, modality: M },
    SC { key: "fatigue", label: "Fatigue", values: V_FATIGUE, modality: M },
    SC { key: "dyspnea", label: "Shortness of breath / dyspnea", values: V_DYSPNEA, modality: M },
    SC { key: "tachycardia", label: "Tachycardia", values: V_TACHYCARDIA, modality: M },
    SC { key: "hemoglobinuria", label: "Dark tea-coloured urine (haemoglobinuria)", values: V_HEMOGLOBINURIA, modality: M },
    SC { key: "heinz_bite_cells", label: "Heinz bodies / bite cells / blister cells", values: V_HEINZ, modality: M },
    SC { key: "oxidant_temporal", label: "Temporal link to an oxidant trigger", values: V_OXIDANT, modality: M },
    SC { key: "cyanosis", label: "Cyanosis", values: V_CYANOSIS, modality: M },
    SC { key: "cyanosis_o2refract", label: "Cyanosis unresponsive to oxygen / chocolate-brown blood", values: V_CYANOSIS_O2, modality: M },
    SC { key: "neuro_type2", label: "Progressive neurological impairment (type II)", values: V_NEURO2, modality: M },
    SC { key: "g6pd_assay_low", label: "Low G6PD enzyme activity (assay)", values: V_G6PD_LOW, modality: M },
    SC { key: "g6pd_assay_normal", label: "G6PD enzyme activity (normal / near-normal result)", values: V_G6PD_NORMAL, modality: M },
    SC { key: "reticulocytosis", label: "Reticulocytosis", values: V_RETICULOCYTOSIS, modality: M },
    SC { key: "pk_assay_low", label: "Reduced red-cell PK enzyme activity", values: V_PK_LOW, modality: M },
    SC { key: "methb_level", label: "Elevated methaemoglobin level", values: V_METHB, modality: M },
    // PK deficiency discriminators.
    SC { key: "neonatal_hyperbili", label: "Neonatal hyperbilirubinaemia", values: V_NEONATAL_HYPERBILI, modality: M },
    SC { key: "iron_overload_notx", label: "Iron overload without transfusion", values: V_IRON_OVERLOAD, modality: M },
    SC { key: "aplastic_parvo", label: "Aplastic crisis (parvovirus B19)", values: V_APLASTIC_PARVO, modality: M },
    SC { key: "crisis_infection", label: "Crisis triggered by infection (not oxidant)", values: V_CRISIS_INFECTION, modality: M },
    SC { key: "dat_neg_nonspherocytic", label: "DAT-negative, non-spherocytic smear", values: V_DAT_NEG, modality: M },
    // Age / sex / ethnicity context.
    SC { key: "male_sex", label: "Male sex", values: V_MALE, modality: M },
    SC { key: "female_sex", label: "Female sex", values: V_FEMALE, modality: M },
    SC { key: "neonatal", label: "Neonatal presentation", values: V_NEONATE, modality: M },
    SC { key: "ethnicity_high_risk", label: "Higher-prevalence ancestry (context only, unscored)", values: V_ETHNICITY, modality: M },
    SC { key: "consanguinity", label: "Consanguinity (parents related)", values: V_CONSANGUINITY, modality: M },
    SC { key: "family_history_rcd", label: "Family history of a red-cell disorder", values: V_FHX, modality: M },
];

const DOCUMENT_CUES: &[(&str, &[&str])] = &[(
    "CONSULTATION",
    &["clinical note", "clinical findings", "on examination", "history of presenting"],
)];
const FILENAME_CUES: &[(&str, &[&str])] = &[("CONSULTATION", &["note", "clinical", "consult"])];

pub const PACK: Pack = Pack {
    key: "clinical",
    label: "Clinical Findings Pack",
    modality: M,
    concepts: &[],
    claim_types: &[],
    medication_synonyms: &[],
    unsupported_concept_labels: &[],
    followup_patterns: &[],
    document_reference_patterns: &[],
    conflict_window_days: &[],
    claim_lookback_days: &[],
    followup_expectations: &[],
    status_concepts: STATUS_CONCEPTS,
    anchored_phrases: &[],
    document_cues: DOCUMENT_CUES,
    filename_cues: FILENAME_CUES,
};
