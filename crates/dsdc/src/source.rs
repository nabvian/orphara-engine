//! `.dsdsrc` text loader, so knowledge is authored as data. Line-oriented;
//! blank lines and `#` comments are ignored.
//!
//! Grammar (whitespace-separated; the display name is the rest of the line):
//! ```text
//! concept <kind> <key> <display name…>     # declare a node, keyed by <key>
//! isa     <child_key> <parent_key>         # ontology IS_A edge
//! rel     <from_key> <edge> <target_key> [weight]   # a typed edge
//! ```
//! Each first-seen key gets a dense `NodeId`. A `rel`/`isa` naming an undeclared
//! key is a hard error — no dangling references.

use std::collections::HashMap;

use core_types::{Edge, EdgeType, NodeId, NodeKind};

use crate::GraphBuilder;

#[derive(Debug, PartialEq, Eq)]
pub enum SourceError {
    Parse(usize, String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::Parse(line, m) => write!(f, "line {line}: {m}"),
        }
    }
}
impl std::error::Error for SourceError {}

fn kind_from_str(s: &str) -> Option<NodeKind> {
    Some(match s {
        "disease" => NodeKind::Disease,
        "phenotype" => NodeKind::Phenotype,
        "finding" => NodeKind::Finding,
        "test" => NodeKind::Test,
        "gene" => NodeKind::Gene,
        "variant" => NodeKind::Variant,
        "protein" => NodeKind::Protein,
        "pathway" => NodeKind::Pathway,
        "mechanism" => NodeKind::Mechanism,
        "trigger" => NodeKind::Trigger,
        "pattern" => NodeKind::Pattern,
        "drug" => NodeKind::Drug,
        "treatment" => NodeKind::Treatment,
        "anatomy" => NodeKind::Anatomy,
        "evidence" => NodeKind::Evidence,
        "source" => NodeKind::Source,
        _ => return None,
    })
}

fn edge_from_str(s: &str) -> Option<EdgeType> {
    Some(match s {
        "has_phenotype" => EdgeType::HasPhenotype,
        "has_finding" => EdgeType::HasFinding,
        "has_pattern" => EdgeType::HasPattern,
        "associated_with" => EdgeType::AssociatedWith,
        "caused_by" => EdgeType::CausedBy,
        "participates_in" => EdgeType::ParticipatesIn,
        "triggered_by" => EdgeType::TriggeredBy,
        "treated_by" => EdgeType::TreatedBy,
        "supported_by" => EdgeType::SupportedBy,
        "contradicted_by" => EdgeType::ContradictedBy,
        "requires" => EdgeType::Requires,
        "excludes" => EdgeType::Excludes,
        "discriminates" => EdgeType::Discriminates,
        "mimics" => EdgeType::Mimics,
        "is_a" => EdgeType::IsA,
        "confounded_by" => EdgeType::ConfoundedBy,
        _ => return None,
    })
}

/// Parse `.dsdsrc` text into a populated [`GraphBuilder`].
pub fn parse(text: &str) -> Result<GraphBuilder, SourceError> {
    Ok(parse_with_keys(text)?.0)
}

/// Parse `.dsdsrc` text and also return the key → [`NodeId`] map.
///
/// The key map is the package's slice of the Global Concept Registry: it is what
/// a concept adapter (extractor term → registry key → `NodeId`) resolves against,
/// and what a caller uses to build a patient graph by name rather than by opaque
/// id. The ids are the dense ones the compiler assigned in first-seen order.
pub fn parse_with_keys(text: &str) -> Result<(GraphBuilder, HashMap<String, NodeId>), SourceError> {
    let mut b = GraphBuilder::new();
    let mut keys: HashMap<String, NodeId> = HashMap::new();

    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let verb = it.next().unwrap();
        match verb {
            "concept" => {
                let kind_s = it.next().ok_or_else(|| SourceError::Parse(line_no, "concept: missing kind".into()))?;
                let key = it.next().ok_or_else(|| SourceError::Parse(line_no, "concept: missing key".into()))?;
                let name = it.collect::<Vec<_>>().join(" ");
                let kind = kind_from_str(kind_s)
                    .ok_or_else(|| SourceError::Parse(line_no, format!("unknown kind '{kind_s}'")))?;
                if keys.contains_key(key) {
                    return Err(SourceError::Parse(line_no, format!("duplicate concept key '{key}'")));
                }
                let display = if name.is_empty() { key } else { name.as_str() };
                let id = b.add_node(kind, display);
                keys.insert(key.to_string(), id);
            }
            "isa" => {
                let child = it.next().ok_or_else(|| SourceError::Parse(line_no, "isa: missing child".into()))?;
                let parent = it.next().ok_or_else(|| SourceError::Parse(line_no, "isa: missing parent".into()))?;
                let c = *keys.get(child).ok_or_else(|| SourceError::Parse(line_no, format!("unknown key '{child}'")))?;
                let p = *keys.get(parent).ok_or_else(|| SourceError::Parse(line_no, format!("unknown key '{parent}'")))?;
                b.add_isa(c, p).map_err(|e| SourceError::Parse(line_no, e.to_string()))?;
            }
            "rel" => {
                let dis = it.next().ok_or_else(|| SourceError::Parse(line_no, "rel: missing disease".into()))?;
                let edge_s = it.next().ok_or_else(|| SourceError::Parse(line_no, "rel: missing edge type".into()))?;
                let tgt = it.next().ok_or_else(|| SourceError::Parse(line_no, "rel: missing target".into()))?;
                let weight = match it.next() {
                    Some(w) => w.parse::<f32>().map_err(|_| SourceError::Parse(line_no, format!("bad weight '{w}'")))?,
                    None => 1.0,
                };
                let edge_type = edge_from_str(edge_s)
                    .ok_or_else(|| SourceError::Parse(line_no, format!("unknown edge type '{edge_s}'")))?;
                let from = *keys.get(dis).ok_or_else(|| SourceError::Parse(line_no, format!("unknown key '{dis}'")))?;
                let target = *keys.get(tgt).ok_or_else(|| SourceError::Parse(line_no, format!("unknown key '{tgt}'")))?;
                b.add_edge(from, Edge { target, edge_type, weight, qualifier: None, evidence: None })
                    .map_err(|e| SourceError::Parse(line_no, e.to_string()))?;
            }
            other => {
                return Err(SourceError::Parse(line_no, format!("unknown directive '{other}'")));
            }
        }
    }
    Ok((b, keys))
}

/// Convenience: parse and compile in one step.
pub fn compile_source(text: &str) -> Result<Vec<u8>, SourceError> {
    Ok(parse(text)?.compile())
}

/// Parse, compile, and return the compiled bytes together with the key map.
pub fn compile_source_with_keys(
    text: &str,
) -> Result<(Vec<u8>, HashMap<String, NodeId>), SourceError> {
    let (b, keys) = parse_with_keys(text)?;
    Ok((b.compile(), keys))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_types::KnowledgeProvider;
    use dsd::Dsd;

    #[test]
    fn parses_and_compiles() {
        let src = "
# a tiny package
concept disease d1 Test Disease
concept finding f1 A Finding
concept test    t1 A Confirmatory Test
rel d1 has_finding f1 0.8
rel d1 requires t1
";
        let bytes = compile_source(src).unwrap();
        let d = Dsd::from_bytes(&bytes).unwrap();
        assert_eq!(d.diseases().len(), 1);
        assert_eq!(d.name(d.diseases()[0]), Some("Test Disease"));
        // the disease has two forward edges (finding + required test)
        assert_eq!(d.neighbors(d.diseases()[0]).len(), 2);
    }

    #[test]
    fn unknown_key_is_rejected() {
        let err = parse("rel d1 has_finding ghost").unwrap_err();
        assert!(matches!(err, SourceError::Parse(1, _)));
    }
}
