pub mod v1;
pub mod v2 {
    pub use super::v1::*;
}
pub mod v3;

#[derive(Copy, Clone, strum::Display, strum::EnumString)]
pub enum Version {
    #[strum(serialize = "v1")]
    V1,
    #[strum(serialize = "v2")]
    V2,
    #[strum(serialize = "v3")]
    V3,
}
