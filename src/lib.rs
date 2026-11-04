#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]

pub mod closure_api;
pub mod config;
pub mod form;
pub mod html;
pub mod i18n;
pub mod routes;
pub mod session;
mod views;

pub use routes::router;
