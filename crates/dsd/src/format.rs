//! Compiled DSD binary layout. Writing and the byte constants live here so the
//! compiler and loader stay in sync.
//!
//! Layout (little-endian):
//! ```text
//! HEADER   magic "DSD1" | format_version u16 | flags u16 | node_count u32 | section_count u16
//! DIR      section_count * { type u16 | offset u64 | len u64 }
//! PAYLOADS the sections, in directory order
//! TRAILER  fnv1a-64 checksum u64 over everything before it
//! ```
//! Unknown directory entries are ignored, so new section types can be added
//! later without breaking this reader. Node ids are dense `[0, node_count)`.

use core_types::{Edge, EdgeType, EvidenceId, NodeKind, QualifierId};

pub const MAGIC: &[u8; 4] = b"DSD1";
pub const FORMAT_VERSION: u16 = 1;

pub const SEC_KINDS: u16 = 1;
pub const SEC_NAMES: u16 = 2;
pub const SEC_FWD_OFF: u16 = 3;
pub const SEC_FWD_EDGES: u16 = 4;
pub const SEC_REV_OFF: u16 = 5;
pub const SEC_REV_EDGES: u16 = 6;
pub const SEC_PAR_OFF: u16 = 7;
pub const SEC_PAR_NODES: u16 = 8;
pub const SEC_CHI_OFF: u16 = 9;
pub const SEC_CHI_NODES: u16 = 10;
pub const SEC_DISEASES: u16 = 11;

/// Sentinel for an absent optional u32 id (qualifier / evidence).
pub const NONE_U32: u32 = u32::MAX;

/// Bytes per serialized edge: target u32 + type u8 + weight f32 + qual u32 + evid u32.
pub const EDGE_BYTES: usize = 4 + 1 + 4 + 4 + 4;

pub fn kind_to_u8(k: NodeKind) -> u8 {
    match k {
        NodeKind::Disease => 0,
        NodeKind::Phenotype => 1,
        NodeKind::Finding => 2,
        NodeKind::Test => 3,
        NodeKind::Gene => 4,
        NodeKind::Variant => 5,
        NodeKind::Protein => 6,
        NodeKind::Pathway => 7,
        NodeKind::Mechanism => 8,
        NodeKind::Trigger => 9,
        NodeKind::Pattern => 10,
        NodeKind::Drug => 11,
        NodeKind::Treatment => 12,
        NodeKind::Anatomy => 13,
        NodeKind::Evidence => 14,
        NodeKind::Source => 15,
    }
}

pub fn kind_from_u8(b: u8) -> Option<NodeKind> {
    Some(match b {
        0 => NodeKind::Disease,
        1 => NodeKind::Phenotype,
        2 => NodeKind::Finding,
        3 => NodeKind::Test,
        4 => NodeKind::Gene,
        5 => NodeKind::Variant,
        6 => NodeKind::Protein,
        7 => NodeKind::Pathway,
        8 => NodeKind::Mechanism,
        9 => NodeKind::Trigger,
        10 => NodeKind::Pattern,
        11 => NodeKind::Drug,
        12 => NodeKind::Treatment,
        13 => NodeKind::Anatomy,
        14 => NodeKind::Evidence,
        15 => NodeKind::Source,
        _ => return None,
    })
}

pub fn edge_to_u8(e: EdgeType) -> u8 {
    match e {
        EdgeType::HasPhenotype => 0,
        EdgeType::HasFinding => 1,
        EdgeType::HasPattern => 2,
        EdgeType::AssociatedWith => 3,
        EdgeType::CausedBy => 4,
        EdgeType::ParticipatesIn => 5,
        EdgeType::TriggeredBy => 6,
        EdgeType::TreatedBy => 7,
        EdgeType::SupportedBy => 8,
        EdgeType::ContradictedBy => 9,
        EdgeType::Requires => 10,
        EdgeType::Excludes => 11,
        EdgeType::Discriminates => 12,
        EdgeType::Mimics => 13,
        EdgeType::IsA => 14,
        EdgeType::ConfoundedBy => 15,
    }
}

pub fn edge_from_u8(b: u8) -> Option<EdgeType> {
    Some(match b {
        0 => EdgeType::HasPhenotype,
        1 => EdgeType::HasFinding,
        2 => EdgeType::HasPattern,
        3 => EdgeType::AssociatedWith,
        4 => EdgeType::CausedBy,
        5 => EdgeType::ParticipatesIn,
        6 => EdgeType::TriggeredBy,
        7 => EdgeType::TreatedBy,
        8 => EdgeType::SupportedBy,
        9 => EdgeType::ContradictedBy,
        10 => EdgeType::Requires,
        11 => EdgeType::Excludes,
        12 => EdgeType::Discriminates,
        13 => EdgeType::Mimics,
        14 => EdgeType::IsA,
        15 => EdgeType::ConfoundedBy,
        _ => return None,
    })
}

/// Non-cryptographic integrity checksum (FNV-1a 64).
pub fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The CSR form produced by the compiler and consumed by [`serialize`].
/// `*_off` arrays have `node_count + 1` entries; `*_off[i]..*_off[i+1]` indexes
/// the flat array for node `i`.
pub struct CompiledGraph {
    pub kinds: Vec<NodeKind>,
    pub names: Vec<String>,
    pub fwd_off: Vec<u32>,
    pub fwd_edges: Vec<Edge>,
    pub rev_off: Vec<u32>,
    pub rev_edges: Vec<Edge>,
    pub par_off: Vec<u32>,
    pub par_nodes: Vec<u32>,
    pub chi_off: Vec<u32>,
    pub chi_nodes: Vec<u32>,
    pub diseases: Vec<u32>,
}

#[derive(Default)]
struct Buf {
    b: Vec<u8>,
}
impl Buf {
    fn u8(&mut self, v: u8) {
        self.b.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn raw(&mut self, s: &[u8]) {
        self.b.extend_from_slice(s);
    }
}

fn write_edges(b: &mut Buf, edges: &[Edge]) {
    for e in edges {
        b.u32(e.target.0);
        b.u8(edge_to_u8(e.edge_type));
        b.f32(e.weight);
        b.u32(e.qualifier.map(|QualifierId(q)| q).unwrap_or(NONE_U32));
        b.u32(e.evidence.map(|EvidenceId(x)| x).unwrap_or(NONE_U32));
    }
}

fn write_u32s(b: &mut Buf, xs: &[u32]) {
    for &x in xs {
        b.u32(x);
    }
}

/// Serialize a compiled graph to the DSD binary.
pub fn serialize(g: &CompiledGraph) -> Vec<u8> {
    let mut kinds = Buf::default();
    for &k in &g.kinds {
        kinds.u8(kind_to_u8(k));
    }
    let mut names = Buf::default();
    for n in &g.names {
        names.u32(n.len() as u32);
        names.raw(n.as_bytes());
    }
    let mut fwd_off = Buf::default();
    write_u32s(&mut fwd_off, &g.fwd_off);
    let mut fwd_edges = Buf::default();
    write_edges(&mut fwd_edges, &g.fwd_edges);
    let mut rev_off = Buf::default();
    write_u32s(&mut rev_off, &g.rev_off);
    let mut rev_edges = Buf::default();
    write_edges(&mut rev_edges, &g.rev_edges);
    let mut par_off = Buf::default();
    write_u32s(&mut par_off, &g.par_off);
    let mut par_nodes = Buf::default();
    write_u32s(&mut par_nodes, &g.par_nodes);
    let mut chi_off = Buf::default();
    write_u32s(&mut chi_off, &g.chi_off);
    let mut chi_nodes = Buf::default();
    write_u32s(&mut chi_nodes, &g.chi_nodes);
    let mut diseases = Buf::default();
    diseases.u32(g.diseases.len() as u32);
    write_u32s(&mut diseases, &g.diseases);

    let sections: [(u16, Vec<u8>); 11] = [
        (SEC_KINDS, kinds.b),
        (SEC_NAMES, names.b),
        (SEC_FWD_OFF, fwd_off.b),
        (SEC_FWD_EDGES, fwd_edges.b),
        (SEC_REV_OFF, rev_off.b),
        (SEC_REV_EDGES, rev_edges.b),
        (SEC_PAR_OFF, par_off.b),
        (SEC_PAR_NODES, par_nodes.b),
        (SEC_CHI_OFF, chi_off.b),
        (SEC_CHI_NODES, chi_nodes.b),
        (SEC_DISEASES, diseases.b),
    ];

    const HEADER_LEN: usize = 4 + 2 + 2 + 4 + 2;
    const DIR_ENTRY: usize = 2 + 8 + 8;

    let mut out = Buf::default();
    out.raw(MAGIC);
    out.u16(FORMAT_VERSION);
    out.u16(0);
    out.u32(g.kinds.len() as u32);
    out.u16(sections.len() as u16);

    let mut payload_offset = (HEADER_LEN + DIR_ENTRY * sections.len()) as u64;
    for (ty, buf) in &sections {
        out.u16(*ty);
        out.u64(payload_offset);
        out.u64(buf.len() as u64);
        payload_offset += buf.len() as u64;
    }
    for (_, buf) in &sections {
        out.raw(buf);
    }
    let checksum = fnv1a(&out.b);
    out.u64(checksum);
    out.b
}
