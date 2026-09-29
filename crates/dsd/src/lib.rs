//! The compiled DSD runtime: a validating loader that implements the frozen
//! [`core_types::KnowledgeProvider`] contract.
//!
//! Because it implements the same trait as the in-memory provider, the Blind
//! Core (`reasoning`) runs over a compiled `.dsd` unchanged. A DSD file is
//! untrusted until validated: the loader checks the magic, version,
//! section directory bounds, checksum and graph indexes before exposing any
//! view, so traversal cannot panic on a malformed file.
//!
//! 0.1.0 note: the file is read into memory and parsed into typed arrays.
//! Memory-mapping and zero-copy hot/cold sections are a later
//! optimisation for the full corpus; correctness comes first.

pub mod format;

use std::fmt;

use core_types::{Edge, EvidenceId, KnowledgeProvider, NodeId, NodeKind, QualifierId};

use format::{
    edge_from_u8, fnv1a, kind_from_u8, EDGE_BYTES, FORMAT_VERSION, MAGIC,
    NONE_U32, SEC_CHI_NODES, SEC_CHI_OFF, SEC_DISEASES, SEC_FWD_EDGES, SEC_FWD_OFF, SEC_KINDS,
    SEC_NAMES, SEC_PAR_NODES, SEC_PAR_OFF, SEC_REV_EDGES, SEC_REV_OFF,
};

#[derive(Debug, PartialEq, Eq)]
pub enum DsdError {
    Truncated,
    BadMagic,
    BadVersion(u16),
    BadEnum,
    Utf8,
    Checksum,
    MissingSection(u16),
    Invalid(&'static str),
    Io(String),
}

impl fmt::Display for DsdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DsdError::Truncated => write!(f, "truncated DSD file"),
            DsdError::BadMagic => write!(f, "not a DSD file (bad magic)"),
            DsdError::BadVersion(v) => write!(f, "unsupported DSD format version {v}"),
            DsdError::BadEnum => write!(f, "invalid node/edge type byte"),
            DsdError::Utf8 => write!(f, "invalid UTF-8 in string section"),
            DsdError::Checksum => write!(f, "checksum mismatch"),
            DsdError::MissingSection(t) => write!(f, "missing required section {t}"),
            DsdError::Invalid(m) => write!(f, "invalid DSD: {m}"),
            DsdError::Io(m) => write!(f, "io error: {m}"),
        }
    }
}
impl std::error::Error for DsdError {}

/// A loaded, validated DSD knowledge package.
pub struct Dsd {
    kinds: Vec<NodeKind>,
    names: Vec<String>,
    fwd_off: Vec<u32>,
    fwd_edges: Vec<Edge>,
    rev_off: Vec<u32>,
    rev_edges: Vec<Edge>,
    par_off: Vec<u32>,
    par_nodes: Vec<NodeId>,
    chi_off: Vec<u32>,
    chi_nodes: Vec<NodeId>,
    diseases: Vec<NodeId>,
}

struct R<'a> {
    b: &'a [u8],
    pos: usize,
}
impl<'a> R<'a> {
    fn new(b: &'a [u8]) -> Self {
        R { b, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], DsdError> {
        let end = self.pos.checked_add(n).ok_or(DsdError::Truncated)?;
        if end > self.b.len() {
            return Err(DsdError::Truncated);
        }
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, DsdError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, DsdError> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, DsdError> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u64(&mut self) -> Result<u64, DsdError> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(u64::from_le_bytes(a))
    }
    fn f32(&mut self) -> Result<f32, DsdError> {
        let s = self.take(4)?;
        Ok(f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn done(&self) -> bool {
        self.pos == self.b.len()
    }
}

fn read_u32_vec(sec: &[u8], count: usize) -> Result<Vec<u32>, DsdError> {
    let mut r = R::new(sec);
    let mut v = Vec::with_capacity(count);
    for _ in 0..count {
        v.push(r.u32()?);
    }
    if !r.done() {
        return Err(DsdError::Invalid("trailing bytes in u32 section"));
    }
    Ok(v)
}

fn read_edges(sec: &[u8]) -> Result<Vec<Edge>, DsdError> {
    if !sec.len().is_multiple_of(EDGE_BYTES) {
        return Err(DsdError::Invalid("edge section not a whole number of edges"));
    }
    let count = sec.len() / EDGE_BYTES;
    let mut r = R::new(sec);
    let mut v = Vec::with_capacity(count);
    for _ in 0..count {
        let target = NodeId(r.u32()?);
        let et = edge_from_u8(r.u8()?).ok_or(DsdError::BadEnum)?;
        let weight = r.f32()?;
        let q = r.u32()?;
        let e = r.u32()?;
        v.push(Edge {
            target,
            edge_type: et,
            weight,
            qualifier: if q == NONE_U32 { None } else { Some(QualifierId(q)) },
            evidence: if e == NONE_U32 { None } else { Some(EvidenceId(e)) },
        });
    }
    Ok(v)
}

fn check_csr(off: &[u32], edges_len: usize, node_count: usize, what: &'static str) -> Result<(), DsdError> {
    if off.len() != node_count + 1 {
        return Err(DsdError::Invalid(what));
    }
    let mut prev = 0u32;
    for &o in off {
        if o < prev {
            return Err(DsdError::Invalid("csr offsets not non-decreasing"));
        }
        prev = o;
    }
    if *off.last().unwrap() as usize != edges_len {
        return Err(DsdError::Invalid("csr last offset != edge count"));
    }
    Ok(())
}

impl Dsd {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Dsd, DsdError> {
        let bytes = std::fs::read(path).map_err(|e| DsdError::Io(e.to_string()))?;
        Dsd::from_bytes(&bytes)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Dsd, DsdError> {
        if data.len() < 8 {
            return Err(DsdError::Truncated);
        }
        // checksum: last 8 bytes cover everything before them.
        let (body, trailer) = data.split_at(data.len() - 8);
        let stored = u64::from_le_bytes(trailer.try_into().unwrap());
        if fnv1a(body) != stored {
            return Err(DsdError::Checksum);
        }

        let mut r = R::new(body);
        if r.take(4)? != MAGIC {
            return Err(DsdError::BadMagic);
        }
        let ver = r.u16()?;
        if ver != FORMAT_VERSION {
            return Err(DsdError::BadVersion(ver));
        }
        let _flags = r.u16()?;
        let node_count = r.u32()? as usize;
        let section_count = r.u16()? as usize;

        let mut dir: Vec<(u16, u64, u64)> = Vec::with_capacity(section_count);
        for _ in 0..section_count {
            let ty = r.u16()?;
            let off = r.u64()?;
            let len = r.u64()?;
            let end = off.checked_add(len).ok_or(DsdError::Truncated)?;
            if end as usize > body.len() {
                return Err(DsdError::Invalid("section extends past file"));
            }
            dir.push((ty, off, len));
        }

        let sec = |ty: u16| -> Result<&[u8], DsdError> {
            let (_, off, len) = dir.iter().find(|(t, _, _)| *t == ty).ok_or(DsdError::MissingSection(ty))?;
            Ok(&body[*off as usize..(*off + *len) as usize])
        };

        // kinds
        let kinds_b = sec(SEC_KINDS)?;
        if kinds_b.len() != node_count {
            return Err(DsdError::Invalid("kinds length != node_count"));
        }
        let mut kinds = Vec::with_capacity(node_count);
        for &b in kinds_b {
            kinds.push(kind_from_u8(b).ok_or(DsdError::BadEnum)?);
        }

        // names
        let mut names = Vec::with_capacity(node_count);
        let mut nr = R::new(sec(SEC_NAMES)?);
        for _ in 0..node_count {
            let len = nr.u32()? as usize;
            let s = nr.take(len)?;
            names.push(std::str::from_utf8(s).map_err(|_| DsdError::Utf8)?.to_string());
        }
        if !nr.done() {
            return Err(DsdError::Invalid("trailing bytes in names section"));
        }

        let fwd_off = read_u32_vec(sec(SEC_FWD_OFF)?, node_count + 1)?;
        let fwd_edges = read_edges(sec(SEC_FWD_EDGES)?)?;
        check_csr(&fwd_off, fwd_edges.len(), node_count, "fwd_off length")?;

        let rev_off = read_u32_vec(sec(SEC_REV_OFF)?, node_count + 1)?;
        let rev_edges = read_edges(sec(SEC_REV_EDGES)?)?;
        check_csr(&rev_off, rev_edges.len(), node_count, "rev_off length")?;

        let par_off = read_u32_vec(sec(SEC_PAR_OFF)?, node_count + 1)?;
        let par_raw = read_u32_vec(sec(SEC_PAR_NODES)?, sec(SEC_PAR_NODES)?.len() / 4)?;
        check_csr(&par_off, par_raw.len(), node_count, "par_off length")?;

        let chi_off = read_u32_vec(sec(SEC_CHI_OFF)?, node_count + 1)?;
        let chi_raw = read_u32_vec(sec(SEC_CHI_NODES)?, sec(SEC_CHI_NODES)?.len() / 4)?;
        check_csr(&chi_off, chi_raw.len(), node_count, "chi_off length")?;

        // diseases
        let dis_b = sec(SEC_DISEASES)?;
        let mut dr = R::new(dis_b);
        let dcount = dr.u32()? as usize;
        let dis_raw = read_u32_vec(dr.take(dcount * 4)?, dcount)?;

        // referential validation: every target/parent/child/disease id is in range.
        let nc = node_count as u32;
        let in_range = |xs: &[u32]| xs.iter().all(|&x| x < nc);
        if !fwd_edges.iter().all(|e| e.target.0 < nc)
            || !rev_edges.iter().all(|e| e.target.0 < nc)
            || !in_range(&par_raw)
            || !in_range(&chi_raw)
            || !in_range(&dis_raw)
        {
            return Err(DsdError::Invalid("node id out of range"));
        }

        Ok(Dsd {
            kinds,
            names,
            fwd_off,
            fwd_edges,
            rev_off,
            rev_edges,
            par_off,
            par_nodes: dis_or_nodes(&par_raw),
            chi_off,
            chi_nodes: dis_or_nodes(&chi_raw),
            diseases: dis_or_nodes(&dis_raw),
        })
    }

    fn slice_edges<'s>(off: &[u32], edges: &'s [Edge], node: NodeId) -> &'s [Edge] {
        let i = node.0 as usize;
        if i + 1 >= off.len() {
            return &[];
        }
        &edges[off[i] as usize..off[i + 1] as usize]
    }
    fn slice_nodes<'s>(off: &[u32], nodes: &'s [NodeId], node: NodeId) -> &'s [NodeId] {
        let i = node.0 as usize;
        if i + 1 >= off.len() {
            return &[];
        }
        &nodes[off[i] as usize..off[i + 1] as usize]
    }
}

fn dis_or_nodes(raw: &[u32]) -> Vec<NodeId> {
    raw.iter().map(|&x| NodeId(x)).collect()
}

impl KnowledgeProvider for Dsd {
    fn node_kind(&self, node: NodeId) -> Option<NodeKind> {
        self.kinds.get(node.0 as usize).copied()
    }
    fn name(&self, node: NodeId) -> Option<&str> {
        self.names.get(node.0 as usize).map(|s| s.as_str())
    }
    fn neighbors(&self, node: NodeId) -> &[Edge] {
        Dsd::slice_edges(&self.fwd_off, &self.fwd_edges, node)
    }
    fn reverse_neighbors(&self, node: NodeId) -> &[Edge] {
        Dsd::slice_edges(&self.rev_off, &self.rev_edges, node)
    }
    fn ontology_parents(&self, node: NodeId) -> &[NodeId] {
        Dsd::slice_nodes(&self.par_off, &self.par_nodes, node)
    }
    fn ontology_children(&self, node: NodeId) -> &[NodeId] {
        Dsd::slice_nodes(&self.chi_off, &self.chi_nodes, node)
    }
    fn diseases(&self) -> &[NodeId] {
        &self.diseases
    }
}

#[cfg(test)]
mod tests {
    use super::format::{serialize, CompiledGraph};
    use super::*;
    use core_types::{EdgeType, NodeKind};

    fn tiny() -> CompiledGraph {
        // node 0 = disease, 1 = finding; edge 0 -HasPhenotype-> 1 forward, reverse 1->0.
        CompiledGraph {
            kinds: vec![NodeKind::Disease, NodeKind::Finding],
            names: vec!["disease-0".into(), "finding-1".into()],
            fwd_off: vec![0, 1, 1],
            fwd_edges: vec![Edge::new(NodeId(1), EdgeType::HasPhenotype)],
            rev_off: vec![0, 0, 1],
            rev_edges: vec![Edge::new(NodeId(0), EdgeType::HasPhenotype)],
            par_off: vec![0, 0, 0],
            par_nodes: vec![],
            chi_off: vec![0, 0, 0],
            chi_nodes: vec![],
            diseases: vec![0],
        }
    }

    #[test]
    fn round_trip() {
        let bytes = serialize(&tiny());
        let d = Dsd::from_bytes(&bytes).unwrap();
        assert_eq!(d.node_kind(NodeId(0)), Some(NodeKind::Disease));
        assert_eq!(d.name(NodeId(1)), Some("finding-1"));
        assert_eq!(d.neighbors(NodeId(0)).len(), 1);
        assert_eq!(d.neighbors(NodeId(0))[0].target, NodeId(1));
        // reverse: finding-1 -> disease-0, the retrieval primitive
        assert_eq!(d.reverse_neighbors(NodeId(1))[0].target, NodeId(0));
        assert_eq!(d.diseases(), &[NodeId(0)]);
        assert!(d.neighbors(NodeId(5)).is_empty());
    }

    #[test]
    fn tamper_is_rejected() {
        let mut bytes = serialize(&tiny());
        let n = bytes.len();
        bytes[n / 2] ^= 0xFF; // flip a byte in the body
        assert!(matches!(Dsd::from_bytes(&bytes), Err(DsdError::Checksum)));
    }

    #[test]
    fn bad_magic_rejected() {
        let mut bytes = serialize(&tiny());
        bytes[0] = b'X';
        // checksum will now fail first (body changed) — still an error, never a panic.
        assert!(Dsd::from_bytes(&bytes).is_err());
    }
}
