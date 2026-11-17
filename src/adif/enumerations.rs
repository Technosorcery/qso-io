//! ADIF 3.1.8 reference identities and metadata, with historical entries.

mod ant_path;
mod arrl_section;
mod award;
mod geography;
/// Country-specific primary subdivision enums following ADIF III.B.12.
pub mod primary;
mod radio;
mod status;

pub use ant_path::AntPath;
pub use arrl_section::ArrlSection;
pub use award::Award;
pub use geography::{
    AlaskaCounty, DxccEntity, NzRegionDistrict, Region, SecondarySubdivision,
    SecondarySubdivisionAlt,
};
pub use radio::{AwardSponsor, Band, ContestId, Credit, Mode, Submode};
pub use status::{
    Continent, EqslAg, MorseKeyType, PropagationMode, QslMedium, QslRcvd, QslSent, QslVia,
    QsoComplete, QsoDownloadStatus, QsoUploadStatus,
};
