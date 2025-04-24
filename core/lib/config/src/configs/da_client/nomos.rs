use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct NomosDaConfig {
    pub app_id: String,
    pub executor_rpc: String,
    pub validator_rpcs: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct NomosSecrets {
    pub username: String,
    pub password: String,
}
