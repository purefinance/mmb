use chrono::{Duration, Utc};
use jsonwebtoken::errors::Error;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct TokenService {
    secret: String,
    access_token_lifetime_ms: i64,
    refresh_token_lifetime_ms: i64,
}

#[derive(Serialize)]
struct AccessTokenClaim {
    username: String,
    role: String,
    exp: i64,
}

#[derive(Serialize, Deserialize)]
pub struct RefreshTokenClaim {
    pub username: String,
    pub role: String,
    pub exp: i64,
}

impl TokenService {
    pub fn new(
        secret: String,
        access_token_lifetime_ms: i64,
        refresh_token_lifetime_ms: i64,
    ) -> Self {
        Self {
            secret,
            access_token_lifetime_ms,
            refresh_token_lifetime_ms,
        }
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

    pub fn generate_refresh_token(&self, username: &str, role: &str) -> Result<String, Error> {
        let dt = Utc::now() + Duration::milliseconds(self.refresh_token_lifetime_ms);
        let claim = RefreshTokenClaim {
            username: username.into(),
            role: role.into(),
            exp: dt.timestamp_millis(),
        };
        let token = encode(
            &Header::default(),
            &claim,
            &EncodingKey::from_secret(self.secret.as_ref()),
        )?;
        Ok(token)
    }

    pub fn parse_refresh_token(
        &self,
        token: &str,
    ) -> jsonwebtoken::errors::Result<RefreshTokenClaim> {
        let token = decode::<RefreshTokenClaim>(
            token,
            &DecodingKey::from_secret(self.secret.as_ref()),
            &Validation::default(),
        )?;
        Ok(token.claims)
    }
}
