#![allow(deprecated)]
use crate::bot::{ MintInfo, SwapInstructionInfo, load_keypair };
use crate::constants::{
    AMM_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID,
    BOT_PROGRAM_ID,
    CPMM_PROGRAM_ID,
    DAMMV2_PROGRAM_ID,
    DLMM_PROGRAM_ID,
    FEE_COLLECTOR,
    MEMO_PROGRAM_V2_ID,
    PUMP_AMM_PROGRAM_ID,
    SYSVAR_INSTRUCTION_ID,
    VAULT_AUTH,
    VAULT_AUTH_WSOL_ATA,
    WSOL_MINT,
    sol_mint,
};
use crate::dex::meteora::constants::{
    damm_v2_event_authority,
    damm_v2_pool_authority,
    damm_v2_program_id,
    dlmm_event_authority,
};
use crate::dex::meteora::dammv2_info::MeteoraDAmmV2Info;
use crate::dex::meteora::{ constants::dlmm_program_id, dlmm_info::DlmmInfo };
use crate::dex::pump::{
    PUMP_AUTHORITY,
    PUMP_FEE_CONFIG,
    PUMP_FEE_PROGRAM,
    PUMP_GLOBAL_CONFIG,
    PUMP_GLOBAL_VOLUME_ACCUMULATOR,
    PumpAmmInfo,
    pump_fee_wallet,
    pump_mayhem_fee_wallet,
    pump_program_id,
};
use crate::dex::raydium::{
    RaydiumAmmInfo,
    RaydiumCpAmmInfo,
    raydium_authority,
    raydium_cp_authority,
    raydium_cp_program_id,
    raydium_program_id,
};

use bincode::Options;
use solana_client::rpc_client::RpcClient;
use solana_sdk::address_lookup_table::state::AddressLookupTable;
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use solana_sdk::hash::Hash;
use solana_sdk::message::AddressLookupTableAccount;
use solana_sdk::message::v0::Message;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::instruction::{ AccountMeta, Instruction };
use solana_sdk::transaction::{ Transaction, VersionedTransaction };
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use spl_associated_token_account::{
    self,
    get_associated_token_address,
    get_associated_token_address_with_program_id,
};
use tokio::time::sleep;
use std::collections::HashMap;
use std::sync::{ Mutex, OnceLock };
use std::str::FromStr;
use std::sync::Arc;
use std::time::{ Duration, Instant };
use tracing::{ error, info, warn };
use crate::config::{ Config, MarketsGroupInfo };
use anyhow::Context;
use solana_sdk::account::Account;

static POOL_ACCOUNT_CACHE: OnceLock<Mutex<HashMap<Pubkey, (Instant, Account)>>> = OnceLock::new();

fn get_pool_account_cached(
    rpc_client: &Arc<RpcClient>,
    pool_id: &Pubkey,
    ttl: Duration
) -> anyhow::Result<Account> {
    let cache = POOL_ACCOUNT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((stored_at, account)) = guard.get(pool_id) {
            if stored_at.elapsed() <= ttl {
                return Ok(account.clone());
            }
        }
    }

    let account = rpc_client.get_account(pool_id)?;
    if let Ok(mut guard) = cache.lock() {
        guard.insert(*pool_id, (Instant::now(), account.clone()));
    }
    Ok(account)
}

pub fn versioned_tx_size(tx: &VersionedTransaction) -> usize {
    // Use little endian + fixint encoding (same as Solana)
    let serialized = bincode::DefaultOptions
        ::new()
        .with_fixint_encoding()
        .with_little_endian()
        .serialize(tx)
        .expect("failed to serialize VersionedTransaction");
    serialized.len()
}

pub fn is_tx_under_mtu(tx: &VersionedTransaction) -> bool {
    versioned_tx_size(tx) <= 1232
}

pub async fn get_swap_ix(
    payer_pubkey: &Pubkey,
    markets_info: &Vec<MarketsGroupInfo>,
    hot_markets_info: Option<Vec<MarketsGroupInfo>>,
    rpc_client: &Arc<RpcClient>,
    config: &Arc<Config>
) -> anyhow::Result<(SwapInstructionInfo, Vec<String>, Vec<String>)> {
    let mut markets: Vec<String> = vec![];
    let mut luts: Vec<String> = vec!["9DFusApoYMoBiysEGbsys5h9QMYU9sN4kFpY9LE4UXye".to_string()];
    let mut markets_info = markets_info.clone();
    if markets_info.len() == 1 && hot_markets_info.is_some() {
        let hot_markets = hot_markets_info.unwrap();
        let first_market = markets_info[0].clone();
        for hot_market in hot_markets.iter() {
            if first_market.markets[0] != hot_market.markets[0] {
                markets_info.push(hot_market.clone());
                break;
            }
        }
    }
    markets_info.iter().for_each(|m| {
        let prev_markets_len = markets.len();
        if prev_markets_len + m.markets.len() > 5 {
            while markets.len() > 3 {
                markets.pop();
            }
            let second_markets_len_to_add = 5 - markets.len();
            let filtered_second_markets: Vec<String> = m.markets
                .iter()
                .take(second_markets_len_to_add)
                .cloned()
                .collect();
            markets.extend_from_slice(&filtered_second_markets);
        } else {
            markets.extend_from_slice(&m.markets);
        }
        if m.luts.is_some() {
            let filtered_luts: Vec<String> = m.luts
                .clone()
                .unwrap()
                .iter()
                .take(8)
                .cloned()
                .collect();
            luts.extend_from_slice(&filtered_luts);
        }
    });
    if markets.len() >= 2 {
        let pool_cache_ttl = Duration::from_millis(config.bot.pool_cache_ttl_ms.unwrap_or(3000));
        let payer_wsol_ata_pubkey = get_associated_token_address(payer_pubkey, &WSOL_MINT);
        let mut accounts = vec![
            AccountMeta::new_readonly(payer_pubkey.clone(), true), // 0. Wallet (signer)
            AccountMeta::new_readonly(WSOL_MINT, false), // 1. SOL mint
            AccountMeta::new(FEE_COLLECTOR, false), // 2. Fee collector
            AccountMeta::new(payer_wsol_ata_pubkey, false), // 3. Wallet SOL account
            AccountMeta::new_readonly(spl_token::ID, false), // 4. Token program
            AccountMeta::new_readonly(solana_system_interface::program::ID, false), // 5. System program
            AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM_ID, false), // 6. Associated Token program
            AccountMeta::new(VAULT_AUTH, false), // 7: vault_auth
            AccountMeta::new(VAULT_AUTH_WSOL_ATA, false), // 8: vault_auth_wsol_ata
            AccountMeta::new_readonly(MEMO_PROGRAM_V2_ID, false) // 9: memo V2
        ];
        let mut mint_pubkey: Pubkey = Pubkey::default();
        let mut total_pools_len = 0;
        let mut mints_info = vec![];
        for market in markets.iter() {
            let pool_id = Pubkey::from_str(market);
            let pool_id = match pool_id {
                Ok(pool_id) => pool_id,
                Err(e) => { anyhow::bail!("{:?}", e) }
            };
            let pool_info = get_pool_account_cached(rpc_client, &pool_id, pool_cache_ttl);
            let pool_info = match pool_info {
                Ok(pool_info) => { pool_info }
                Err(e) => { anyhow::bail!("{:?}", e) }
            };
            match pool_info.owner {
                PUMP_AMM_PROGRAM_ID => {
                    match PumpAmmInfo::load_checked(&pool_info.data) {
                        Ok(amm_info) => {
                            let (sol_vault, token_vault, mint) = if
                                sol_mint() == amm_info.base_mint
                            {
                                (
                                    amm_info.pool_base_token_account,
                                    amm_info.pool_quote_token_account,
                                    amm_info.quote_mint,
                                )
                            } else if sol_mint() == amm_info.quote_mint {
                                (
                                    amm_info.pool_quote_token_account,
                                    amm_info.pool_base_token_account,
                                    amm_info.base_mint,
                                )
                            } else {
                                error!("Not WSOL paired pump_amm pool. {}", pool_id);
                                continue;
                            };
                            if mint_pubkey == Pubkey::default() || mint_pubkey != mint {
                                mint_pubkey = mint;
                                let mint_owner = if
                                    let Ok(mint_info) = rpc_client.get_account(&mint_pubkey)
                                {
                                    mint_info.owner
                                } else {
                                    spl_token::ID
                                };
                                mints_info.push(MintInfo { mint, mint_owner });
                                accounts.push(
                                    AccountMeta::new_readonly(mint_pubkey.clone(), false)
                                );
                                let mint_ata = get_associated_token_address_with_program_id(
                                    &payer_pubkey,
                                    &mint_pubkey,
                                    &mint_owner
                                );
                                accounts.push(AccountMeta::new_readonly(mint_owner, false));
                                accounts.push(AccountMeta::new(mint_ata, false));
                            }
                            let (fee_wallet, fee_token_wallet) = if amm_info.is_mayhem_mode {
                                let wallet = pump_mayhem_fee_wallet();
                                (
                                    wallet,
                                    spl_associated_token_account::get_associated_token_address(
                                        &wallet,
                                        &amm_info.quote_mint
                                    ),
                                )
                            } else {
                                let wallet = pump_fee_wallet();
                                (
                                    wallet,
                                    spl_associated_token_account::get_associated_token_address(
                                        &wallet,
                                        &amm_info.quote_mint
                                    ),
                                )
                            };

                            let coin_creator_vault_ata =
                                spl_associated_token_account::get_associated_token_address(
                                    &amm_info.coin_creator_vault_authority,
                                    &amm_info.quote_mint
                                );
                            accounts.push(AccountMeta::new_readonly(pump_program_id(), false));
                            accounts.push(AccountMeta::new_readonly(PUMP_GLOBAL_CONFIG, false));
                            accounts.push(AccountMeta::new_readonly(PUMP_AUTHORITY, false));
                            accounts.push(AccountMeta::new_readonly(fee_wallet, false));
                            accounts.push(AccountMeta::new(pool_id, false));
                            accounts.push(AccountMeta::new(token_vault, false));
                            accounts.push(AccountMeta::new(sol_vault, false));
                            accounts.push(AccountMeta::new(fee_token_wallet, false));
                            accounts.push(AccountMeta::new(coin_creator_vault_ata, false));
                            accounts.push(
                                AccountMeta::new_readonly(
                                    amm_info.coin_creator_vault_authority,
                                    false
                                )
                            );
                            let (user_volume_accumulator, _) = Pubkey::find_program_address(
                                &[b"user_volume_accumulator", payer_pubkey.as_ref()],
                                &pump_program_id()
                            );
                            accounts.push(AccountMeta::new(PUMP_GLOBAL_VOLUME_ACCUMULATOR, false));
                            accounts.push(AccountMeta::new(user_volume_accumulator, false));
                            accounts.push(AccountMeta::new_readonly(PUMP_FEE_CONFIG, false));
                            accounts.push(AccountMeta::new_readonly(PUMP_FEE_PROGRAM, false));
                            total_pools_len += 1;
                        }
                        Err(e) => {
                            error!("Error parsing AmmInfo from Pump pool {}: {:?}", pool_id, e);
                            return Err(e);
                        }
                    }
                }
                DLMM_PROGRAM_ID => {
                    match DlmmInfo::load_checked(&pool_info.data) {
                        Ok(amm_info) => {
                            let (sol_vault, token_vault, mint) = if
                                sol_mint() == amm_info.token_x_mint
                            {
                                (
                                    amm_info.token_x_vault,
                                    amm_info.token_y_vault,
                                    amm_info.token_y_mint,
                                )
                            } else if sol_mint() == amm_info.token_y_mint {
                                (
                                    amm_info.token_y_vault,
                                    amm_info.token_x_vault,
                                    amm_info.token_x_mint,
                                )
                            } else {
                                error!("Not WSOL paired dlmm pool. {}", pool_id);
                                continue;
                            };
                            if mint_pubkey == Pubkey::default() || mint_pubkey != mint {
                                mint_pubkey = mint;
                                let mint_owner = if
                                    let Ok(mint_info) = rpc_client.get_account(&mint_pubkey)
                                {
                                    mint_info.owner
                                } else {
                                    spl_token::ID
                                };
                                mints_info.push(MintInfo { mint, mint_owner });
                                accounts.push(
                                    AccountMeta::new_readonly(mint_pubkey.clone(), false)
                                );
                                let mint_ata = get_associated_token_address_with_program_id(
                                    &payer_pubkey,
                                    &mint_pubkey,
                                    &mint_owner
                                );
                                accounts.push(AccountMeta::new_readonly(mint_owner, false));
                                accounts.push(AccountMeta::new(mint_ata, false));
                            }

                            let bin_arrays = match amm_info.calculate_bin_arrays(&pool_id) {
                                Ok(arrays) => arrays,
                                Err(e) => {
                                    error!(
                                        "Error calculating bin arrays for DLMM pool {}: {:?}",
                                        pool_id,
                                        e
                                    );
                                    return Err(e);
                                }
                            };
                            accounts.push(AccountMeta::new_readonly(dlmm_program_id(), false));
                            accounts.push(AccountMeta::new(dlmm_event_authority(), false)); // DLMM event authority
                            accounts.push(AccountMeta::new(pool_id.clone(), false));
                            accounts.push(AccountMeta::new(token_vault, false));
                            accounts.push(AccountMeta::new(sol_vault, false));
                            accounts.push(AccountMeta::new(amm_info.oracle, false));
                            for bin_array in &bin_arrays {
                                accounts.push(AccountMeta::new(*bin_array, false));
                            }
                            total_pools_len += 1;
                        }
                        Err(e) => {
                            error!("Error parsing AmmInfo from DLMM pool {}: {:?}", pool_id, e);
                            return Err(e);
                        }
                    }
                }
                DAMMV2_PROGRAM_ID => {
                    match MeteoraDAmmV2Info::load_checked(&pool_info.data) {
                        Ok(amm_info) => {
                            let (sol_vault, token_vault, mint) = if
                                sol_mint() == amm_info.base_mint
                            {
                                (amm_info.base_vault, amm_info.quote_vault, amm_info.quote_mint)
                            } else if sol_mint() == amm_info.quote_mint {
                                (amm_info.quote_vault, amm_info.base_vault, amm_info.base_mint)
                            } else {
                                error!("Not WSOL paired dammV2 pool. {}", pool_id);
                                continue;
                            };
                            if mint_pubkey == Pubkey::default() || mint_pubkey != mint {
                                mint_pubkey = mint;
                                let mint_owner = if
                                    let Ok(mint_info) = rpc_client.get_account(&mint_pubkey)
                                {
                                    mint_info.owner
                                } else {
                                    spl_token::ID
                                };
                                mints_info.push(MintInfo { mint, mint_owner });
                                accounts.push(
                                    AccountMeta::new_readonly(mint_pubkey.clone(), false)
                                );
                                let mint_ata = get_associated_token_address_with_program_id(
                                    &payer_pubkey,
                                    &mint_pubkey,
                                    &mint_owner
                                );
                                accounts.push(AccountMeta::new_readonly(mint_owner, false));
                                accounts.push(AccountMeta::new(mint_ata, false));
                            }

                            accounts.push(AccountMeta::new_readonly(damm_v2_program_id(), false));
                            accounts.push(
                                AccountMeta::new_readonly(damm_v2_event_authority(), false)
                            );
                            accounts.push(
                                AccountMeta::new_readonly(damm_v2_pool_authority(), false)
                            );
                            accounts.push(AccountMeta::new(pool_id.clone(), false));
                            accounts.push(AccountMeta::new(token_vault, false));
                            accounts.push(AccountMeta::new(sol_vault, false));
                            accounts.push(AccountMeta::new_readonly(SYSVAR_INSTRUCTION_ID, false));
                            total_pools_len += 1;
                        }
                        Err(e) => {
                            error!(
                                "Error parsing Meteora DAMM V2 pool data from pool {}: {:?}",
                                pool_id,
                                e
                            );
                            continue;
                        }
                    }
                }
                CPMM_PROGRAM_ID => {
                    match RaydiumCpAmmInfo::load_checked(&pool_info.data) {
                        Ok(amm_info) => {
                            let (sol_vault, token_vault, mint) = if
                                sol_mint() == amm_info.token_0_mint
                            {
                                (
                                    amm_info.token_0_vault,
                                    amm_info.token_1_vault,
                                    amm_info.token_1_mint,
                                )
                            } else if sol_mint() == amm_info.token_1_mint {
                                (
                                    amm_info.token_1_vault,
                                    amm_info.token_0_vault,
                                    amm_info.token_0_mint,
                                )
                            } else {
                                error!("Not WSOL paired Cpmm pool. {}", pool_id);
                                continue;
                            };
                            if mint_pubkey == Pubkey::default() || mint_pubkey != mint {
                                mint_pubkey = mint;
                                let mint_owner = if
                                    let Ok(mint_info) = rpc_client.get_account(&mint_pubkey)
                                {
                                    mint_info.owner
                                } else {
                                    spl_token::ID
                                };
                                mints_info.push(MintInfo { mint, mint_owner });
                                accounts.push(
                                    AccountMeta::new_readonly(mint_pubkey.clone(), false)
                                );
                                let mint_ata = get_associated_token_address_with_program_id(
                                    &payer_pubkey,
                                    &mint_pubkey,
                                    &mint_owner
                                );
                                accounts.push(AccountMeta::new_readonly(mint_owner, false));
                                accounts.push(AccountMeta::new(mint_ata, false));
                            }

                            accounts.push(
                                AccountMeta::new_readonly(raydium_cp_program_id(), false)
                            );
                            accounts.push(AccountMeta::new_readonly(raydium_cp_authority(), false)); // Raydium CP authority
                            accounts.push(AccountMeta::new(pool_id.clone(), false));
                            accounts.push(AccountMeta::new_readonly(amm_info.amm_config, false));
                            accounts.push(AccountMeta::new(token_vault, false));
                            accounts.push(AccountMeta::new(sol_vault, false));
                            accounts.push(AccountMeta::new(amm_info.observation_key, false));
                            total_pools_len += 1;
                        }
                        Err(e) => {
                            error!(
                                "Error parsing AmmInfo from Raydium CP pool {}: {:?}",
                                pool_id,
                                e
                            );
                            return Err(e);
                        }
                    }
                }
                AMM_PROGRAM_ID => {
                    match RaydiumAmmInfo::load_checked(&pool_info.data) {
                        Ok(amm_info) => {
                            let (sol_vault, token_vault, mint) = if
                                sol_mint() == amm_info.coin_mint
                            {
                                (amm_info.coin_vault, amm_info.pc_vault, amm_info.pc_mint)
                            } else if sol_mint() == amm_info.pc_mint {
                                (amm_info.pc_vault, amm_info.coin_vault, amm_info.coin_mint)
                            } else {
                                error!("Not WSOL paired Amm pool. {}", pool_id);
                                continue;
                            };
                            if mint_pubkey == Pubkey::default() || mint_pubkey != mint {
                                mint_pubkey = mint;
                                let mint_owner = if
                                    let Ok(mint_info) = rpc_client.get_account(&mint_pubkey)
                                {
                                    mint_info.owner
                                } else {
                                    spl_token::ID
                                };
                                mints_info.push(MintInfo { mint, mint_owner });
                                accounts.push(
                                    AccountMeta::new_readonly(mint_pubkey.clone(), false)
                                );
                                let mint_ata = get_associated_token_address_with_program_id(
                                    &payer_pubkey,
                                    &mint_pubkey,
                                    &mint_owner
                                );
                                accounts.push(AccountMeta::new_readonly(mint_owner, false));
                                accounts.push(AccountMeta::new(mint_ata, false));
                            }
                            accounts.push(AccountMeta::new_readonly(raydium_program_id(), false));
                            accounts.push(AccountMeta::new_readonly(raydium_authority(), false)); // Raydium authority
                            accounts.push(AccountMeta::new(pool_id.clone(), false));
                            accounts.push(AccountMeta::new(token_vault, false));
                            accounts.push(AccountMeta::new(sol_vault, false));
                            total_pools_len += 1;
                        }
                        Err(e) => {
                            error!("Error parsing AmmInfo from Raydium pool {}: {:?}", pool_id, e);
                            return Err(e);
                        }
                    }
                }
                _ => {
                    warn!("Not supported Program owned Pool: {}", pool_id);
                    continue;
                }
            }
        }
        if total_pools_len < 2 {
            anyhow::bail!("Insufficient pools_len: {:#?}", markets);
        }
        let new_version = config.bot.new_version.unwrap_or_default();
        let no_failure_mode = config.bot.no_failure_mode;
        let flashloan_enabled = config.flashloan.enabled;
        let mut data = Vec::with_capacity(19);
        let start_in_amount: u64 = 10_000_000 + (rand::random::<u64>() % 10_000_000);
        data.push(if new_version { 17 } else { 16 });
        data.extend_from_slice(&start_in_amount.to_le_bytes());
        let min_profit: i64 = 0 as i64;
        data.extend_from_slice(&min_profit.to_le_bytes());
        data.extend_from_slice(if no_failure_mode { &[0] } else { &[1] });
        data.extend_from_slice(if flashloan_enabled { &[1] } else { &[0] });
        let swap_ix = Instruction {
            accounts,
            data,
            program_id: BOT_PROGRAM_ID,
        };
        let mut lut_accounts: Vec<AddressLookupTableAccount> = vec![];
        let current_slot = rpc_client.get_slot().unwrap_or_default();
        let mut is_simul_passed = false;
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
                        lut_accounts.push(lookup_table_account);
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
            sleep(Duration::from_millis(50)).await;
        }
        let mut ixs: Vec<Instruction> = vec![];
        mints_info.iter().for_each(|mint| {
            let create_ata_ix = create_associated_token_account_idempotent(
                &payer_pubkey,
                &payer_pubkey,
                &mint.mint,
                &mint.mint_owner
            );
            ixs.push(create_ata_ix);
        });
        ixs.push(swap_ix.clone());
        // let blockhash = rpc_client.get_latest_blockhash().unwrap();
        let max_len = 6;
        let mut proper_luts = vec![];
        let mut searched_loop_len = 0;
        let recent_blockhash: Hash;
        let mut retry_count = 0;
        loop {
            let blockhash_result = rpc_client.get_latest_blockhash();
            if blockhash_result.is_err() {
                retry_count += 1;
                if retry_count <= 3 {
                    error!("Failed to get latest_blockhash. Retrying... tried: {}", retry_count);
                    sleep(Duration::from_secs(1)).await;
                    continue;
                } else {
                    anyhow::bail!("Failed to get latest_blockhash.");
                }
            } else {
                recent_blockhash = blockhash_result.unwrap();
                break;
            }
        }

        info!("Finding proper luts info...");
        for mask in 1..1 << lut_accounts.len() {
            searched_loop_len += 1;
            // skip if subsequence length > max_len
            if ((mask as i32).count_ones() as usize) > max_len {
                continue;
            }
            let mut sub_luts = Vec::new();
            for i in 0..lut_accounts.len() {
                if (mask & (1 << i)) != 0 {
                    sub_luts.push(lut_accounts[i].clone());
                }
            }
            // check if luts works fine through simulation

            let message = Message::try_compile(
                &payer_pubkey,
                &ixs,
                &sub_luts,
                recent_blockhash
            ).unwrap();
            let payer = load_keypair(&config.wallet.private_key)
                .context("Failed to load wallet keypair")
                .unwrap();
            let tx = VersionedTransaction::try_new(
                solana_sdk::message::VersionedMessage::V0(message),
                &[payer]
            ).unwrap();
            if is_tx_under_mtu(&tx) {
                is_simul_passed = true;
                info!("Luts info Found!)");
                info!("searched_loop_count: {}", searched_loop_len);
                proper_luts.extend_from_slice(&sub_luts);
                break;
            }
            // let simul_result = rpc_client.simulate_transaction_with_config(
            //     &tx,
            //     RpcSimulateTransactionConfig {
            //         replace_recent_blockhash: true,
            //         ..Default::default()
            //     }
            // );
            // if simul_result.is_ok() {
            //     is_simul_passed = true;
            //     // info!("loop_len: {}", searched_loop_len);
            //     info!("passed tx_bytes: {}", versioned_tx_size(&tx));
            //     proper_luts.extend_from_slice(&sub_luts);
            //     break;
            // } else {
            //     warn!("failed to pass tx_bytes: {}", versioned_tx_size(&tx));
            // }
        }
        if !is_simul_passed {
            anyhow::bail!("Can't find proper luts for {}", mints_info[0].mint);
        }
        let mut ixs = vec![
            ComputeBudgetInstruction::set_compute_unit_limit(60000),
            ComputeBudgetInstruction::set_compute_unit_price(100000)
        ];
        let payer = load_keypair(&config.wallet.private_key)
            .context("Failed to load wallet keypair")
            .unwrap();
        mints_info.iter().for_each(|mint| {
            let mint_ata = get_associated_token_address_with_program_id(
                &payer_pubkey,
                &mint.mint,
                &mint.mint_owner
            );
            if rpc_client.get_account(&mint_ata).is_err() {
                info!("Creating new ata of mint: {}", &mint.mint);
                ixs.push(
                    create_associated_token_account_idempotent(
                        &payer_pubkey,
                        &payer_pubkey,
                        &mint.mint,
                        &mint.mint_owner
                    )
                )
            }
        });
        if ixs.len() > 2 {
            let mut retry_count: u8 = 0;
            loop {
                let mut tx = Transaction::new_with_payer(&ixs, Some(&payer_pubkey));
                tx.sign(&[&payer], recent_blockhash);
                if let Ok(sig) = rpc_client.send_and_confirm_transaction(&tx) {
                    info!("Created new ata. https://solscan.io/tx/{}", sig);
                    break;
                } else {
                    retry_count += 1;
                    if retry_count < 3 {
                        error!("Failed to create new ata. Retrying... tried: {}", retry_count);
                        sleep(Duration::from_secs(1)).await;
                    } else {
                        anyhow::bail!("Failed to create new ata.");
                    }
                }
            }
        }
        let new_luts: Vec<String> = proper_luts
            .iter()
            .map(|lut_acc| lut_acc.key.to_string())
            .collect();
        Ok((
            SwapInstructionInfo {
                mints_list: mints_info,
                ix: swap_ix,
                luts: proper_luts,
            },
            markets,
            new_luts,
        ))
    } else {
        anyhow::bail!("Insufficient markets_len: {}", markets.len())
    }
}
