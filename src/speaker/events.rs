use crate::rib;

#[derive(Debug)]
pub enum RibEvent {
    UpdateRoutes(Update),
}

#[derive(Debug)]
pub enum FibEvent {
    RibUpdated,
}

#[derive(Debug)]
pub struct Update {
    pub added: Option<rib::RibUpdate>,
    pub withdrawn: Option<rib::RibUpdate>,
    pub rid: u32,
}
