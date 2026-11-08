#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]

pub mod closure_api;
pub mod composer;
pub mod config;
pub mod editor;
pub mod form;
pub mod html;
pub mod i18n;
pub mod routes;
pub mod serve;
pub mod session;
pub mod templates;
mod views;

pub use routes::router;
