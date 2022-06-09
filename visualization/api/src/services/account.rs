#[derive(Clone, Default)]
pub struct AccountService;

impl AccountService {
    pub fn authorize(&self, username: &str, password: &str) -> bool {
        username == "admin" && password == "admin"
    }
}
