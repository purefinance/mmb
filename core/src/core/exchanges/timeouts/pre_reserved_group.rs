use crate::core::exchanges::timeouts::requests_timeout_manager::RequestGroupId;

#[derive(Clone)]
pub struct PreReservedGroup {
    pub(crate) id: RequestGroupId,
    pub(crate) _group_type: String,
    pub(crate) pre_reserved_requests_count: usize,
}

impl PreReservedGroup {
    pub fn new(
        id: RequestGroupId,
        _group_type: String,
        pre_reserved_requests_count: usize,
    ) -> Self {
        Self {
            id,
            _group_type,
            pre_reserved_requests_count,
        }
    }
}
