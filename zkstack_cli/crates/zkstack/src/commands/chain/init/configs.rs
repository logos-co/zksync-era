use anyhow::Context;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{
    copy_configs, traits::SaveConfigWithBasePath, ChainConfig, ContractsConfig, EcosystemConfig,
};
use zksync_basic_types::Address;

use crate::{
    commands::{
        chain::{
            args::init::{
                configs::{InitConfigsArgs, InitConfigsArgsFinal},
                da_configs::ValidiumType,
            },
            genesis,
            utils::encode_ntv_asset_id,
        },
        portal::update_portal_config,
    },
    messages::{
        MSG_CHAIN_CONFIGS_INITIALIZED, MSG_CHAIN_NOT_FOUND_ERR,
        MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR,
    },
    utils::{
        consensus::{generate_consensus_keys, set_consensus_secrets, set_genesis_specs},
        ports::EcosystemPortsScanner,
    },
};

pub async fn run(args: InitConfigsArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;
    let chain_config = ecosystem_config
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    let args = args.fill_values_with_prompt(&chain_config);

    init_configs(&args, shell, &ecosystem_config, &chain_config).await?;
    logger::outro(MSG_CHAIN_CONFIGS_INITIALIZED);

    Ok(())
}

pub async fn init_configs(
    init_args: &InitConfigsArgsFinal,
    shell: &Shell,
    ecosystem_config: &EcosystemConfig,
    chain_config: &ChainConfig,
) -> anyhow::Result<ContractsConfig> {
    // Port scanner should run before copying configs to avoid marking initial ports as assigned
    let mut ecosystem_ports = EcosystemPortsScanner::scan(shell, Some(&chain_config.name))?;
    copy_configs(shell, &ecosystem_config.link_to_code, &chain_config.configs)?;

    if !init_args.no_port_reallocation {
        ecosystem_ports.allocate_ports_in_yaml(
            shell,
            &chain_config.path_to_general_config(),
            chain_config.id,
        )?;
    }

    let general_config = chain_config.get_general_config().await?;
    let prover_data_handler_url = general_config.proof_data_handler_url()?;

    let consensus_keys = generate_consensus_keys();
    let mut general_config = general_config.patched();
    if let Some(url) = prover_data_handler_url {
        general_config.set_prover_gateway_url(url)?;
    }
    set_genesis_specs(&mut general_config, chain_config, &consensus_keys)?;

    match &init_args.validium_config {
        Some(ValidiumType::NoDA) => {
            general_config.set_no_da_client()?;
        }
        None | Some(ValidiumType::EigenDA) => {
            general_config.remove_da_client();
        }
        Some(ValidiumType::Avail((avail_config, _))) => {
            general_config.set_avail_client(avail_config)?;
        }
        Some(ValidiumType::Nomos((nomos_config, _))) => {
            general_config.set_nomos_client(nomos_config)?;
        }
    }
    general_config.save().await?;

    // Initialize genesis config
    let mut genesis_config = chain_config.get_genesis_config().await?.patched();
    genesis_config.update_from_chain_config(chain_config)?;
    genesis_config.save().await?;

    // Initialize contracts config
    let mut contracts_config = ecosystem_config.get_contracts_config()?;
    contracts_config.l1.diamond_proxy_addr = Address::zero();
    contracts_config.l1.governance_addr = Address::zero();
    contracts_config.l1.chain_admin_addr = Address::zero();
    contracts_config.l1.base_token_addr = chain_config.base_token.address;
    contracts_config.l1.base_token_asset_id = Some(encode_ntv_asset_id(
        chain_config.l1_network.chain_id().into(),
        contracts_config.l1.base_token_addr,
    ));
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    // Initialize secrets config
    let mut secrets = chain_config.get_secrets_config().await?.patched();
    secrets.set_l1_rpc_url(init_args.l1_rpc_url.clone())?;
    set_consensus_secrets(&mut secrets, &consensus_keys)?;
    match &init_args.validium_config {
        None | Some(ValidiumType::NoDA) | Some(ValidiumType::EigenDA) => { /* Do nothing */ }
        Some(ValidiumType::Avail((_, avail_secrets))) => {
            secrets.set_avail_secrets(avail_secrets)?;
        }
        Some(ValidiumType::Nomos((_, nomos_secrets))) => {
            secrets.set_nomos_secrets(nomos_secrets)?;
        }
    }
    secrets.save().await?;

    genesis::database::update_configs(init_args.genesis_args.clone(), shell, chain_config).await?;

    update_portal_config(shell, chain_config)
        .await
        .context(MSG_PORTAL_FAILED_TO_CREATE_CONFIG_ERR)?;

    Ok(contracts_config)
}
