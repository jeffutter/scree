use serde_json::json;

/// Every failure Scree reports. Each variant maps to one documented exit code.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Refused(String),
    #[error("{0}")]
    Conflict(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("database error: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "database schema version {found} is newer than this binary supports ({supported}); upgrade scree"
    )]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::Other(_) => 1,
            Error::Usage(_) => 2,
            Error::NotFound(_) => 3,
            Error::Refused(_) => 4,
            Error::Conflict(_) => 5,
            Error::Db(_) | Error::Io(_) | Error::SchemaTooNew { .. } => 6,
        }
    }

    pub fn code_str(&self) -> &'static str {
        match self {
            Error::Other(_) => "failure",
            Error::Usage(_) => "usage",
            Error::NotFound(_) => "not_found",
            Error::Refused(_) => "refused",
            Error::Conflict(_) => "conflict",
            Error::Db(_) | Error::Io(_) | Error::SchemaTooNew { .. } => "database",
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        json!({"error": {"code": self.code_str(), "message": self.to_string()}})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes() {
        assert_eq!(Error::Other("x".into()).exit_code(), 1);
        assert_eq!(Error::Usage("x".into()).exit_code(), 2);
        assert_eq!(Error::NotFound("x".into()).exit_code(), 3);
        assert_eq!(Error::Refused("x".into()).exit_code(), 4);
        assert_eq!(Error::Conflict("x".into()).exit_code(), 5);
        assert_eq!(Error::Db(rusqlite::Error::InvalidQuery).exit_code(), 6);
        assert_eq!(
            Error::SchemaTooNew {
                found: 9,
                supported: 1
            }
            .exit_code(),
            6
        );
    }

    #[test]
    fn json_shape() {
        let v = Error::Refused("nope".into()).to_json();
        assert_eq!(v["error"]["code"], "refused");
        assert_eq!(v["error"]["message"], "nope");
    }
}
