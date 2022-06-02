use chrono::{Duration, Utc};
use jsonwebtoken::errors::Error;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;

#[derive(Clone)]
pub struct AccountService {
    secret: String,
    access_token_lifetime_ms: i64,
}

#[derive(Serialize)]
struct AccessTokenClaim {
    username: String,
    role: String,
    exp: i64,
}

impl AccountService {
    pub fn new(secret: String, access_token_lifetime_ms: i64) -> AccountService {
        AccountService {
            secret,
            access_token_lifetime_ms,
        }
    }

    pub fn authorize(&self, username: &str, password: &str) -> bool {
        username == "admin" && password == "admin"
    }

    pub fn generate_access_token(
        &self,
        username: &str,
        role: &str,
    ) -> Result<(String, i64), Error> {
        let expiration =
            (Utc::now() + Duration::milliseconds(self.access_token_lifetime_ms)).timestamp_millis();
        let claim = AccessTokenClaim {
            username: username.into(),
            role: role.into(),
            exp: expiration,
        };
        let token = encode(
            &Header::default(),
            &claim,
            &EncodingKey::from_secret(self.secret.as_ref()),
        )?;
        Ok((token, expiration))
    }
}
