use serde::Serialize;

/// The `{"tickets": [...]}` shape returned by `list` and `next`.
#[derive(Debug, Serialize)]
pub struct ListResult {
    pub tickets: Vec<serde_json::Value>,
}

pub fn to_json<T: Serialize>(value: &T) -> crate::error::Result<String> {
    serde_json::to_string(value).map_err(|e| crate::error::Error::Other(e.to_string()))
}
