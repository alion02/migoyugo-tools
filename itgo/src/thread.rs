pub struct Thread {
    pub nodes: u64,
    pub pv_nodes: u64,
    pub node_limit: u64,
    pub evals: u64,
    countdown: u32,
}

impl Thread {
    pub fn new(node_limit: u64) -> Self {
        Self { nodes: 0, node_limit, evals: 0, pv_nodes: 0, countdown: 1 }
    }

    pub fn tick_countdown(&mut self) -> bool {
        self.countdown -= 1;
        self.countdown == 0
    }

    pub fn reset_countdown(&mut self) {
        self.countdown = (self.node_limit - self.nodes).min(8192) as u32;
    }
}
