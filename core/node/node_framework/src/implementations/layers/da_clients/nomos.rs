use zksync_config::configs::da_client::nomos::{NomosDaConfig, NomosSecrets};
use zksync_da_client::DataAvailabilityClient;
use zksync_da_clients::nomos::client::NomosDaClient;
use zksync_node_framework_derive::IntoContext;

use crate::{implementations::resources::da_client::DAClientResource, WiringError, WiringLayer};

#[derive(Debug)]
pub struct NomosWiringLayer {
    config: NomosDaConfig,
    secret: NomosSecrets,
}

impl NomosWiringLayer {
    pub fn new(config: NomosDaConfig, secret: NomosSecrets) -> Self {
        Self { config, secret }
    }
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub client: DAClientResource,
}

#[async_trait::async_trait]
impl WiringLayer for NomosWiringLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "nomos_client_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let client: Box<dyn DataAvailabilityClient> =
            Box::new(NomosDaClient::new(self.config, self.secret)?);

        Ok(Self::Output {
            client: DAClientResource(client),
        })
    }
}
