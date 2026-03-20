#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(deprecated)]
use crate::config::*;
use crate::constants::{
    BOT_PROGRAM_ID,
    DURATION_DEFAULT_VALUE,
    JITO_TIP_ACCOUNTS,
    MAX_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE,
    MIN_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE,
    MIN_PROFIT_DEFAULT_VALUE,
    MIN_PROFIT_PER_ARB_DEFAULT_VALUE,
    MIN_ROI_DEFAULT_VALUE,
    MIN_TX_LEN_DEFAULT_VALUE,
    MINTS_LIMIT_DEFAULT_VALUE,
    VAULT_AUTH,
    WHITELIST_ONLY_DEFAULT_VALUE,
    sol_mint,
};
use crate::jito::{ _send_txs_using_jito_all_at_once, send_tx_using_jito_all_at_once };
use crate::lut::{ create_lut, extend_lut, find_luts };
use crate::refresh::{ get_swap_ix };
use anyhow::{ Context, Result };
use chrono::Utc;
use rand::{ random, thread_rng };
use reqwest::Client;
use serde::{ Deserialize, Serialize };
use serde_json::{ json, Value };
use solana_account_decoder_client_types::UiAccountData;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_filter::MemcmpEncodedBytes;
use solana_client::rpc_request::TokenAccountsFilter;
use solana_client::rpc_response::RpcKeyedAccount;
use solana_sdk::address_lookup_table::{ AddressLookupTableAccount };
use solana_sdk::commitment_config::CommitmentLevel;
use solana_sdk::hash::Hash;
use solana_sdk::instruction::{ AccountMeta, Instruction, InstructionError };
use solana_sdk::message::v0::Message;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_instruction::transfer;
use solana_sdk::transaction::{ Transaction, TransactionError, VersionedTransaction };
use solana_sdk::{
    address_lookup_table::state::AddressLookupTable,
    compute_budget::ComputeBudgetInstruction,
};
use rand::seq::SliceRandom;
use spl_associated_token_account::{
    get_associated_token_address,
    get_associated_token_address_with_program_id,
};
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use spl_token::instruction::{ close_account, sync_native };
use spl_token::native_mint;
use tokio::fs::OpenOptions;
use tokio::time::sleep;
use tracing_subscriber::fmt::format;
use std::collections::{ HashMap, HashSet };
use std::fs;
use std::hash::{ Hash as StdHash, Hasher };
use std::collections::hash_map::DefaultHasher;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{ Duration, Instant };
use tokio::sync::Mutex;
use tracing::{ debug, error, info, warn };
use spl_token::state::Account as TokenAccount;
use solana_client::rpc_config::{
    RpcAccountInfoConfig,
    RpcProgramAccountsConfig,
    RpcSimulateTransactionConfig,
};
use solana_sdk::{ commitment_config::CommitmentConfig };
// use solana_account_decoder::{ UiAccountEncoding, parse_account_data::parse_account_data };
// use solana_rpc_client_api::config::RpcAccountInfoConfig;
use solana_sdk::account::ReadableAccount;
use solana_sdk::program_pack::Pack;
use solana_sdk::{ compute_budget, pubkey, system_program };
use axum::{ routing::{ get }, Json, Router, extract::{ State, Query } };
use rand::Rng;
use tokio::io::AsyncWriteExt;
use rand::prelude::IndexedRandom;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TriggerQuery {
    mint: String,
}

// async fn run_trigger(
//     State(state): State<Arc<Mutex<Vec<ProfitState>>>>,
//     Query(filter): Query<TriggerQuery>
// ) -> Json<()> {
//     let mint = filter.mint;
//     let mut guard = state.lock().await;
//     for profit_state in (*guard).iter_mut() {
//         if (*profit_state).mint == mint {
//             (*profit_state).is_profitable = true;
//             info!("Trigger enabled: {}", mint);
//             return Json(());
//         }
//     }
//     warn!("Trigger received, but can not find mint. {}", mint);
//     Json(())
// }

pub async fn stop_or_unwrap(rpc_client: Arc<RpcClient>, config: Arc<Config>, payer: &Keypair) {
    let payer_pubkey = payer.pubkey();
    let min_balance_lamports = config.stop.min_balance_lamports;
    let should_unwrap = config.stop.should_unwrap;
    let unwrap_amount_lamports = config.stop.unwrap_amount_lamports;
    loop {
        let balance = rpc_client.get_balance(&payer_pubkey);
        if let Ok(amount) = balance {
            if amount < min_balance_lamports {
                warn!("Payer Sol balance is {} lower than {}.", amount, min_balance_lamports);
                if should_unwrap == false {
                    std::process::exit(1);
                } else {
                    let wsol_ata = get_associated_token_address(&payer_pubkey, &native_mint::id());
                    let wsol_amount = rpc_client.get_token_account_balance(&wsol_ata);
                    if let Ok(wsol_amount) = wsol_amount {
                        let current_wsol_amount = wsol_amount.amount
                            .parse::<u64>()
                            .unwrap_or_default();
                        if current_wsol_amount > unwrap_amount_lamports {
                            let ixs = vec![
                                ComputeBudgetInstruction::set_compute_unit_limit(50_000),
                                ComputeBudgetInstruction::set_compute_unit_price(10_000),
                                close_account(
                                    &spl_token::id(),
                                    &wsol_ata,
                                    &payer_pubkey,
                                    &payer_pubkey,
                                    &[]
                                ).unwrap(),
                                create_associated_token_account_idempotent(
                                    &payer_pubkey,
                                    &payer_pubkey,
                                    &native_mint::id(),
                                    &spl_token::id()
                                ),
                                transfer(
                                    &payer_pubkey,
                                    &wsol_ata,
                                    current_wsol_amount - unwrap_amount_lamports
                                ),
                                sync_native(&spl_token::id(), &wsol_ata).unwrap()
                            ];
                            let (recent_blockhash, _) = rpc_client
                                .get_latest_blockhash_with_commitment(CommitmentConfig {
                                    commitment: solana_sdk::commitment_config::CommitmentLevel::Finalized,
                                })
                                .expect("failed to get latest blockhash");
                            let tx = Transaction::new_signed_with_payer(
                                &ixs,
                                Some(&payer_pubkey),
                                &[&payer],
                                recent_blockhash
                            );
                            let sig = rpc_client.send_and_confirm_transaction(&tx);
                            if let Ok(sig) = sig {
                                info!(
                                    "Unwrapped {} lamports. https://solscan.io/tx/{}",
                                    unwrap_amount_lamports,
                                    sig
                                );
                            } else {
                                sleep(Duration::from_secs(5)).await;
                            }
                        } else {
                            warn!(
                                "Current Wsol amount is not enough and stopping bot.({} < {})",
                                current_wsol_amount,
                                unwrap_amount_lamports
                            );
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
        sleep(Duration::from_secs(30)).await;
    }
}

#[derive(Debug, Clone, Default)]
pub struct GlobalState {
    pub ixs: Vec<SwapInstructionInfo>,
    pub luts: Vec<AddressLookupTableAccount>,
    pub blockhash: Hash,
    pub is_profitable: Vec<ProfitableState>,
}

#[derive(Debug, Clone, Default)]
pub struct ProfitableState {
    pub is_profitable: bool,
    pub count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct MintInfo {
    pub mint: Pubkey,
    pub mint_owner: Pubkey,
}

#[derive(Debug, Clone)]
pub struct SwapInstructionInfo {
    pub mints_list: Vec<MintInfo>,
    pub ix: Instruction,
    pub luts: Vec<AddressLookupTableAccount>,
}

#[derive(Debug, Default, Clone)]
struct SendTelemetry {
    attempted: u64,
    ok: u64,
    err: u64,
    cooldown_skips: u64,
}

fn route_key(ix: &Instruction) -> u64 {
    let mut hasher = DefaultHasher::new();
    StdHash::hash(&ix.program_id, &mut hasher);
    StdHash::hash(&ix.data, &mut hasher);
    StdHash::hash(&ix.accounts.len(), &mut hasher);
    hasher.finish()
}

fn profile_fee_multiplier(profile: &str, is_profitable: bool) -> f64 {
    match profile {
        "calm" => { if is_profitable { 1.05 } else { 0.95 } }
        "congested" => { if is_profitable { 1.45 } else { 1.2 } }
        _ => { if is_profitable { 1.2 } else { 1.0 } }
    }
}

pub async fn run_spam_mode(
    global_state: Arc<Mutex<GlobalState>>,
    config: Arc<Config>,
    rpc_client: Arc<RpcClient>
) -> Result<()> {
    let payer = load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    let payer_pubkey = payer.pubkey();
    let SpamConfig {
        process_delay,
        sending_rpc_urls,
        priority_fee,
        max_spam_tx_len_after_simulation_success,
        max_retries,
        ..
    } = &config.spam;
    let max_spam_tx_len_after_simulation_success =
        max_spam_tx_len_after_simulation_success.unwrap_or(100);
    let sending_rpc_clients = if sending_rpc_urls.len() > 0 {
        sending_rpc_urls
            .iter()
            .map(|url| Arc::new(RpcClient::new(url.clone())))
            .collect::<Vec<_>>()
    } else {
        vec![rpc_client.clone()]
    };

    let duration = Duration::from_millis(*process_delay);
    let compute_unit_limit = config.bot.compute_unit_limit;
    let compute_budget_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(
        compute_unit_limit as u32
    );
    let priority_fee = priority_fee.clone();

    let max_retries = max_retries.unwrap_or(1);
    let endpoint_jitter_ms = config.spam.endpoint_jitter_ms.unwrap_or(7);
    let profile = config.bot.execution_profile.clone().unwrap_or("normal".to_string());
    let route_cooldown = Duration::from_secs(config.bot.route_cooldown_seconds.unwrap_or(5));
    let mut route_failures: HashMap<u64, u32> = HashMap::new();
    let mut route_cooldowns: HashMap<u64, Instant> = HashMap::new();
    let mut endpoint_stats: Vec<(u64, u64)> = vec![(0, 0); sending_rpc_clients.len()];
    let mut telemetry = SendTelemetry::default();
    let mut last_telemetry_log = Instant::now();

    let run_after_simulation_profit = config.bot.run_after_simulation_profit.unwrap_or_default();

    loop {
        let state = {
            let guard = global_state.lock().await;
            guard.clone()
        };

        let default_luts: Vec<AddressLookupTableAccount> = state.luts.clone();
        // if default_luts.len() == 0 {
        //     sleep(duration).await;
        //     continue;
        // }
        let profitable_state = state.is_profitable;
        let blockhash = state.blockhash;
        for (index, ix_info) in state.ixs.iter().enumerate() {
            let key = route_key(&ix_info.ix);
            if
                route_cooldowns
                    .get(&key)
                    .map(|until| *until > Instant::now())
                    .unwrap_or(false)
            {
                telemetry.cooldown_skips += 1;
                continue;
            }
            if ix_info.ix.accounts.first().map(|a| a.pubkey) != Some(payer_pubkey) {
                warn!(
                    "Skipping swap ix with mismatched payer signer. expected={}, got={}",
                    payer_pubkey,
                    ix_info.ix.accounts.first().map(|a| a.pubkey).unwrap_or_default()
                );
                continue;
            }
            telemetry.attempted += 1;
            let mut luts: Vec<AddressLookupTableAccount> = ix_info.luts.clone();

            if luts.len() == 0 {
                luts.extend(default_luts.clone());
            }

            let is_profitable = profitable_state
                .get(index)
                .unwrap_or(&ProfitableState::default()).is_profitable;
            let mut priority_fee_lamports = rand
                ::rng()
                .random_range(priority_fee.from..priority_fee.to);
            let fee_multiplier = profile_fee_multiplier(&profile, is_profitable);
            priority_fee_lamports = ((priority_fee_lamports as f64) * fee_multiplier) as u64;
            let compute_unit_price = (priority_fee_lamports * 1000000_u64) / compute_unit_limit;
            let compute_budget_price_ix =
                ComputeBudgetInstruction::set_compute_unit_price(compute_unit_price);
            let ixs: Vec<Instruction> = vec![
                compute_budget_limit_ix.clone(),
                compute_budget_price_ix,
                ix_info.ix.clone()
            ];
            let message = Message::try_compile(&payer_pubkey, &ixs, &luts, blockhash).unwrap();
            let tx = VersionedTransaction::try_new(
                solana_sdk::message::VersionedMessage::V0(message),
                &[&payer]
            ).unwrap();
            if run_after_simulation_profit && is_profitable == false {
                let simul_result = rpc_client.simulate_transaction_with_config(
                    &tx,
                    RpcSimulateTransactionConfig {
                        replace_recent_blockhash: true,
                        ..Default::default()
                    }
                );
                if let Ok(simul_result) = simul_result {
                    if let Some(logs) = simul_result.value.logs {
                        let profitable_log = logs.iter().find(|log| log.contains("real_profit"));
                        if profitable_log.is_some() && simul_result.value.err.is_none() {
                            let now = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
                            let mut file = OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open("log.txt").await?;
                            let line = format!("[{}] {}\n", now, format!("{:#?}", logs));
                            file.write_all(line.as_bytes()).await?;

                            info!("Found profitable route: {:?}", profitable_log.unwrap());
                            route_failures.remove(&key);
                            route_cooldowns.remove(&key);
                            {
                                let mut guard = global_state.lock().await;
                                (*guard).is_profitable[index].is_profitable = true;
                            }
                        } else {
                            info!("{:#?}", logs);
                            if let Some(err) = simul_result.value.err.clone() {
                                if
                                    err !=
                                    TransactionError::InstructionError(
                                        2,
                                        InstructionError::Custom(81)
                                    )
                                {
                                    info!(
                                        "Unknown Error: {:?}",
                                        simul_result.value.err.clone().unwrap()
                                    );

                                    info!("{:#?}", logs);
                                    let now = Utc::now()
                                        .format("%Y-%m-%d %H:%M:%S%.3f")
                                        .to_string();
                                    let mut file = OpenOptions::new()
                                        .create(true)
                                        .append(true)
                                        .open("unknown_error.txt").await?;
                                    let line = format!("[{}] {}\n", now, format!("{:#?}", logs));
                                    file.write_all(line.as_bytes()).await?;
                                }
                            }
                        }
                    }
                } else {
                    // error!("Error while simulating {:?}", simul_result.err());
                }
            } else {
                let mut endpoint_indices: Vec<usize> = (0..sending_rpc_clients.len()).collect();
                endpoint_indices.sort_by_key(|idx| {
                    let (ok, err) = endpoint_stats[*idx];
                    (ok as i64) - (err as i64)
                });
                endpoint_indices.reverse();
                let mut handles = Vec::with_capacity(endpoint_indices.len());
                for i in endpoint_indices {
                    let tx = tx.clone();
                    let client = sending_rpc_clients[i].clone();
                    handles.push(tokio::spawn(async move {
                        let result = client.send_transaction_with_config(
                            &tx,
                            solana_client::rpc_config::RpcSendTransactionConfig {
                                skip_preflight: true,
                                max_retries: Some(max_retries as usize),
                                preflight_commitment: Some(CommitmentLevel::Confirmed),
                                ..Default::default()
                            }
                        );
                        (i, result)
                    }));
                    if endpoint_jitter_ms > 0 {
                        let jitter = rand::rng().random_range(0..=endpoint_jitter_ms);
                        sleep(Duration::from_millis(jitter)).await;
                    }
                }
                let mut any_success = false;
                for handle in handles {
                    if let Ok((i, result)) = handle.await {
                        match result {
                            Ok(sig) => {
                                any_success = true;
                                telemetry.ok += 1;
                                endpoint_stats[i].0 += 1;
                                info!("Transaction sent successfully through RPC client {}: {}", i, sig);
                            }
                            Err(_) => {
                                telemetry.err += 1;
                                endpoint_stats[i].1 += 1;
                            }
                        }
                    }
                }
                if any_success {
                    route_failures.remove(&key);
                    route_cooldowns.remove(&key);
                } else {
                    let fail = route_failures.entry(key).or_insert(0);
                    *fail += 1;
                    if *fail >= 3 {
                        route_cooldowns.insert(key, Instant::now() + route_cooldown);
                    }
                }
                if run_after_simulation_profit {
                    let mut guard = global_state.lock().await;
                    if (*guard).is_profitable.len() >= index + 1 {
                        (*guard).is_profitable[index].count += 1;
                        if
                            (*guard).is_profitable[index].count >
                            max_spam_tx_len_after_simulation_success
                        {
                            (*guard).is_profitable[index].count = 0;
                            (*guard).is_profitable[index].is_profitable = false;
                        }
                    }
                }
            }
        }
        if last_telemetry_log.elapsed() >= Duration::from_secs(5) {
            let rate =
                if telemetry.attempted == 0 { 0.0 } else { (telemetry.ok as f64) / (telemetry.attempted as f64) };
            info!(
                "telemetry profile={} attempted={} ok={} err={} cooldown_skips={} success_rate={:.2}",
                profile,
                telemetry.attempted,
                telemetry.ok,
                telemetry.err,
                telemetry.cooldown_skips,
                rate
            );
            last_telemetry_log = Instant::now();
        }
        sleep(duration).await;
    }
}

pub fn get_jito_ix(tip_config: &AmountRange, payer_pubkey: &Pubkey) -> Instruction {
    let jito_tip_lamports = rand::rng().random_range(tip_config.from..tip_config.to);
    let mut rng = thread_rng();
    let to_pubkey = JITO_TIP_ACCOUNTS.choose(&mut rng).unwrap();
    let ix = transfer(payer_pubkey, to_pubkey, jito_tip_lamports);
    return ix;
}

pub async fn run_jito_mode(
    global_state: Arc<Mutex<GlobalState>>,
    config: Arc<Config>
) -> Result<()> {
    let payer = load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    let payer_pubkey = payer.pubkey();
    let JitoConfig { process_delay, block_engine_urls, tip_config, uuid, .. } = &config.jito;
    let use_bundle_sender = config.jito.use_bundle_sender.unwrap_or(false);
    let profile = config.bot.execution_profile.clone().unwrap_or("normal".to_string());
    let route_cooldown = Duration::from_secs(config.bot.route_cooldown_seconds.unwrap_or(5));
    let mut route_failures: HashMap<u64, u32> = HashMap::new();
    let mut route_cooldowns: HashMap<u64, Instant> = HashMap::new();
    let duration = Duration::from_millis(*process_delay);
    let compute_unit_limit = config.bot.compute_unit_limit;
    let compute_budget_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(
        compute_unit_limit as u32
    );

    loop {
        let state = {
            let guard = global_state.lock().await;
            guard.clone()
        };

        let default_luts: Vec<AddressLookupTableAccount> = state.luts.clone();

        let blockhash = state.blockhash;
        for (_index, ix_info) in state.ixs.iter().enumerate() {
            let key = route_key(&ix_info.ix);
            if
                route_cooldowns
                    .get(&key)
                    .map(|until| *until > Instant::now())
                    .unwrap_or(false)
            {
                continue;
            }
            if ix_info.ix.accounts.first().map(|a| a.pubkey) != Some(payer_pubkey) {
                warn!(
                    "Skipping jito ix with mismatched payer signer. expected={}, got={}",
                    payer_pubkey,
                    ix_info.ix.accounts.first().map(|a| a.pubkey).unwrap_or_default()
                );
                continue;
            }
            let mut adjusted_tip = tip_config.clone();
            let profile_mult = profile_fee_multiplier(&profile, true);
            adjusted_tip.from = ((adjusted_tip.from as f64) * profile_mult) as u64;
            adjusted_tip.to = ((adjusted_tip.to as f64) * profile_mult) as u64;
            if adjusted_tip.to <= adjusted_tip.from {
                adjusted_tip.to = adjusted_tip.from + 1;
            }
            let jito_tip_ix = get_jito_ix(&adjusted_tip, &payer_pubkey);
            let ixs: Vec<Instruction> = vec![
                compute_budget_limit_ix.clone(),
                jito_tip_ix.clone(),
                ix_info.ix.clone()
            ];

            let mut luts: Vec<AddressLookupTableAccount> = ix_info.luts.clone();

            if luts.len() == 0 {
                luts.extend(default_luts.clone());
            }

            let message = Message::try_compile(&payer_pubkey, &ixs, &luts, blockhash).unwrap();
            let tx: VersionedTransaction = VersionedTransaction::try_new(
                solana_sdk::message::VersionedMessage::V0(message),
                &[&payer]
            ).unwrap();
            let results = if use_bundle_sender {
                _send_txs_using_jito_all_at_once(&[tx.clone()], block_engine_urls, uuid.clone()).await
            } else {
                send_tx_using_jito_all_at_once(&tx, block_engine_urls, uuid).await
            };
            let has_success = results.iter().any(|r| r.is_some());
            if has_success {
                route_failures.remove(&key);
                route_cooldowns.remove(&key);
            } else {
                let fail = route_failures.entry(key).or_insert(0);
                *fail += 1;
                if *fail >= 3 {
                    route_cooldowns.insert(key, Instant::now() + route_cooldown);
                }
            }
        }
        sleep(duration).await;
    }
}

pub async fn update_lookup_tables_info_from_file(
    lookup_tables: FileConfig,
    rpc_client: Arc<RpcClient>,
    state: Arc<Mutex<GlobalState>>
) {
    let FileConfig { path, update_seconds, .. } = lookup_tables;
    let duration = Duration::from_secs(update_seconds);
    loop {
        let luts_info = LutsInfo::load(&path);
        let luts_info = match luts_info {
            Ok(luts_info) => luts_info,
            Err(e) => {
                error!("Can't read luts file ({}): {}", path, e);
                sleep(duration).await;
                continue;
            }
        };
        let rpc_client = rpc_client.clone();
        let current_slot = rpc_client.get_slot().unwrap_or_default();
        let mut lookup_table_accounts = vec![];
        let mut luts = vec![];
        luts.extend_from_slice(&luts_info.luts);
        for lut in luts.iter() {
            let lut_account_pubkey = Pubkey::from_str(lut).unwrap();
            let account = rpc_client.get_account(&lut_account_pubkey);
            if let Ok(account) = account {
                match AddressLookupTable::deserialize(&account.data) {
                    Ok(lookup_table) => {
                        if lookup_table.meta.deactivation_slot < current_slot {
                            continue;
                        }
                        let lookup_table_account = AddressLookupTableAccount {
                            key: lut_account_pubkey,
                            addresses: lookup_table.addresses.into_owned(),
                        };
                        lookup_table_accounts.push(lookup_table_account);
                    }
                    Err(e) => {
                        error!(
                            "   Failed to deserialize lookup table {}: {}",
                            lut_account_pubkey,
                            e
                        );
                    }
                }
            }
        }
        {
            let mut guard = state.lock().await;
            (*guard).luts.clear();
            (*guard).luts.extend(lookup_table_accounts);
        }
        if update_seconds == 0 {
            break;
        }
        sleep(duration).await;
    }
}

pub async fn update_market_info_from_markets_file(
    market: FileConfig,
    payer_pubkey: &Pubkey,
    rpc_client: Arc<RpcClient>,
    config: Arc<Config>,
    state: Arc<Mutex<GlobalState>>
) {
    let FileConfig { path, update_seconds, .. } = market;
    let duration = Duration::from_secs(update_seconds);

    loop {
        let market_info = MarketsInfo::load(&path);
        let market_info = match market_info {
            Ok(market_info) => market_info,
            Err(e) => {
                error!("Can't read market_info file ({}): {}", path, e);
                sleep(duration).await;
                continue;
            }
        };
        let rpc_client = rpc_client.clone();
        let mut swap_ixs = vec![];
        let mut profit_state = vec![];
        for market_group in market_info.group.iter() {
            if market_group.markets.len() >= 2 {
                let market_info = vec![market_group.clone()];
                let swap_result = get_swap_ix(
                    payer_pubkey,
                    &market_info,
                    None,
                    &rpc_client,
                    &config
                ).await;
                let swap_info = match swap_result {
                    Ok(swap_info) => swap_info.0,
                    Err(e) => {
                        error!("Error while getting swap_ix {:?}, {:?}", market_group.markets, e);
                        continue;
                    }
                };
                swap_ixs.push(swap_info);
                profit_state.push(ProfitableState::default());
            }
        }
        {
            let mut guard = state.lock().await;
            // let mut running_ixs: Vec<SwapInstructionInfo> = vec![];
            // let mut running_profitable_state: Vec<ProfitableState> = vec![];
            // (*guard).is_profitable
            //     .iter()
            //     .enumerate()
            //     .for_each(|(index, profit_state)| {
            //         if profit_state.is_profitable == true {
            //             running_ixs.push((*guard).ixs[index].clone());
            //             running_profitable_state.push((*guard).is_profitable[index].clone());
            //         }
            //     });

            (*guard).ixs.clear();
            (*guard).ixs.extend_from_slice(&swap_ixs);
            // (*guard).ixs.extend_from_slice(&running_ixs);
            (*guard).is_profitable.clear();
            (*guard).is_profitable.extend_from_slice(&profit_state);
            // (*guard).is_profitable.extend_from_slice(&running_profitable_state);
        }
        if update_seconds == 0 {
            break;
        }
        sleep(duration).await;
    }
}

pub async fn create_markets_file_from_tokens(
    tokens: Vec<String>,
    payer_pubkey: &Pubkey,
    rpc_client: &Arc<RpcClient>,
    config: &Arc<Config>
) {
    if tokens.len() > 0 {
        let query = format!(
            "limit={}&maxPoolLen=4&minTxLen={}&minPoolWsolLiquidity={}&maxPoolWsolLiquidity={}&duration={}&minProfit={}",
            200,
            1,
            5,
            75000,
            6000,
            100_000
        );
        let market_group_info = get_tokens_info_by_query(query, tokens).await;
        if let Ok(market_group_info) = market_group_info {
            let sub_groups: Vec<&[MarketsGroupInfo]> = market_group_info.chunks(2).collect();
            if sub_groups.len() > 0 {
                let market_group = sub_groups[0];
                let swap_result = get_swap_ix(
                    payer_pubkey,
                    &market_group.to_vec(),
                    None,
                    &rpc_client,
                    &config
                ).await;
                if let Ok(swap_result) = swap_result {
                    let (_, markets, luts) = swap_result;
                    if create_markets_toml_file(markets, luts).is_ok() {
                        info!("Successfully created markets.toml file.")
                    } else {
                        error!("Failed to write into markets.toml file.")
                    }
                } else {
                    error!("Can't find proper luts info");
                }
            }
        } else {
            error!("Failed to fetch market info from the server.")
        }
    }
}

pub async fn get_hot_mints() -> Result<Vec<MarketsGroupInfo>> {
    let query = format!(
        "limit={}&maxPoolLen=4&minTxLen={}&minPoolWsolLiquidity={}&maxPoolWsolLiquidity={}&duration={}&minProfit={}&minProfitPerArb={}&sortBy=score",
        4,
        10,
        100,
        75000,
        3000,
        10_000_000,
        1_000_000
    );
    let url = format!("http://194.164.217.147:3003/recent-tokens?{}", query);
    let client = Client::new();
    let res = client.get(url).header("Content-Type", "application/json").send().await;
    let mut market_group_info: Vec<MarketsGroupInfo> = vec![];
    if let Ok(response) = res {
        if let Ok(data) = response.json::<serde_json::Value>().await {
            let result: TokenResult = serde_json::from_value(data)?;
            // tracing::info!("fetched hot_tokens result: {:?}", result);
            let json_str = serde_json::to_string_pretty(&result)?;
            fs::write("hot_tokens.json", json_str)?;
            for arb_mint_info in result.arb_mint_info {
                let markets = arb_mint_info.pool_ids.clone();
                let luts = arb_mint_info.lookup_table_accounts.clone();
                market_group_info.push(MarketsGroupInfo { markets, luts: Some(luts) });
            }
            Ok(market_group_info)
        } else {
            anyhow::bail!("Error while fetching hot_token info.");
        }
    } else {
        anyhow::bail!("Arb-Assist Server is not working.");
    }
}

pub fn create_markets_toml_file(markets: Vec<String>, luts: Vec<String>) -> Result<()> {
    let group = MarketsGroupInfo { markets, luts: Some(luts) };
    let market_group_info = MarketsInfo { group: vec![group] };
    let json_str = toml::to_string_pretty(&market_group_info)?;
    fs::write("markets.toml", json_str)?;
    Ok(())
}

pub async fn update_market_info_from_markets_group_info(
    payer_pubkey: &Pubkey,
    rpc_client: &Arc<RpcClient>,
    config: &Arc<Config>,
    state: &Arc<Mutex<GlobalState>>,
    market_group_info: &Result<Vec<MarketsGroupInfo>>
) {
    let merge_mints = config.bot.merge_mints.unwrap_or_default();
    if let Ok(market_group_info) = market_group_info {
        let mut swap_ixs = vec![];
        let mut profit_state = vec![];
        if !merge_mints {
            for market_group in market_group_info.iter() {
                if market_group.markets.len() >= 2 {
                    let market_info = vec![market_group.clone()];
                    let swap_result = get_swap_ix(
                        payer_pubkey,
                        &market_info,
                        None,
                        &rpc_client,
                        &config
                    ).await;
                    let swap_info = match swap_result {
                        Ok(swap_info) => swap_info.0,
                        Err(e) => {
                            error!(
                                "Error while getting luts info for non_merge_minted swap_ix {:?}, {:?}",
                                market_group.markets,
                                e
                            );
                            continue;
                        }
                    };
                    swap_ixs.push(swap_info);
                    profit_state.push(ProfitableState::default());
                }
            }
            {
                let mut guard = state.lock().await;
                (*guard).ixs.clear();
                (*guard).ixs.extend_from_slice(&swap_ixs);
                (*guard).is_profitable.clear();
                (*guard).is_profitable.extend_from_slice(&profit_state);
            }
        } else {
            let force_two_mints = config.auto.force_two_mints.unwrap_or_default();
            let hot_mints_info = if force_two_mints {
                if let Ok(hot_mints_info) = get_hot_mints().await {
                    Some(hot_mints_info)
                } else {
                    None
                }
            } else {
                None
            };

            let sub_groups: Vec<&[MarketsGroupInfo]> = market_group_info.chunks(2).collect();
            for &market_group in sub_groups.iter() {
                let swap_result = get_swap_ix(
                    payer_pubkey,
                    &market_group.to_vec(),
                    hot_mints_info.clone(),
                    &rpc_client,
                    &config
                ).await;
                let swap_info = match swap_result {
                    Ok(swap_info) => swap_info.0,
                    Err(e) => {
                        let pool_ids: Vec<String> = market_group
                            .iter()
                            .map(|g| g.markets.clone())
                            .flatten()
                            .collect();
                        error!(
                            "Error while getting luts info for merge_minted swap_ix {:?} {:?}",
                            pool_ids,
                            e
                        );
                        info!("Finding luts info for each mint");
                        for market_group in market_group.iter() {
                            let market_info = vec![market_group.clone()];
                            let swap_result = get_swap_ix(
                                payer_pubkey,
                                &market_info,
                                None,
                                &rpc_client,
                                &config
                            ).await;
                            match swap_result {
                                Ok(swap_info) => {
                                    swap_ixs.push(swap_info.0);
                                    profit_state.push(ProfitableState::default());
                                }
                                Err(e) => {
                                    error!(
                                        "Error while getting luts info for non-merge_minted swap_ix: {:?} {:?}",
                                        market_info[0].markets,
                                        e
                                    );
                                }
                            }
                        }
                        continue;
                    }
                };
                swap_ixs.push(swap_info);
                profit_state.push(ProfitableState::default());
            }
            {
                let mut guard = state.lock().await;
                (*guard).ixs.clear();
                (*guard).ixs.extend_from_slice(&swap_ixs);
                (*guard).is_profitable.clear();
                (*guard).is_profitable.extend_from_slice(&profit_state);
            }
        }
    }
}

pub async fn run_bot(config_path: &str) -> anyhow::Result<()> {
    let config = Arc::new(Config::load(config_path)?);
    info!("Configuration loaded successfully");

    let payer = load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    let payer_pubkey = payer.pubkey();
    info!("Wallet loaded: {}", payer_pubkey);
    let rpc_client = Arc::new(RpcClient::new(config.rpc.url.clone()));
    let initial_blockhash = rpc_client.get_latest_blockhash()?;
    let state = Arc::new(
        Mutex::new(GlobalState { blockhash: initial_blockhash, ..Default::default() })
    );

    let block_refresh_interval = Duration::from_millis(
        config.bot.blockhash_refresh_interval_ms.unwrap_or(2000)
    );
    let blockhash_client = rpc_client.clone();
    let cloned_state = state.clone();
    tokio::spawn(async move {
        blockhash_refresher(blockhash_client, cloned_state, block_refresh_interval).await;
    });
    let cloned_rpc_client = rpc_client.clone();
    let cloned_payer = Keypair::from_bytes(&payer.to_bytes().clone()).unwrap();
    let cloned_config = config.clone();
    tokio::spawn(async move {
        stop_or_unwrap(cloned_rpc_client, cloned_config, &cloned_payer).await;
    });
    let native_mint = sol_mint();
    let payer_wsol_ata = get_associated_token_address(&payer_pubkey, &native_mint);
    // Checking if payer wsol account exists and if not, create new one
    loop {
        match rpc_client.get_account(&payer_wsol_ata) {
            Ok(_) => {
                info!("   Payer Wsol account exists!");
                break;
            }
            Err(_) => {
                info!("   Payer Wsol account does not exist. Creating it...");

                // Create the instruction to create the associated token account
                let create_ata_ix =
                    spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                        &payer_pubkey, // Funding account
                        &payer_pubkey, // Wallet account
                        &native_mint, // Token mint
                        &spl_token::ID // Token program
                    );

                // Get a recent blockhash
                let blockhash = rpc_client.get_latest_blockhash()?;

                let compute_unit_price_ix =
                    ComputeBudgetInstruction::set_compute_unit_price(100_000);
                let compute_unit_limit_ix =
                    ComputeBudgetInstruction::set_compute_unit_limit(50_000);

                // Create the transaction
                let create_ata_tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
                    &[compute_unit_price_ix, compute_unit_limit_ix, create_ata_ix],
                    Some(&payer_pubkey),
                    &[&payer],
                    blockhash
                );

                // Send the transaction
                match rpc_client.send_and_confirm_transaction(&create_ata_tx) {
                    Ok(sig) => {
                        info!("   Payer wsol account created successfully! Signature: {}", sig);
                    }
                    Err(e) => {
                        info!("   Failed to create payer wsol account: {:?}", e);
                        return Err(anyhow::anyhow!("Failed to create payer wsol account"));
                    }
                }
            }
        }
    }

    let cloned_config = config.clone();
    let cloned_rpc_client = rpc_client.clone();
    let cloned_state = state.clone();
    if cloned_config.auto.enabled {
        let duration = Duration::from_secs(cloned_config.auto.refresh_interval_in_secs);
        let market_group_info = get_auto_mint_info_from_url(&cloned_config).await;
        update_market_info_from_markets_group_info(
            &payer_pubkey,
            &cloned_rpc_client,
            &cloned_config,
            &cloned_state,
            &market_group_info
        ).await;
        tokio::spawn(async move {
            loop {
                sleep(duration).await;
                let cloned_config = cloned_config.clone();

                let market_group_info = get_auto_mint_info_from_url(&cloned_config).await;

                update_market_info_from_markets_group_info(
                    &payer_pubkey,
                    &cloned_rpc_client,
                    &cloned_config,
                    &cloned_state,
                    &market_group_info
                ).await;
            }
        });
    } else {
        // Should read markets from the file at regular interval and also read luts from the file regularly.
        let markets = config.markets_file.clone();
        if markets.is_some() {
            let markets = markets.unwrap();
            for market in markets.iter() {
                let market = market.clone();
                let rpc_client = rpc_client.clone();
                let cloned_config = config.clone();
                let cloned_state = state.clone();
                if market.enabled == true {
                    tokio::spawn(async move {
                        update_market_info_from_markets_file(
                            market,
                            &payer_pubkey,
                            rpc_client,
                            cloned_config,
                            cloned_state
                        ).await;
                    });
                }
            }
        }

        let lookup_tables_file = config.lookup_tables_file.clone();
        if lookup_tables_file.is_some() {
            let lookup_tables_file = lookup_tables_file.unwrap();
            for lookup_tables in lookup_tables_file.iter() {
                let lookup_tables = lookup_tables.clone();
                let rpc_client = rpc_client.clone();
                let cloned_state = state.clone();
                if lookup_tables.enabled == true {
                    tokio::spawn(async move {
                        update_lookup_tables_info_from_file(
                            lookup_tables,
                            rpc_client,
                            cloned_state
                        ).await;
                    });
                }
            }
        }
    }

    if config.spam.enabled {
        let cloned_state = state.clone();
        let cloned_config = config.clone();
        let cloned_rpc_client = rpc_client.clone();
        tokio::spawn(async move {
            loop {
                let cloned_state = cloned_state.clone();
                let cloned_config = cloned_config.clone();
                let cloned_rpc_client = cloned_rpc_client.clone();
                if let Err(e) = run_spam_mode(cloned_state, cloned_config, cloned_rpc_client).await {
                    error!("Error while running run_spam_mode: {e}");
                    sleep(Duration::from_secs(5)).await;
                }
            }
        });
    }
    if config.jito.enabled {
        let cloned_state = state.clone();
        let cloned_config = config.clone();
        tokio::spawn(async move {
            loop {
                let cloned_state = cloned_state.clone();
                let cloned_config = cloned_config.clone();
                if let Err(e) = run_jito_mode(cloned_state, cloned_config).await {
                    error!("Error while running run_jito_mode: {e}");
                    sleep(Duration::from_secs(5)).await;
                }
            }
        });
    }
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn wrap_sol(amount: &f64, config_path: &str) -> anyhow::Result<()> {
    let config = Config::load(config_path)?;
    info!("Configuration loaded successfully");

    let payer = load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    info!("Wallet loaded: {}", payer.pubkey());
    let rpc_client = RpcClient::new(config.rpc.url.clone());

    let wsol_ata = get_associated_token_address(&payer.pubkey(), &native_mint::id());
    info!("{}", wsol_ata);
    let amount_in_lamports = (1_000_000_000_f64 * amount) as u64;
    let ixs = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(50_000),
        ComputeBudgetInstruction::set_compute_unit_price(10_000),
        close_account(&spl_token::id(), &wsol_ata, &payer.pubkey(), &payer.pubkey(), &[]).unwrap(),
        create_associated_token_account_idempotent(
            &payer.pubkey(),
            &payer.pubkey(),
            &native_mint::id(),
            &spl_token::id()
        ),
        transfer(&payer.pubkey(), &wsol_ata, amount_in_lamports),
        sync_native(&spl_token::id(), &wsol_ata).unwrap()
    ];
    let (recent_blockhash, _) = rpc_client
        .get_latest_blockhash_with_commitment(CommitmentConfig {
            commitment: solana_sdk::commitment_config::CommitmentLevel::Finalized,
        })
        .expect("failed to get latest blockhash");
    let tx = Transaction::new_signed_with_payer(
        &ixs,
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash
    );
    let sig = rpc_client.send_and_confirm_transaction(&tx).expect("Failed to send transaction");
    info!("✅ Transaction confirmed! Signature: {}", sig);
    Ok(())
}

pub async fn get_auto_mint_info_result(config: &Config) -> anyhow::Result<RoutingConfig> {
    let client = Client::new();
    let auto_filter = config.auto.filters.clone();
    let auto_filter = auto_filter.unwrap_or(AutoFilter {
        limit: Some(MINTS_LIMIT_DEFAULT_VALUE),
        min_tx_len: Some(MIN_TX_LEN_DEFAULT_VALUE),
        min_pool_wsol_liquidity: Some(MIN_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE),
        max_pool_wsol_liquidity: Some(MAX_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE),
        duration: Some(DURATION_DEFAULT_VALUE),
        min_profit: Some(MIN_PROFIT_DEFAULT_VALUE),
        min_profit_per_arb: Some(MIN_PROFIT_PER_ARB_DEFAULT_VALUE),
        min_roi: Some(MIN_ROI_DEFAULT_VALUE),
        ignore_offchain_bots: Some(WHITELIST_ONLY_DEFAULT_VALUE),
    });
    let query = format!(
        "limit={}&maxPoolLen=4&minTxLen={}&minPoolWsolLiquidity={}&maxPoolWsolLiquidity={}&duration={}&minProfit={}&minProfitPerArb={}&minRoi={}&isWhitelisted={}",
        auto_filter.limit.unwrap_or(MINTS_LIMIT_DEFAULT_VALUE),
        auto_filter.min_tx_len.unwrap_or(MIN_TX_LEN_DEFAULT_VALUE),
        auto_filter.min_pool_wsol_liquidity.unwrap_or(MIN_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE),
        auto_filter.max_pool_wsol_liquidity.unwrap_or(MAX_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE),
        auto_filter.duration.unwrap_or(DURATION_DEFAULT_VALUE),
        auto_filter.min_profit.unwrap_or(MIN_PROFIT_DEFAULT_VALUE),
        auto_filter.min_profit_per_arb.unwrap_or(MIN_PROFIT_PER_ARB_DEFAULT_VALUE),
        auto_filter.min_roi.unwrap_or(MIN_ROI_DEFAULT_VALUE),
        auto_filter.ignore_offchain_bots.unwrap_or(WHITELIST_ONLY_DEFAULT_VALUE)
    );
    let url = format!("http://194.164.217.147:3003/recent-tokens?{}", query);
    // info!("{}", url);
    let res = client.get(url).header("Content-Type", "application/json").send().await;

    if let Ok(response) = res {
        if let Ok(data) = response.json::<serde_json::Value>().await {
            let result: TokenResult = serde_json::from_value(data)?;
            // tracing::info!("fetched hot_tokens result: {:?}", result);
            
            let json_str = serde_json::to_string_pretty(&result)?;
            fs::write("routing.json", json_str)?;
            // info!("✅ Written routing.json");
            let mut routing_config = RoutingConfig {
                mint_config_list: vec![],
            };
            let mut mint_config_list = vec![];
            for arb_mint_info in result.arb_mint_info {
                let mut pump_pool_list: Vec<String> = vec![];
                let mut meteora_dlmm_pool_list: Vec<String> = vec![];
                let mut meteora_damm_v2_pool_list: Vec<String> = vec![];
                let mut raydium_cp_pool_list: Vec<String> = vec![];
                let mut raydium_pool_list: Vec<String> = vec![];
                for pool_id in arb_mint_info.pool_ids_info {
                    match pool_id.pool_type {
                        PoolType::PumpAmm => {
                            pump_pool_list.push(pool_id.pool_id.clone());
                        }
                        PoolType::Dlmm => {
                            meteora_dlmm_pool_list.push(pool_id.pool_id.clone());
                        }
                        PoolType::DAmmV2 => {
                            meteora_damm_v2_pool_list.push(pool_id.pool_id.clone());
                        }
                        PoolType::Cpmm => {
                            raydium_cp_pool_list.push(pool_id.pool_id.clone());
                        }
                        PoolType::Amm => {
                            raydium_pool_list.push(pool_id.pool_id.clone());
                        }
                    }
                }
                mint_config_list.push(MintConfig {
                    mint: arb_mint_info.mint,
                    pump_pool_list: Some(pump_pool_list),
                    meteora_dlmm_pool_list: Some(meteora_dlmm_pool_list),
                    meteora_damm_v2_pool_list: Some(meteora_damm_v2_pool_list),
                    raydium_cp_pool_list: Some(raydium_cp_pool_list),
                    raydium_pool_list: Some(raydium_pool_list),
                    lookup_table_accounts: arb_mint_info.lookup_table_accounts,
                });
            }
            routing_config.mint_config_list = mint_config_list.clone();
            let auto_routing_config = AutoRoutingConfig {
                routing: routing_config.clone(),
            };
            let toml_str = toml::to_string_pretty(&auto_routing_config)?;
            fs::write("routing.toml", toml_str)?;
            info!("✅ Written routing.toml");
            info!("Fetched {} mint list.", mint_config_list.len());
            Ok(routing_config)
        } else {
            anyhow::bail!("Error while fetching token info.")
        }
    } else {
        anyhow::bail!("Error while fetching token info.")
    }
}

pub async fn get_tokens_info_by_query(
    query: String,
    tokens: Vec<String>
) -> anyhow::Result<Vec<MarketsGroupInfo>> {
    let url = format!("http://194.164.217.147:3003/recent-tokens?{}", query);
    let client = Client::new();
    let res = client.get(url).header("Content-Type", "application/json").send().await;
    let mut market_group_info: Vec<MarketsGroupInfo> = vec![];
    if let Ok(response) = res {
        if let Ok(data) = response.json::<serde_json::Value>().await {
            let result: TokenResult = serde_json::from_value(data)?;
            // tracing::info!("fetched hot_tokens result: {:?}", result);
            
            let json_str = serde_json::to_string_pretty(&result)?;
            if result.count > 0 {
                fs::write("tokens_routing.json", json_str)?;
            }
            for arb_mint_info in result.arb_mint_info {
                if tokens.contains(&arb_mint_info.mint) {
                    let markets = arb_mint_info.pool_ids.clone();
                    let luts = arb_mint_info.lookup_table_accounts.clone();
                    market_group_info.push(MarketsGroupInfo { markets, luts: Some(luts) });
                }
            }
            info!("Fetched {} mint list.", market_group_info.len());
            Ok(market_group_info)
        } else {
            anyhow::bail!("Error while fetching token info.")
        }
    } else {
        anyhow::bail!("Arb-Assist Server is not working.")
    }
}

pub async fn get_auto_mint_info_from_url(
    config: &Arc<Config>
) -> anyhow::Result<Vec<MarketsGroupInfo>> {
    let client = Client::new();
    let auto_filter = config.auto.filters.clone();
    let auto_filter = auto_filter.unwrap_or(AutoFilter {
        limit: Some(MINTS_LIMIT_DEFAULT_VALUE),
        min_tx_len: Some(MIN_TX_LEN_DEFAULT_VALUE),
        min_pool_wsol_liquidity: Some(MIN_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE),
        max_pool_wsol_liquidity: Some(MAX_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE),
        duration: Some(DURATION_DEFAULT_VALUE),
        min_profit: Some(MIN_PROFIT_DEFAULT_VALUE),
        min_profit_per_arb: Some(MIN_PROFIT_PER_ARB_DEFAULT_VALUE),
        min_roi: Some(MIN_ROI_DEFAULT_VALUE),
        ignore_offchain_bots: Some(WHITELIST_ONLY_DEFAULT_VALUE),
    });
    let query = format!(
        "limit={}&maxPoolLen=10&minTxLen={}&minPoolWsolLiquidity={}&maxPoolWsolLiquidity={}&duration={}&minProfit={}&minProfitPerArb={}&minRoi={}&isWhitelisted={}",
        auto_filter.limit.unwrap_or(MINTS_LIMIT_DEFAULT_VALUE),
        auto_filter.min_tx_len.unwrap_or(MIN_TX_LEN_DEFAULT_VALUE),
        auto_filter.min_pool_wsol_liquidity.unwrap_or(MIN_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE),
        auto_filter.max_pool_wsol_liquidity.unwrap_or(MAX_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE),
        auto_filter.duration.unwrap_or(DURATION_DEFAULT_VALUE),
        auto_filter.min_profit.unwrap_or(MIN_PROFIT_DEFAULT_VALUE),
        auto_filter.min_profit_per_arb.unwrap_or(MIN_PROFIT_PER_ARB_DEFAULT_VALUE),
        auto_filter.min_roi.unwrap_or(MIN_ROI_DEFAULT_VALUE),
        auto_filter.ignore_offchain_bots.unwrap_or(WHITELIST_ONLY_DEFAULT_VALUE)
    );
    let url = format!("http://194.164.217.147:3003/recent-tokens?{}", query);
    // info!("{}", url);
    let res = client.get(url).header("Content-Type", "application/json").send().await;
    let mut market_group_info: Vec<MarketsGroupInfo> = vec![];
    if let Ok(response) = res {
        if let Ok(data) = response.json::<serde_json::Value>().await {
            let result: TokenResult = serde_json::from_value(data)?;
            // tracing::info!("fetched hot_tokens result: {:?}", result);
            
            let json_str = serde_json::to_string_pretty(&result)?;
            fs::write("routing.json", &json_str)?;
            if result.count > 0 {
                let is_log_enabled = config.bot.log.unwrap_or(false);
                if is_log_enabled {
                    let now = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
                    let mut file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("routing_log.txt").await?;
                    let line = format!("[{}] {}\n", now, format!("{:#?}", result));
                    file.write_all(line.as_bytes()).await?;
                }
            }
            // info!("✅ Written routing.json");
            for arb_mint_info in result.arb_mint_info {
                let markets = arb_mint_info.pool_ids.clone();
                let luts = arb_mint_info.lookup_table_accounts.clone();
                market_group_info.push(MarketsGroupInfo { markets, luts: Some(luts) });
            }
            info!("Fetched {} mint list.", result.count);
            Ok(market_group_info)
        } else {
            anyhow::bail!("Error while fetching token info.")
        }
    } else {
        anyhow::bail!("Arb-Assist Server is not working.")
    }
}

pub async fn generate_token_list(config_path: &str) -> anyhow::Result<()> {
    let config = Config::load(config_path)?;
    info!("Configuration loaded successfully");
    let _routing_config = get_auto_mint_info_result(&config).await?;
    Ok(())
}

pub async fn find_all_lookup_tables(config_path: &str) -> anyhow::Result<()> {
    let config = Config::load(config_path)?;
    info!("Configuration loaded successfully");
    let rpc = Arc::new(RpcClient::new(&config.rpc.url));
    let payer = load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    let accounts = find_luts(rpc, &payer.pubkey())?;
    info!("Total LUTS len: {}", accounts.len());
    for (index, (pubkey, account)) in accounts.into_iter().enumerate() {
        match AddressLookupTable::deserialize(&account.data) {
            Ok(lookup_table) => {
                let lookup_table_account = AddressLookupTableAccount {
                    key: pubkey.clone(),
                    addresses: lookup_table.addresses.into_owned(),
                };
                info!(
                    "LUT account #{} ({}) -> address_len: {}",
                    index + 1,
                    pubkey,
                    lookup_table_account.addresses.len()
                );
            }
            Err(e) => {
                error!("   Failed to deserialize lookup table {}: {}", pubkey, e);
                continue; // Skip this lookup table but continue processing others
            }
        }
    }

    Ok(())
}

pub async fn create_new_lookup_table(config_path: &str) -> anyhow::Result<()> {
    let config = Config::load(config_path)?;
    info!("Configuration loaded successfully");
    let rpc = Arc::new(RpcClient::new(&config.rpc.url));
    let payer = load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    let lut_pubkey = create_lut(&payer, rpc)?;
    info!("New Lookup Table created: {}", lut_pubkey);
    Ok(())
}

pub async fn update_vault_auth_info(claimer: &str, config_path: &str) -> anyhow::Result<()> {
    let config = Config::load(config_path)?;
    info!("Configuration loaded successfully");
    let rpc_client = RpcClient::new(&config.rpc.url);
    let payer = load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    let payer_pubkey = payer.pubkey();
    let claimer_pubkey = Pubkey::from_str(claimer)?;
    let accounts = vec![
        AccountMeta::new(payer_pubkey.clone(), true),
        AccountMeta::new(VAULT_AUTH, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(claimer_pubkey, false)
    ];
    let data: [u8; 1] = [10];
    let ix = Instruction::new_with_bytes(BOT_PROGRAM_ID, &data, accounts);
    let ixs: Vec<Instruction> = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(10_000),
        ComputeBudgetInstruction::set_compute_unit_price(100_000),
        ix.clone()
    ];
    let blockhash = rpc_client.get_latest_blockhash()?;
    let tx = Transaction::new_signed_with_payer(&ixs, Some(&payer_pubkey), &[&payer], blockhash);
    let simul = rpc_client.simulate_transaction(&tx)?;

    if simul.value.err.is_some() {
        info!("{:#?}", simul.value.logs);
    } else {
        let sig = rpc_client.send_and_confirm_transaction(&tx)?;
        info!("https://solscan.io/tx/{}", sig);
    }
    Ok(())
}

pub async fn claim_fees(config_path: &str) -> anyhow::Result<()> {
    let config = Config::load(config_path)?;
    info!("Configuration loaded successfully");
    let rpc_client = RpcClient::new(&config.rpc.url);
    let payer = load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    let payer_pubkey = payer.pubkey();
    let vault_auth_wsol_ata = get_associated_token_address(&VAULT_AUTH, &native_mint::id());
    let vault_auth_data = rpc_client.get_account_data(&VAULT_AUTH)?;
    let fee_to_claim = u64::from_le_bytes(vault_auth_data[0..8].try_into().unwrap());
    let admin1 = Pubkey::try_from(&vault_auth_data[8..40]).unwrap();
    let admin2 = Pubkey::try_from(&vault_auth_data[40..72]).unwrap();
    info!("fee: {}, admin1: {}, admin2: {}", fee_to_claim, admin1, admin2);
    let admin1_wsol_ata = get_associated_token_address(&admin1, &native_mint::id());
    let admin2_wsol_ata = get_associated_token_address(&admin2, &native_mint::id());
    let accounts = vec![
        AccountMeta::new(payer_pubkey.clone(), true),
        AccountMeta::new(VAULT_AUTH, false),
        AccountMeta::new(vault_auth_wsol_ata, false),
        AccountMeta::new(admin1_wsol_ata, false),
        AccountMeta::new(admin2_wsol_ata, false),
        AccountMeta::new_readonly(spl_token::ID, false)
    ];
    let data: [u8; 1] = [11];
    let ix = Instruction::new_with_bytes(BOT_PROGRAM_ID, &data, accounts);
    let ixs: Vec<Instruction> = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(150_000),
        ComputeBudgetInstruction::set_compute_unit_price(100_000),
        create_associated_token_account_idempotent(
            &payer_pubkey,
            &admin1,
            &native_mint::ID,
            &spl_token::ID
        ),
        create_associated_token_account_idempotent(
            &payer_pubkey,
            &admin2,
            &native_mint::ID,
            &spl_token::ID
        ),
        ix.clone()
    ];
    let blockhash = rpc_client.get_latest_blockhash()?;
    let tx = Transaction::new_signed_with_payer(&ixs, Some(&payer_pubkey), &[&payer], blockhash);
    let simul = rpc_client.simulate_transaction(&tx)?;
    if simul.value.err.is_some() {
        info!("{:#?}", simul.value.logs);
        info!("{:#?}", simul.value.err);
    } else {
        let sig = rpc_client.send_and_confirm_transaction(&tx)?;
        info!("https://solscan.io/tx/{}", sig);
    }
    // if simul.value.err.is_none() {
    //     let sig = rpc_client.send_and_confirm_transaction(&tx)?;
    //     info!("https://solscan.io/tx/{}", sig);
    // }
    Ok(())
}

pub async fn create_markets(config_path: &str) -> anyhow::Result<()> {
    let config = Arc::new(Config::load(config_path)?);
    info!("Configuration loaded successfully");
    let rpc_client = Arc::new(RpcClient::new(&config.rpc.url));
    let payer = load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    let payer_pubkey = payer.pubkey();
    let tokens_info = TokensInfo::load("tokens.toml");
    if let Ok(tokens_info) = tokens_info {
        let tokens = tokens_info.tokens;
        create_markets_file_from_tokens(tokens, &payer_pubkey, &rpc_client, &config).await;
    } else {
        error!("failed to read tokens.toml file.");
    }
    Ok(())
}

pub async fn close_all_empty_atas(config_path: &str) -> anyhow::Result<()> {
    let config = Config::load(config_path)?;
    info!("Configuration loaded successfully");
    let rpc = Arc::new(RpcClient::new(&config.rpc.url));
    let payer = load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
    let filter = TokenAccountsFilter::ProgramId(spl_token::id());
    let accounts = rpc.get_token_accounts_by_owner(&payer.pubkey(), filter)?;
    info!("Token program accounts:");
    info!("all_atas_len: {}", accounts.len());
    #[derive(Deserialize)]
    pub struct TokenAmount {
        pub amount: String,
    }
    #[derive(Deserialize)]
    pub struct InfoObject {
        pub info: ParsedAccount,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ExtensionObject {
        pub extension: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ParsedAccount {
        pub token_amount: TokenAmount,
        pub extensions: Option<Vec<ExtensionObject>>,
    }
    let zero_balance_accounts: Vec<Pubkey> = accounts
        .into_iter()
        .filter_map(|RpcKeyedAccount { pubkey, account }| {
            // info!("{:#?}", account.data);
            match account.data {
                UiAccountData::Json(parsed_account) => {
                    let data: InfoObject = serde_json::from_value(parsed_account.parsed).unwrap();
                    // info!("{}", data.info.token_amount.amount);
                    if data.info.token_amount.amount.parse::<u64>().unwrap() == 0_u64 {
                        return Some(pubkey.parse().unwrap());
                    }
                }
                _ => {
                    return None;
                }
            }
            None
        })
        .collect();

    info!("Zero balance ATAs: {:?}", zero_balance_accounts.len());
    zero_balance_accounts.chunks(20).for_each(|accounts| {
        if accounts.len() > 0 {
            let close_ixs: Vec<Instruction> = accounts
                .into_iter()
                .map(|ata| {
                    close_account(
                        &spl_token::id(),
                        &ata,
                        &payer.pubkey(),
                        &payer.pubkey(),
                        &[]
                    ).unwrap()
                })
                .collect();
            let compute_budget_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(70_000);
            let compute_budget_price_ix = ComputeBudgetInstruction::set_compute_unit_price(100000);
            let mut ixs = vec![compute_budget_limit_ix, compute_budget_price_ix];
            ixs.extend_from_slice(&close_ixs);
            let tx = Transaction::new_signed_with_payer(
                &ixs,
                Some(&payer.pubkey()),
                &[&payer],
                rpc.get_latest_blockhash().unwrap()
            );
            let sig = rpc.send_and_confirm_transaction(&tx).unwrap();
            info!("https://solscan.io/tx/{}", sig)
        }
    });
    let token_2022_program_id = Pubkey::from_str(
        "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
    ).unwrap();
    let filter = TokenAccountsFilter::ProgramId(token_2022_program_id.clone());
    let accounts = rpc.get_token_accounts_by_owner(&payer.pubkey(), filter)?;
    info!("Token 2022 program accounts:");
    info!("all_atas_len: {}", accounts.len());

    let zero_balance_accounts: Vec<Pubkey> = accounts
        .into_iter()
        .filter_map(|RpcKeyedAccount { pubkey, account }| {
            // info!("{:#?}", account.data);
            match account.data {
                UiAccountData::Json(parsed_account) => {
                    // info!("{:#?}", parsed_account);
                    let data: InfoObject = serde_json::from_value(parsed_account.parsed).unwrap();
                    // info!("{}", data.info.token_amount.amount);
                    if
                        data.info.token_amount.amount.parse::<u64>().unwrap() == 0_u64 &&
                        data.info.extensions.unwrap().len() == 1
                    {
                        return Some(pubkey.parse().unwrap());
                    }
                }
                _ => {
                    return None;
                }
            }
            None
        })
        .collect();

    info!("Zero balance ATAs: {:?}", zero_balance_accounts.len());
    zero_balance_accounts.chunks(20).for_each(|accounts| {
        if accounts.len() > 0 {
            let close_ixs: Vec<Instruction> = accounts
                .into_iter()
                .map(|ata| {
                    spl_token_2022::instruction
                        ::close_account(
                            &token_2022_program_id,
                            &ata,
                            &payer.pubkey(),
                            &payer.pubkey(),
                            &[]
                        )
                        .unwrap()
                })
                .collect();
            let compute_budget_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(70_000);
            let compute_budget_price_ix = ComputeBudgetInstruction::set_compute_unit_price(100000);
            let mut ixs = vec![compute_budget_limit_ix, compute_budget_price_ix];
            ixs.extend_from_slice(&close_ixs);
            let tx = Transaction::new_signed_with_payer(
                &ixs,
                Some(&payer.pubkey()),
                &[&payer],
                rpc.get_latest_blockhash().unwrap()
            );
            let sig = rpc.send_and_confirm_transaction(&tx).unwrap();
            info!("https://solscan.io/tx/{}", sig)
        }
    });
    Ok(())
}

async fn blockhash_refresher(
    rpc_client: Arc<RpcClient>,
    cached_blockhash: Arc<Mutex<GlobalState>>,
    refresh_interval: Duration
) {
    loop {
        match rpc_client.get_latest_blockhash() {
            Ok(blockhash) => {
                let mut guard = cached_blockhash.lock().await;
                (*guard).blockhash = blockhash;
            }
            Err(e) => {
                error!("Failed to refresh blockhash: {:?}", e);
            }
        }
        tokio::time::sleep(refresh_interval).await;
    }
}

pub fn load_keypair(private_key: &str) -> anyhow::Result<Keypair> {
    if
        let Ok(keypair) = bs58
            ::decode(private_key)
            .into_vec()
            .map_err(|e| anyhow::anyhow!("Failed to decode base58: {}", e))
            .and_then(|bytes| {
                Keypair::try_from(&bytes[..]).map_err(|e|
                    anyhow::anyhow!("Invalid keypair bytes: {}", e)
                )
            })
    {
        return Ok(keypair);
    }

    if let Ok(keypair) = solana_sdk::signature::read_keypair_file(private_key) {
        return Ok(keypair);
    }

    anyhow::bail!("Failed to load keypair from: {}", private_key)
}
