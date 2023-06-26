pub mod v1;
pub mod v2 {
    pub use super::v1::*;
}
pub mod v4;
pub mod v5;

#[derive(Copy, Clone, strum::Display, strum::EnumString)]
pub enum Version {
    #[strum(serialize = "v1")]
    V1,
    #[strum(serialize = "v2")]
    V2,
    // There is no V3 for historical reasons. We yanked Version 3, because it was released with a
    // security flaw. It should never be added back.
    #[strum(serialize = "v4")]
    V4,
    #[strum(serialize = "v5")]
    V5,
}
