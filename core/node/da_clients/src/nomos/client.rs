use std::ops::Range;

use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{error, info};
use zksync_config::configs::da_client::nomos::{NomosDaConfig, NomosSecrets};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
    DataAvailabilityClient,
};

type AppId = [u8; 32];
type Index = [u8; 8];
#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash, PartialOrd, Ord)]
pub struct HeaderId([u8; 32]);

#[derive(Serialize)]
pub struct DispersalRequest {
    pub data: Vec<u8>,
    pub metadata: MetaData,
}

#[derive(Serialize, Deserialize)]
pub struct GetRangeReq {
    pub app_id: AppId,
    pub range: Range<[u8; 8]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CryptarchiaInfo {
    pub tip: HeaderId,
    pub slot: u64,
    pub height: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct Tx(pub String);

pub type BlobId = [u8; 32];
#[derive(Debug, Clone, Hash, Serialize, Deserialize, Eq, PartialEq)]
pub struct BlobInfo {
    id: BlobId,
    metadata: MetaData,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    header: String,
    cl_transactions: IndexSet<Tx>,
    bl_blobs: IndexSet<BlobInfo>,
}

#[derive(Default, Debug, Copy, Clone, Hash, Serialize, Deserialize, Eq, PartialEq)]
pub struct MetaData {
    app_id: AppId,
    index: Index,
}

#[derive(Debug, Clone)]
pub struct NomosDaClient {
    client: reqwest::Client,
    config: NomosDaConfig,
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

    fn get_app_id(&self) -> Result<[u8; 32], DAError> {
        let app_id = hex::decode(&self.config.app_id)
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
        Ok(app_id)
    }

    async fn send_dispersal_request(&self, request: DispersalRequest) -> Result<BlobId, DAError> {
        let request_builder = self
            .client
            .post(format!("{}/disperse-data", self.config.executor_rpc))
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

        info!("Nomos Response: {:?}", response);
        if response.status() != reqwest::StatusCode::OK {
            error!("Failed to disperse data: {:?}", response);
            return Err(DAError {
                error: anyhow::anyhow!("Failed to disperse data"),
                is_retriable: true,
            });
        }

        let blob_id: BlobId = response.json().await.map_err(|e| {
            error!("Failed to parse response: {:?}", e);
            DAError {
                error: anyhow::anyhow!("Failed to parse response"),
                is_retriable: true,
            }
        })?;

        info!("Blob ID: {:?}", blob_id);

        Ok(blob_id)
    }

    async fn get_cryptarchia_info(&self, url: &str) -> Result<CryptarchiaInfo, DAError> {
        info!("Getting CryptarchiaInfo from {}", url);
        let info_request = self
            .client
            .get(format!("{}/cryptarchia/info", url))
            .basic_auth(&self.secrets.username, Some(self.secrets.password.clone()))
            .build()
            .map_err(|e| {
                error!("Failed to build request: {:?}", e);
                DAError {
                    error: anyhow::anyhow!("Failed to build request"),
                    is_retriable: false,
                }
            })?;

        let info_response = self.client.execute(info_request).await.map_err(|e| {
            error!("Failed to execute request: {:?}", e);
            DAError {
                error: anyhow::anyhow!("Failed to execute request"),
                is_retriable: true,
            }
        })?;

        if info_response.status() != reqwest::StatusCode::OK {
            error!("Failed to get info: {:?}", info_response);
            return Err(DAError {
                error: anyhow::anyhow!("Failed to get info"),
                is_retriable: true,
            });
        }

        let tip: CryptarchiaInfo = info_response.json().await.map_err(|e| {
            error!("Failed to parse cryptarchia info: {:?}", e);
            DAError {
                error: anyhow::anyhow!("Failed to parse cryptarchia info"),
                is_retriable: true,
            }
        })?;

        Ok(tip)
    }

    async fn get_blobs_in_block(
        &self,
        url: &str,
        header_id: HeaderId,
    ) -> Result<Vec<BlobInfo>, DAError> {
        let get_block_request = self
            .client
            .post(format!("{}/storage/block", url))
            .header("Content-Type", "application/json")
            .basic_auth(&self.secrets.username, Some(self.secrets.password.clone()))
            .json(&header_id)
            .build()
            .map_err(|e| {
                error!("Failed to build request: {:?}", e);
                DAError {
                    error: anyhow::anyhow!("Failed to build request"),
                    is_retriable: false,
                }
            })?;

        let get_block_response = self.client.execute(get_block_request).await.map_err(|e| {
            error!("Failed to execute request: {:?}", e);
            DAError {
                error: anyhow::anyhow!("Failed to execute request"),
                is_retriable: true,
            }
        })?;

        if get_block_response.status() != reqwest::StatusCode::OK {
            error!("Failed to get block: {:?}", get_block_response);
            return Err(DAError {
                error: anyhow::anyhow!("Failed to get block"),
                is_retriable: false,
            });
        }

        let block: Value = get_block_response.json().await.map_err(|e| {
            error!("Failed to parse block: {:?}", e);
            DAError {
                error: anyhow::anyhow!("Failed to parse block"),
                is_retriable: true,
            }
        })?;

        let blobs = block["bl_blobs"].as_array().ok_or_else(|| {
            error!("Failed to parse blobs");
            DAError {
                error: anyhow::anyhow!("Failed to parse blobs"),
                is_retriable: true,
            }
        })?;

        let blobs: Vec<BlobInfo> = blobs
            .iter()
            .filter_map(|blob| {
                let blob: BlobInfo = serde_json::from_value(blob.clone()).ok()?;
                Some(blob)
            })
            .collect();

        Ok(blobs)
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

        let request = DispersalRequest {
            data,
            metadata: MetaData {
                app_id: self.get_app_id()?,
                index: Default::default(),
            },
        };

        let blob_id = self.send_dispersal_request(request).await?;
        let mut nr_of_retries = 0;
        loop {
            info!("Requesting cryptarchia info...");

            for url in self.config.validator_rpcs.split(",") {
                let tip = self.get_cryptarchia_info(url).await?;
                info!("Received Tip: {:?}", tip);

                let blobs = self.get_blobs_in_block(url, tip.tip).await?;
                info!("Received blobs: {:?}", blobs);

                if !blobs.is_empty() {
                    // Check if the blob is in the block
                    let blob_found = blobs.iter().any(|blob| blob.id == blob_id);

                    if blob_found {
                        info!("Blob found in block: {:?}", blob_id);
                        break;
                    } else {
                        info!("Blob not found in block: {:?}", blob_id);
                    }
                }

                if nr_of_retries > 30 {
                    error!("Failed to find blob in block after 30 retries");
                    return Err(DAError {
                        error: anyhow::anyhow!("Failed to find blob in block"),
                        is_retriable: false,
                    });
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            nr_of_retries += 1;

            if nr_of_retries > 10 {
                error!("Failed to find blob in block after 5 retries");
                break;
            }
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

macro_rules! serde_bytes_newtype {
    ($newtype:ty, $len:expr) => {
        impl serde::Serialize for $newtype {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                if serializer.is_human_readable() {
                    const_hex::const_encode::<$len, false>(&self.0)
                        .as_str()
                        .serialize(serializer)
                } else {
                    self.0.serialize(serializer)
                }
            }
        }

        impl<'de> serde::Deserialize<'de> for $newtype {
            fn deserialize<D>(deserializer: D) -> Result<$newtype, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                if deserializer.is_human_readable() {
                    let s = <&str>::deserialize(deserializer)?;
                    const_hex::decode_to_array(s)
                        .map(Self)
                        .map_err(serde::de::Error::custom)
                } else {
                    <[u8; $len]>::deserialize(deserializer).map(Self)
                }
            }
        }
    };
}

serde_bytes_newtype!(HeaderId, 32);
