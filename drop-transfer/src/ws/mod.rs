pub mod client;
mod events;
pub mod server;
mod utils;

pub use events::*;

#[async_trait::async_trait]
pub trait Pinger {
    async fn tick(&mut self);
}
