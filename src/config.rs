use serde::{ Deserialize, Deserializer, Serialize };
use std::{ env, fs::File, io::Read };

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub bot: BotConfig,
    pub rpc: RpcConfig,
    pub spam: SpamConfig,
    pub jito: JitoConfig,
    pub wallet: WalletConfig,
    pub flashloan: FlashloanConfig,
    pub auto: AutoConfig,
    pub stop: StopConfig,
    pub markets_file: Option<Vec<FileConfig>>,
    pub lookup_tables_file: Option<Vec<FileConfig>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MarketsInfo {
    pub group: Vec<MarketsGroupInfo>,
}

impl MarketsInfo {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let config: MarketsInfo = toml::from_str(&contents)?;
        Ok(config)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TokensInfo {
    pub tokens: Vec<String>,
}

impl TokensInfo {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let config: TokensInfo = toml::from_str(&contents)?;
        Ok(config)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct LutsInfo {
    pub luts: Vec<String>,
}

impl LutsInfo {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let config: LutsInfo = if path.contains(".txt") {
            let mut luts = vec![];
            contents.lines().for_each(|line| {
                let first = line.split_whitespace().next();
                if first.is_some() {
                    luts.push(first.unwrap().to_string());
                }
            });
            let config: LutsInfo = LutsInfo { luts };
            config
        } else {
            let config: LutsInfo = toml::from_str(&contents)?;
            config
        };
        Ok(config)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MarketsGroupInfo {
    pub markets: Vec<String>,
    pub luts: Option<Vec<String>>,
}

// #[derive(Debug, Deserialize, Clone)]
// pub struct SenderConfig {
//     pub enabled: bool,
//     pub process_delay: u64,
//     pub priority_fee: AmountRange,
//     pub urls: Vec<String>,
//     pub tip_config: AmountRange,
//     pub max_spam_tx_len_after_simulation_success: u32,
//     // pub block_engine_strategy: BlockEngineStrategy,
//     // pub enable_single_transaction_bundle: bool,
// }

#[derive(Debug, Deserialize, Clone)]
pub struct JitoConfig {
    pub enabled: bool,
    pub process_delay: u64,
    pub uuid: String,
    pub block_engine_urls: Vec<String>,
    pub tip_config: AmountRange,
    pub use_bundle_sender: Option<bool>,
    // pub block_engine_strategy: BlockEngineStrategy,
    // pub enable_single_transaction_bundle: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StopConfig {
    pub min_balance_lamports: u64,
    pub unwrap_amount_lamports: u64,
    pub should_unwrap: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AmountRange {
    pub from: u64,
    pub to: u64,
}

// #[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
// #[serde(rename_all = "PascalCase")]
// pub enum BlockEngineStrategy {
//     OneByOne,
//     AllAtOnce,
// }

#[derive(Debug, Deserialize, Clone)]
pub struct BotConfig {
    pub compute_unit_limit: u64,
    pub blockhash_refresh_interval_ms: Option<u64>,
    pub execution_profile: Option<String>,
    pub route_cooldown_seconds: Option<u64>,
    pub pool_cache_ttl_ms: Option<u64>,
    // pub process_delay: u64,
    pub no_failure_mode: bool,
    pub merge_mints: Option<bool>,
    pub run_after_simulation_profit: Option<bool>,
    pub new_version: Option<bool>,
    pub log: Option<bool>,
    // pub reuse_existing_luts: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AutoConfig {
    pub enabled: bool,
    // pub server_url: String,
    pub refresh_interval_in_secs: u64,
    pub force_two_mints: Option<bool>,
    pub filters: Option<AutoFilter>,
    // pub trigger_mode: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AutoFilter {
    pub limit: Option<u64>,
    pub min_tx_len: Option<u64>,
    pub min_pool_wsol_liquidity: Option<u64>,
    pub max_pool_wsol_liquidity: Option<u64>,
    pub duration: Option<u64>,
    pub min_profit: Option<u64>,
    pub min_profit_per_arb: Option<u64>,
    pub min_roi: Option<f64>,
    pub ignore_offchain_bots: Option<bool>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct RoutingConfig {
    pub mint_config_list: Vec<MintConfig>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct FileConfig {
    pub enabled: bool,
    pub path: String,
    pub update_seconds: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FlashloanConfig {
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone, Default, Serialize)]
pub struct MintConfig {
    pub mint: String,
    pub pump_pool_list: Option<Vec<String>>,
    pub meteora_dlmm_pool_list: Option<Vec<String>>,
    pub raydium_pool_list: Option<Vec<String>>,
    pub raydium_cp_pool_list: Option<Vec<String>>,
    pub meteora_damm_v2_pool_list: Option<Vec<String>>,
    pub lookup_table_accounts: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RpcConfig {
    #[serde(deserialize_with = "serde_string_or_env")]
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SpamConfig {
    pub enabled: bool,
    pub process_delay: u64,
    pub sending_rpc_urls: Vec<String>,
    pub priority_fee: AmountRange,
    pub endpoint_jitter_ms: Option<u64>,
    pub max_spam_tx_len_after_simulation_success: Option<u32>,
    pub max_retries: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WalletConfig {
    #[serde(deserialize_with = "serde_string_or_env")]
    pub private_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArbMintInfo {
    pub mint: String,
    pub total_profit: u64,
    pub total_volume: u64,
    pub roi: f64,
    pub arbs_count: u64,
    pub total_fee: u64,
    pub total_wsol_liquidity: f64,
    pub pool_ids: Vec<String>,
    pub pool_ids_info: Vec<PoolInfo>,
    pub lookup_table_accounts: Vec<String>,
    pub txs: Vec<TxInfo>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TxInfo {
    url: String,
    profit: u64,
    fee: u64,
    payer: String,
    slot: u64,
    is_whitelisted: bool,
    // roi: f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TokenResult {
    pub count: u64,
    pub arb_mint_info: Vec<ArbMintInfo>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PoolInfo {
    pub pool_type: PoolType,
    pub sol_vault: String,
    pub pool_id: String,
    pub wsol_liquidity: f64,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum PoolType {
    PumpAmm,
    Dlmm,
    Cpmm,
    Amm,
    DAmmV2,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoRoutingConfig {
    pub routing: RoutingConfig,
}

pub fn serde_string_or_env<'de, D>(deserializer: D) -> Result<String, D::Error>
    where D: Deserializer<'de>
{
    let value_or_env = String::deserialize(deserializer)?;
    let value = match value_or_env.chars().next() {
        Some('$') =>
            env
                ::var(&value_or_env[1..])
                .unwrap_or_else(|_| panic!("reading `{}` from env", &value_or_env[1..])),
        _ => value_or_env,
    };
    Ok(value)
}

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }
}

// #[derive(Default, Debug, Clone)]
// pub struct ProfitState {
//     pub is_profitable: bool,
//     pub count: u32,
//     pub mint: String,
// }
