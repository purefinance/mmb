use chrono::{Duration, Utc};
use jsonwebtoken::errors::Error;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct TokenService {
    access_token_secret: String,
    refresh_token_secret: String,
    access_token_lifetime: i64,
    refresh_token_lifetime: i64,
}

#[derive(Serialize, Deserialize)]
pub struct AccessTokenClaim {
    pub username: String,
    pub role: String,
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
        access_token_secret: String,
        refresh_token_secret: String,
        access_token_lifetime: i64,
        refresh_token_lifetime: i64,
    ) -> Self {
        Self {
            access_token_secret,
            refresh_token_secret,
            access_token_lifetime,
            refresh_token_lifetime,
        }
    }

    pub fn generate_access_token(
        &self,
        username: &str,
        role: &str,
    ) -> Result<(String, i64), Error> {
        let expiration = (Utc::now() + Duration::seconds(self.access_token_lifetime)).timestamp();
        let claim = AccessTokenClaim {
            username: username.into(),
            role: role.into(),
            exp: expiration,
        };
        let token = encode(
            &Header::default(),
            &claim,
            &EncodingKey::from_secret(self.access_token_secret.as_ref()),
        )?;
        Ok((token, expiration))
    }

    pub fn generate_refresh_token(&self, username: &str, role: &str) -> Result<String, Error> {
        let expiration = (Utc::now() + Duration::seconds(self.refresh_token_lifetime)).timestamp();
        let claim = RefreshTokenClaim {
            username: username.into(),
            role: role.into(),
            exp: expiration,
        };
        let token = encode(
            &Header::default(),
            &claim,
            &EncodingKey::from_secret(self.refresh_token_secret.as_ref()),
        )?;
        Ok(token)
    }

    pub fn parse_access_token(
        &self,
        token: &str,
    ) -> jsonwebtoken::errors::Result<AccessTokenClaim> {
        let token = decode::<AccessTokenClaim>(
            token,
            &DecodingKey::from_secret(self.access_token_secret.as_ref()),
            &Validation::default(),
        )?;
        Ok(token.claims)
    }

    pub fn parse_refresh_token(
        &self,
        token: &str,
    ) -> jsonwebtoken::errors::Result<RefreshTokenClaim> {
        let token = decode::<RefreshTokenClaim>(
            token,
            &DecodingKey::from_secret(self.refresh_token_secret.as_ref()),
            &Validation::default(),
        )?;
        Ok(token.claims)
    }
}
