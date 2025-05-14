use crate::error::FsPulseError;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i64)] // Ensures explicit numeric representation
pub enum AlertType {
    SuspiciousHash,
    InvalidItem,
}

pub enum AlertStatus {
    Open,
}

impl AlertType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertType::SuspiciousHash => "H",
            AlertType::InvalidItem => "I",
        }
    }

    pub fn short_str_to_full(s: &str) -> Result<&str, FsPulseError> {
        match s {
            "H" => Ok("Suspicious Hash"),
            "I" => Ok("Invalid Item"),
            _ => Err(FsPulseError::Error(format!("Invalid alert type: '{}'", s))),
        }
    }
}

impl AlertStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertStatus::Open => "O",
        }
    }

    pub fn short_str_to_full(s: &str) -> Result<&str, FsPulseError> {
        match s {
            "O" => Ok("Open"),
            _ => Err(FsPulseError::Error(format!(
                "Invalid alert status: '{}'",
                s
            ))),
        }
    }
}
