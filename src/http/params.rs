use serde::{Deserialize, Deserializer};

pub fn deserialize_trim<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    let trimmed = s.map(|s| s.trim().to_string());
    Ok(trimmed.filter(|t| !t.is_empty()))
}

#[derive(Deserialize)]
pub struct QueryParams {
    pub skip: Option<u64>,
    pub take: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_trim")]
    pub sort: Option<String>,
    pub desc: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_trim")]
    pub q: Option<String>,
}
