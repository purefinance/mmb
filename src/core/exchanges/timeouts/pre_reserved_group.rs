use uuid::Uuid;

#[derive(Clone)]
pub struct PreReservedGroup {
    pub(crate) id: Uuid,
    pub(crate) group_type: String,
    pub(crate) pre_reserved_requests_count: usize,
}

impl PreReservedGroup {
    pub fn new(id: Uuid, group_type: String, pre_reserved_requests_count: usize) -> Self {
        Self {
            id,
            group_type,
            pre_reserved_requests_count,
        }
    }
}
