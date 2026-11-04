#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]

pub mod closure_api;
pub mod config;
pub mod html;
pub mod messages;
pub mod routes;
pub mod session;

pub use routes::router;
