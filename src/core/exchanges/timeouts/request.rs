use uuid::Uuid;

use crate::core::{exchanges::general::request_type::RequestType, DateTime};

#[derive(Clone, PartialEq)]
pub struct Request {
    pub(crate) request_type: RequestType,
    pub(crate) allowed_start_time: DateTime,
    pub(crate) group_id: Option<Uuid>,
}

impl Request {
    pub fn new(
        request_type: RequestType,
        allowed_start_time: DateTime,
        group_id: Option<Uuid>,
    ) -> Self {
        Self {
            request_type,
            allowed_start_time,
            group_id,
        }
    }
}
