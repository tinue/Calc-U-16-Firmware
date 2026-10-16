//! Voyager (SwissMicros DM-L series) hardware simulator.

pub mod board;
pub mod bus;
pub mod cpu;
pub mod ihex;
pub mod machine;
pub mod periph;

pub use machine::{Config, Machine};
pub mod frontend;
pub mod models;
