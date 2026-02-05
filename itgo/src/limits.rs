use crate::protocol::Limit;

pub struct Limits {
    pub depth: u32,
    pub nodes: u64,
    pub ms: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { depth: 64, nodes: !0, ms: !0 }
    }
}

impl Limits {
    pub fn new(limits: impl IntoIterator<Item = Limit>) -> Self {
        Self::default().and(limits)
    }

    pub fn and(mut self, limits: impl IntoIterator<Item = Limit>) -> Self {
        for limit in limits {
            match limit {
                Limit::Depth(value) => self.depth = self.depth.min(value),
                Limit::Nodes(value) => self.nodes = self.nodes.min(value),
                Limit::Ms(value) => self.ms = self.ms.min(value),
            }
        }
        self
    }
}
