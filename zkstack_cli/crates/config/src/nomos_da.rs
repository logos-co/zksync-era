use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NomosDaConfig {
    pub executor_rpc: String,
    pub validator_rpcs: String,
    pub app_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NomosSecrets {
    pub username: String,
    pub password: String,
}
