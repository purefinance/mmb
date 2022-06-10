use crate::services::token::AccessTokenClaim;

#[derive(Clone, Default)]
pub struct AccountService;

impl AccountService {
    pub fn authorize(&self, username: &str, password: &str) -> bool {
        username == "admin" && password == "admin"
    }
}

pub struct User {
    pub username: String,
    pub role: String,
}

impl User {
    pub(crate) fn build_guest() -> Self {
        Self {
            username: "Guest".to_string(),
            role: "guest".to_string(),
        }
    }
}

impl From<AccessTokenClaim> for User {
    fn from(claim: AccessTokenClaim) -> Self {
        Self {
            username: claim.username,
            role: claim.role,
        }
    }
}
