use serde::{Deserialize, Serialize};
use tracing::{error, info};
use zksync_config::configs::da_client::nomos::{NomosDaConfig, NomosSecrets};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
    DataAvailabilityClient,
};

#[derive(Serialize)]
pub struct DispersalRequest {
    pub data: Vec<u8>,
    pub metadata: MetaData,
}
#[derive(Copy, Clone, Default, Debug, PartialEq, PartialOrd, Ord, Eq, Serialize, Deserialize)]
pub struct Index([u8; 8]);
#[derive(Default, Debug, Copy, Clone, Serialize, Eq, PartialEq)]
pub struct MetaData {
    app_id: [u8; 32],
    index: Index,
}

#[derive(Debug, Clone)]
pub struct NomosDaClient {
    config: NomosDaConfig,
    client: reqwest::Client,
    secrets: NomosSecrets,
}

impl NomosDaClient {
    pub fn new(config: NomosDaConfig, secrets: NomosSecrets) -> anyhow::Result<Self> {
        let client = reqwest::Client::new();
        Ok(Self {
            config,
            client,
            secrets,
        })
    }
}

#[async_trait::async_trait]
impl DataAvailabilityClient for NomosDaClient {
    async fn dispatch_blob(
        &self,
        batch_number: u32,
        data: Vec<u8>,
    ) -> Result<DispatchResponse, DAError> {
        let mut data = data;
        let padding = 31 - (data.len() % 31);
        if padding != 31 {
            data.extend(vec![0; padding]);
        }

        let app_id: [u8; 32] = hex::decode(&self.config.app_id)
            .map_err(|_| {
                error!("Failed to decode app_id");
                DAError {
                    error: anyhow::anyhow!("Failed to decode app_id"),
                    is_retriable: false,
                }
            })?
            .try_into()
            .map_err(|_| {
                error!("Failed to decode app_id");
                DAError {
                    error: anyhow::anyhow!("Failed to decode app_id"),
                    is_retriable: false,
                }
            })?;

        let request = DispersalRequest {
            data,
            metadata: MetaData {
                app_id,
                index: Default::default(),
            },
        };

        let request_builder = self
            .client
            .post(&self.config.rpc)
            .basic_auth(&self.secrets.username, Some(self.secrets.password.clone()))
            .json(&request);

        let req = request_builder.build().map_err(|e| {
            error!("Failed to build request: {:?}", e);
            DAError {
                error: anyhow::anyhow!("Failed to build request"),
                is_retriable: false,
            }
        })?;

        info!("Nomos Request: {:?}", req);

        let response = self.client.execute(req).await.map_err(|e| {
            error!("Failed to execute request: {:?}", e);
            DAError {
                error: anyhow::anyhow!("Failed to execute request"),
                is_retriable: true,
            }
        })?;

        info!("Nomos Rpc Response: {:?}", response);

        if response.status() != reqwest::StatusCode::OK {
            error!("Failed to dispatch blob: {:?}", response);
            return Err(DAError {
                error: anyhow::anyhow!("Failed to dispatch blob"),
                is_retriable: true,
            });
        }

        let response_id = format!("{batch_number:?}");
        Ok(DispatchResponse::from(response_id))
    }

    async fn ensure_finality(
        &self,
        dispatch_request_id: String,
    ) -> Result<Option<FinalityResponse>, DAError> {
        Ok(Some(FinalityResponse {
            blob_id: dispatch_request_id,
        }))
    }

    async fn get_inclusion_data(&self, _blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        Ok(None)
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        None
    }

    fn client_type(&self) -> ClientType {
        ClientType::Nomos
    }

    async fn balance(&self) -> Result<u64, DAError> {
        Ok(0)
    }
}
