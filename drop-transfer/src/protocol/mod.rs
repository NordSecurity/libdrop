pub mod v4;
pub mod v6;

#[derive(Copy, Clone, strum::Display, strum::EnumString)]
pub enum Version {
    // Versions V1 and V2 were yanked because these lacked the client
    // authentication, which is a security flaw.

    // There is no V3 for historical reasons. We yanked Version 3, because it was released with a
    // security flaw. It should never be added back.
    #[strum(serialize = "v4")]
    V4,
    #[strum(serialize = "v5")]
    V5,
    #[strum(serialize = "v6")]
    V6,
}

impl From<Version> for i32 {
    fn from(version: Version) -> Self {
        match version {
            Version::V4 => 4,
            Version::V5 => 5,
            Version::V6 => 6,
        }
    }
}
