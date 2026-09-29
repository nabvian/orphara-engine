//! UCUM validation. Unlike LOINC/RxNorm/SNOMED this needs no external release
//! and no licence — the grammar in [`crate::units`] is the authority — so it is
//! always available. It is the safe first step of the "improve OCR" idea: a
//! printed unit can be validated and canonicalised deterministically.

use crate::units;

/// The outcome of validating a printed unit string.
#[derive(Clone, Debug, PartialEq)]
pub struct UcumCheck {
    pub input: String,
    /// Whether the grammar accepts it as a unit.
    pub valid: bool,
    /// The canonical UCUM spelling (present iff valid).
    pub canonical: Option<String>,
    /// The quantity kind (present iff the grammar assigns one).
    pub dimension: Option<String>,
}

/// Validate and canonicalise a printed unit against the UCUM grammar.
pub fn validate(unit: &str) -> UcumCheck {
    let trimmed = unit.trim();
    let dim = units::dimension(trimmed);
    let valid = units::is_plausible_unit(trimmed, false) && dim.is_some();
    UcumCheck {
        input: unit.to_string(),
        valid,
        canonical: if valid { Some(units::canonical(trimmed)) } else { None },
        dimension: dim,
    }
}

/// Whether two printed units are comparable (same quantity kind or an exact
/// scale conversion exists) — the reconciliation-relevant question.
pub fn comparable(a: &str, b: &str) -> bool {
    units::same_dimension(Some(a), Some(b)) || units::convert(1.0, a, b).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_canonicalises() {
        let c = validate("gm/dl");
        assert!(c.valid);
        assert_eq!(c.canonical.as_deref(), Some("g/dL"));
        assert_eq!(c.dimension.as_deref(), Some("mass_per_volume"));
    }

    #[test]
    fn rejects_non_units() {
        assert!(!validate("banana").valid);
        assert!(!validate("42").valid);
    }

    #[test]
    fn comparability() {
        assert!(comparable("g/dL", "g/L")); // conversion exists
        assert!(!comparable("g/dL", "fL")); // different dimensions
    }
}
