use serde::Deserialize;

use crate::{
    configs::da_client::nomos::NomosDaConfig, AvailConfig, CelestiaConfig, EigenConfig,
    ObjectStoreConfig,
};

pub mod avail;
pub mod celestia;
pub mod eigen;
pub mod nomos;

pub const AVAIL_CLIENT_CONFIG_NAME: &str = "Avail";
pub const CELESTIA_CLIENT_CONFIG_NAME: &str = "Celestia";
pub const EIGEN_CLIENT_CONFIG_NAME: &str = "Eigen";
pub const OBJECT_STORE_CLIENT_CONFIG_NAME: &str = "ObjectStore";
pub const NOMOS_CLIENT_CONFIG_NAME: &str = "Nomos";
pub const NO_DA_CLIENT_CONFIG_NAME: &str = "NoDA";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum DAClientConfig {
    Avail(AvailConfig),
    Celestia(CelestiaConfig),
    Eigen(EigenConfig),
    Nomos(NomosDaConfig),
    ObjectStore(ObjectStoreConfig),
    NoDA,
}

impl From<AvailConfig> for DAClientConfig {
    fn from(config: AvailConfig) -> Self {
        Self::Avail(config)
    }
}
