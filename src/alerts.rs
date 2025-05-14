#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i64)] // Ensures explicit numeric representation
pub enum AlertKind {
    SuspiciousHash = 0,
    InvalidItem = 1,
}