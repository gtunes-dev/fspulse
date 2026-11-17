use log::warn;
use serde::{Deserialize, Serialize};

#[repr(i64)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ItemType {
    File = 0,
    Directory = 1,
    Symlink = 2,
    Unknown = 3,
}

impl ItemType {
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => ItemType::File,
            1 => ItemType::Directory,
            2 => ItemType::Symlink,
            3 => ItemType::Unknown,
            _ => {
                warn!(
                    "Invalid ItemType value in database: {}, defaulting to Unknown",
                    value
                );
                ItemType::Unknown
            }
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            ItemType::File => "F",
            ItemType::Directory => "D",
            ItemType::Symlink => "S",
            ItemType::Unknown => "U",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            ItemType::File => "File",
            ItemType::Directory => "Directory",
            ItemType::Symlink => "Symlink",
            ItemType::Unknown => "Unknown",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "FILE" => Some(ItemType::File),
            "DIRECTORY" | "DIR" => Some(ItemType::Directory),
            "SYMLINK" => Some(ItemType::Symlink),
            "UNKNOWN" => Some(ItemType::Unknown),
            // Short names
            "F" => Some(ItemType::File),
            "D" => Some(ItemType::Directory),
            "S" => Some(ItemType::Symlink),
            "U" => Some(ItemType::Unknown),
            _ => None,
        }
    }
}

impl std::fmt::Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

#[repr(i64)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Access {
    Ok = 0,        // No known access issues (default)
    MetaError = 1, // Can't stat (found during scan phase)
    ReadError = 2, // Can stat, can't read (found during analysis phase)
}

impl Access {
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => Access::Ok,
            1 => Access::MetaError,
            2 => Access::ReadError,
            _ => {
                warn!(
                    "Invalid Access value in database: {}, defaulting to Ok",
                    value
                );
                Access::Ok
            }
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            Access::Ok => "N",
            Access::MetaError => "M",
            Access::ReadError => "R",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            Access::Ok => "No Error",
            Access::MetaError => "Meta Error",
            Access::ReadError => "Read Error",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "NO ERROR" | "NOERROR" => Some(Access::Ok),
            "META ERROR" | "METAERROR" => Some(Access::MetaError),
            "READ ERROR" | "READERROR" => Some(Access::ReadError),
            // Short names
            "N" => Some(Access::Ok),
            "M" => Some(Access::MetaError),
            "R" => Some(Access::ReadError),
            _ => None,
        }
    }
}

impl std::fmt::Display for Access {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_name())
    }
}
