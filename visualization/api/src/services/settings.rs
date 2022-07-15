use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};

#[derive(Clone)]
pub struct SettingsService {
    pool: Pool<Postgres>,
}

#[derive(Debug)]
pub enum SettingCodes {
    Configuration,
}

#[derive(sqlx::FromRow, Serialize, Deserialize, Clone)]
pub struct Setting {
    pub content: Option<String>,
}

impl SettingsService {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
    pub async fn get_settings(&self, code: SettingCodes) -> Result<Setting, sqlx::Error> {
        sqlx::query_as::<Postgres, Setting>(include_str!("sql/get_settings_by_code.sql"))
            .bind(format!("{:?}", code))
            .fetch_one(&self.pool)
            .await
    }

    pub async fn save_setting(&self, code: SettingCodes, content: &str) -> Result<(), sqlx::Error> {
        sqlx::query(include_str!("sql/merge_settings.sql"))
            .bind(format!("{:?}", code))
            .bind(content)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
