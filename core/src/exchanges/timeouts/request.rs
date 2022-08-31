use mmb_utils::DateTime;

use crate::exchanges::general::request_type::RequestType;
use crate::exchanges::timeouts::requests_timeout_manager::RequestGroupId;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Request {
    pub(crate) request_type: RequestType,
    pub(crate) allowed_start_time: DateTime,
    pub(crate) group_id: Option<RequestGroupId>,
}

impl Request {
    pub fn new(
        request_type: RequestType,
        allowed_start_time: DateTime,
        group_id: Option<RequestGroupId>,
    ) -> Self {
        Self {
            request_type,
            allowed_start_time,
            group_id,
        }
    }
}
