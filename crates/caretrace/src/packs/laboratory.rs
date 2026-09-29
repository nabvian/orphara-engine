//! Laboratory pack: numeric CBC / chemistry / enzyme analytes. Other modalities
//! live in sibling packs.

use super::base::{ClaimTypeDef, ConceptDef, Pack};

const CONCEPTS: &[ConceptDef] = &[
    ConceptDef {
        key: "hemoglobin", label: "Hemoglobin", canonical_unit: "g/dL",
        material_abs: 0.6, material_rel: 0.06,
        synonyms: &["hemoglobin", "haemoglobin", "hb", "hgb", "hemoglobin level",
                    "haemoglobin level", "hb level"],
        unit_aliases: &[("gm/dl", "g/dL"), ("g/dl", "g/dL"), ("g / dl", "g/dL"),
                        ("gm%", "g/dL"), ("g%", "g/dL")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "rbc", label: "RBC Count", canonical_unit: "x10^12/L",
        material_abs: 0.3, material_rel: 0.07,
        synonyms: &["rbc", "rbc count", "red blood cell count", "red cell count",
                    "erythrocyte count"],
        unit_aliases: &[("million/ul", "x10^12/L"), ("10^12/l", "x10^12/L"),
                        ("x10^12/l", "x10^12/L"), ("mill/cumm", "x10^12/L")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "hematocrit", label: "Hematocrit", canonical_unit: "%",
        material_abs: 2.0, material_rel: 0.06,
        synonyms: &["hematocrit", "haematocrit", "hct", "pcv", "packed cell volume"],
        unit_aliases: &[("percent", "%"), ("%", "%")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "mcv", label: "MCV", canonical_unit: "fL",
        material_abs: 3.0, material_rel: 0.05,
        synonyms: &["mcv", "mean corpuscular volume", "mean cell volume"],
        unit_aliases: &[("fl", "fL"), ("femtolitre", "fL")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "mch", label: "MCH", canonical_unit: "pg",
        material_abs: 1.5, material_rel: 0.06,
        synonyms: &["mch", "mean corpuscular hemoglobin", "mean corpuscular haemoglobin"],
        unit_aliases: &[("pg", "pg")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "mchc", label: "MCHC", canonical_unit: "g/dL",
        material_abs: 1.5, material_rel: 0.05,
        synonyms: &["mchc", "mean corpuscular hemoglobin concentration",
                    "mean corpuscular haemoglobin concentration"],
        unit_aliases: &[("g/dl", "g/dL"), ("gm/dl", "g/dL")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "rdw", label: "RDW", canonical_unit: "%",
        material_abs: 1.5, material_rel: 0.08,
        synonyms: &["rdw", "rdw-cv", "red cell distribution width"],
        unit_aliases: &[("%", "%")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "wbc", label: "WBC Count", canonical_unit: "x10^9/L",
        material_abs: 1.0, material_rel: 0.15,
        synonyms: &["wbc", "wbc count", "white blood cell count", "tlc",
                    "total leucocyte count", "total leukocyte count", "leucocyte count"],
        unit_aliases: &[("/ul", "x10^9/L"), ("cells/ul", "x10^9/L"), ("cumm", "x10^9/L"),
                        ("/cumm", "x10^9/L"), ("10^9/l", "x10^9/L"), ("x10^9/l", "x10^9/L")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "platelets", label: "Platelet Count", canonical_unit: "x10^9/L",
        material_abs: 30.0, material_rel: 0.15,
        synonyms: &["platelets", "platelet", "platelet count", "plt", "thrombocyte count"],
        unit_aliases: &[("/ul", "x10^9/L"), ("lakh/cumm", "x10^9/L"), ("10^9/l", "x10^9/L"),
                        ("x10^9/l", "x10^9/L"), ("/cumm", "x10^9/L")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "ferritin", label: "Serum Ferritin", canonical_unit: "ng/mL",
        material_abs: 5.0, material_rel: 0.15,
        synonyms: &["ferritin", "serum ferritin", "ferritin level"],
        unit_aliases: &[("ng/ml", "ng/mL"), ("ug/l", "ng/mL"), ("mcg/l", "ng/mL")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "creatinine", label: "Serum Creatinine", canonical_unit: "mg/dL",
        material_abs: 0.2, material_rel: 0.15,
        synonyms: &["creatinine", "serum creatinine", "s. creatinine", "creat"],
        unit_aliases: &[("mg/dl", "mg/dL"), ("mg%", "mg/dL")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "glucose", label: "Glucose", canonical_unit: "mg/dL",
        material_abs: 15.0, material_rel: 0.12,
        synonyms: &["glucose", "blood glucose", "fasting glucose", "fasting blood sugar",
                    "fbs", "random blood sugar", "rbs", "plasma glucose"],
        unit_aliases: &[("mg/dl", "mg/dL"), ("mg%", "mg/dL")],
        modality: "LABORATORY",
    },
    // Red-cell enzyme assays — reported as numeric biomarkers. The direction
    // (low/high) that matters diagnostically is taken from each report's OWN
    // printed reference range or flag, never an invented cutoff.
    ConceptDef {
        key: "g6pd_activity", label: "G6PD enzyme activity", canonical_unit: "U/g",
        material_abs: 1.0, material_rel: 0.15,
        synonyms: &["g6pd activity", "g6pd enzyme activity", "g6pd", "g-6-pd",
                    "glucose-6-phosphate dehydrogenase", "glucose 6 phosphate dehydrogenase"],
        unit_aliases: &[("u/ghb", "U/g"), ("u/g hb", "U/g"), ("iu/ghb", "U/g")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "pk_activity", label: "Pyruvate kinase activity", canonical_unit: "U/g",
        material_abs: 1.0, material_rel: 0.15,
        synonyms: &["pyruvate kinase activity", "pyruvate kinase", "pk activity",
                    "red cell pk", "red-cell pyruvate kinase"],
        unit_aliases: &[("u/ghb", "U/g"), ("u/g hb", "U/g")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "methaemoglobin", label: "Methaemoglobin", canonical_unit: "%",
        material_abs: 1.0, material_rel: 0.15,
        synonyms: &["methaemoglobin", "methemoglobin", "methaemoglobin level",
                    "methemoglobin level", "met-hb", "methb"],
        unit_aliases: &[("%", "%")],
        modality: "LABORATORY",
    },
    // Haemolysis biomarkers — the biochemical signature the dossiers describe
    // (reticulocyte response, unconjugated hyperbilirubinaemia). Interpreted
    // high/low from each report's OWN printed reference range/flag.
    ConceptDef {
        key: "reticulocyte_percentage", label: "Reticulocyte percentage", canonical_unit: "%",
        material_abs: 0.5, material_rel: 0.20,
        synonyms: &["reticulocyte percentage", "reticulocyte count", "reticulocytes",
                    "reticulocyte", "retic count", "retic"],
        unit_aliases: &[("%", "%")],
        modality: "LABORATORY",
    },
    ConceptDef {
        key: "bilirubin", label: "Bilirubin", canonical_unit: "mg/dL",
        material_abs: 0.5, material_rel: 0.20,
        synonyms: &["bilirubin", "total bilirubin", "indirect bilirubin",
                    "unconjugated bilirubin", "serum bilirubin"],
        unit_aliases: &[("mg/dl", "mg/dL"), ("mg%", "mg/dL")],
        modality: "LABORATORY",
    },
];

const MEDICATION_SYNONYMS: &[(&str, &str)] = &[
    ("ferrous sulfate", "ferrous sulfate"),
    ("ferrous sulphate", "ferrous sulfate"),
    ("fe sulfate", "ferrous sulfate"),
    ("iron (ferrous sulfate)", "ferrous sulfate"),
    ("ferrous ascorbate", "ferrous ascorbate"),
    ("folic acid", "folic acid"),
    ("vitamin b12", "vitamin b12"),
    ("cyanocobalamin", "vitamin b12"),
    ("pantoprazole", "pantoprazole"),
    ("paracetamol", "paracetamol"),
    ("acetaminophen", "paracetamol"),
    ("vitamin c", "ascorbic acid"),
    ("ascorbic acid", "ascorbic acid"),
];

const CLAIM_TYPES: &[ClaimTypeDef] = &[
    ClaimTypeDef {
        key: "IRON_DEFICIENCY", label: "Iron deficiency",
        patterns: &["iron deficiency confirmed", "iron deficiency anaemia confirmed",
                    "iron deficiency anemia confirmed", "confirmed iron deficiency",
                    "iron deficiency established", "iron deficiency documented"],
        requires_any: &["ferritin", "tsat", "serum_iron"],
        requires_labels: &["Serum Ferritin", "Transferrin saturation (TSAT)",
                           "Serum iron studies"],
        supportive_direction: &[("ferritin", "lt", 30.0)],
        supportive_status_polarity: None, modality: "LABORATORY",
    },
    ClaimTypeDef {
        key: "ANEMIA_PRESENT", label: "Anemia documented",
        patterns: &["anaemia", "anemia", "anaemic", "anemic"],
        requires_any: &["hemoglobin"], requires_labels: &["Hemoglobin"],
        supportive_direction: &[("hemoglobin", "lt", 13.0)],
        supportive_status_polarity: None, modality: "LABORATORY",
    },
    ClaimTypeDef {
        key: "MICROCYTOSIS", label: "Microcytosis",
        patterns: &["microcytic", "microcytosis", "microcytic hypochromic"],
        requires_any: &["mcv"], requires_labels: &["MCV"],
        supportive_direction: &[("mcv", "lt", 80.0)],
        supportive_status_polarity: None, modality: "LABORATORY",
    },
    ClaimTypeDef {
        key: "HB_NORMALIZED", label: "Hemoglobin normalized",
        patterns: &["hemoglobin normalised", "hemoglobin normalized",
                    "haemoglobin normalised", "haemoglobin normalized",
                    "hb normalised", "hb normalized", "haemoglobin has normalised"],
        requires_any: &["hemoglobin"], requires_labels: &["Hemoglobin"],
        supportive_direction: &[("hemoglobin", "gt", 13.0)],
        supportive_status_polarity: None, modality: "LABORATORY",
    },
    ClaimTypeDef {
        key: "RENAL_FUNCTION_NORMAL", label: "Renal function normal",
        patterns: &["renal function normal", "renal function is normal",
                    "normal renal function", "kidney function normal"],
        requires_any: &["creatinine"], requires_labels: &["Serum Creatinine"],
        supportive_direction: &[("creatinine", "lt", 1.3)],
        supportive_status_polarity: None, modality: "LABORATORY",
    },
    ClaimTypeDef {
        key: "THALASSEMIA_TRAIT_EXCLUDED", label: "Thalassemia trait excluded",
        patterns: &["thalassaemia trait excluded", "thalassemia trait excluded",
                    "thalassaemia excluded", "thalassemia excluded",
                    "haemoglobinopathy excluded", "hemoglobinopathy excluded"],
        requires_any: &["hb_electrophoresis", "hplc"],
        requires_labels: &["Hemoglobin electrophoresis / HPLC"],
        supportive_direction: &[], supportive_status_polarity: None, modality: "LABORATORY",
    },
    ClaimTypeDef {
        key: "GI_BLEED_EXCLUDED", label: "GI blood loss excluded",
        patterns: &["gi blood loss excluded", "no evidence of gi bleed",
                    "gastrointestinal blood loss excluded", "occult blood negative"],
        requires_any: &["fecal_occult_blood", "endoscopy"],
        requires_labels: &["Faecal occult blood test", "Endoscopy report"],
        supportive_direction: &[], supportive_status_polarity: None, modality: "LABORATORY",
    },
];

const UNSUPPORTED_CONCEPT_LABELS: &[(&str, &str)] = &[
    ("tsat", "Transferrin saturation (TSAT)"),
    ("serum_iron", "Serum iron"),
    ("transferrin_saturation", "Transferrin saturation (TSAT)"),
    ("hb_electrophoresis", "Hemoglobin electrophoresis"),
    ("hplc", "HPLC"),
    ("fecal_occult_blood", "Faecal occult blood test"),
    ("endoscopy", "Endoscopy report"),
];

const FOLLOWUP_PATTERNS: &[(&str, &str)] = &[
    (r"repeat\s+(?:the\s+)?(cbc|complete blood count|haemogram|hemogram)[^.\n]{0,60}", "Repeat CBC"),
    (r"repeat\s+(?:serum\s+)?ferritin[^.\n]{0,60}", "Repeat ferritin"),
    (r"review\s+(?:with|after)\s+[^.\n]{0,60}", "Review with results"),
    (r"(?:refer|referral)\s+(?:to|for)\s+([a-z ]{3,40})", "Referral"),
];

// Citing verb prefix, inlined (shared citing-verb prefix).
const CITE: &str = concat!(
    r"\b(?:as\s+(?:described|reported|noted|detailed)\s+(?:in|on)|",
    r"see|refer\s+to|per|awaiting|pending|correlate\s+with)\s+(?:the\s+)?"
);

const DOCUMENT_REFERENCE_PATTERNS: &[(&str, &str, &str)] = &[
    // Patterns are assembled at registry time by prefixing CITE; store the
    // suffix + label + hint here and let the registry build the full regex.
    (r"endoscopy\s+report\b", "Endoscopy report", "OTHER"),
    (r"biopsy\s+report\b", "Biopsy report", "PATHOLOGY_REPORT"),
    (r"histopathology\s+report\b", "Histopathology report", "PATHOLOGY_REPORT"),
    (r"peripheral\s+smear\s+report\b", "Peripheral smear report", "PATHOLOGY_REPORT"),
];

const CONFLICT_WINDOW_DAYS: &[(&str, i64)] = &[
    ("hemoglobin", 2), ("platelets", 2), ("hematocrit", 2), ("rbc", 2),
    ("mcv", 7), ("mch", 7), ("mchc", 7), ("rdw", 7), ("wbc", 2),
    ("ferritin", 14), ("creatinine", 2), ("glucose", 1),
];

const CLAIM_LOOKBACK_DAYS: &[(&str, i64)] = &[
    ("IRON_DEFICIENCY", 180), ("ANEMIA_PRESENT", 90), ("MICROCYTOSIS", 90),
    ("HB_NORMALIZED", 7), ("RENAL_FUNCTION_NORMAL", 30),
    ("THALASSEMIA_TRAIT_EXCLUDED", 365), ("GI_BLEED_EXCLUDED", 365),
];

const FOLLOWUP_EXPECTATIONS: &[(&str, (i64, &str))] = &[
    ("Repeat CBC", (56, "LAB_REPORT")),
    ("Repeat ferritin", (56, "LAB_REPORT")),
    ("Review with results", (42, "")),
    ("Referral", (60, "")),
];

const DOCUMENT_CUES: &[(&str, &[&str])] = &[(
    "LAB_REPORT",
    &["complete blood count", "laboratory summary", "iron studies", "cumulative laboratory"],
)];

const FILENAME_CUES: &[(&str, &[&str])] =
    &[("LAB_REPORT", &["cbc", "lab", "ferritin", "laboratory"])];

/// The prefix that turns each `DOCUMENT_REFERENCE_PATTERNS` suffix into a full,
/// citing-verb-anchored pattern. Exposed so the registry assembles it once.
pub const CITE_PREFIX: &str = CITE;

pub const PACK: Pack = Pack {
    key: "laboratory",
    label: "Laboratory Pack",
    modality: "LABORATORY",
    concepts: CONCEPTS,
    claim_types: CLAIM_TYPES,
    medication_synonyms: MEDICATION_SYNONYMS,
    unsupported_concept_labels: UNSUPPORTED_CONCEPT_LABELS,
    followup_patterns: FOLLOWUP_PATTERNS,
    document_reference_patterns: DOCUMENT_REFERENCE_PATTERNS,
    conflict_window_days: CONFLICT_WINDOW_DAYS,
    claim_lookback_days: CLAIM_LOOKBACK_DAYS,
    followup_expectations: FOLLOWUP_EXPECTATIONS,
    status_concepts: &[],
    anchored_phrases: &[],
    document_cues: DOCUMENT_CUES,
    filename_cues: FILENAME_CUES,
};
