use casbin::Enforcer;

pub struct AuthService {
    pub enforcer: Enforcer,
}

impl AuthService {
    pub fn new(enforcer: Enforcer) -> Self {
        Self { enforcer }
    }
}
