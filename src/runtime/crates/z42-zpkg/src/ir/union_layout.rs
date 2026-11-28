//! Layout used to translate frame reference slots to active heap locations.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnionLayoutDesc {
    pub nodes: Box<[UnionLayoutNode]>,
}

impl UnionLayoutDesc {
    /// Children are stored before their parents; the last node is the root.
    pub fn root(&self) -> &UnionLayoutNode {
        self.nodes.last().expect("decoded union layout has a root")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnionLayoutNode {
    pub size: u32,
    pub align: u8,
    pub tag_bytes: u8,
    pub reference_count: u16,
    pub cases: Box<[Box<[UnionLayoutEntry]>]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnionLayoutEntry {
    pub offset: u32,
    pub slot: u16,
    /// A preceding union layout node, or a single reference word.
    pub child: Option<u32>,
}
