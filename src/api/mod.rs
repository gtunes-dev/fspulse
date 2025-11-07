pub mod routes;

// Re-export route handlers for convenience
pub use routes::query;
pub use routes::alerts;
pub use routes::scans;
pub use routes::roots;
pub use routes::schedules;
pub use routes::items;
