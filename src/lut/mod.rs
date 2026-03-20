use std::{ sync::Arc };

use anyhow::{ anyhow, Result };
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::{ RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcSendTransactionConfig },
    rpc_filter::MemcmpEncodedBytes,
};
use solana_sdk::{
    account::Account,
    address_lookup_table::{
        instruction::{ create_lookup_table, extend_lookup_table },
        state::AddressLookupTable,
    },
    compute_budget::ComputeBudgetInstruction,
    message::{ v0::Message, VersionedMessage },
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::VersionedTransaction,
};
use tracing::info;

use crate::constants::LUT_PROGRAM_ID;

pub fn find_luts(rpc: Arc<RpcClient>, auth_pubkey: &Pubkey) -> Result<Vec<(Pubkey, Account)>> {
    let filters = vec![
        solana_client::rpc_filter::RpcFilterType::Memcmp(
            solana_client::rpc_filter::Memcmp::new(
                22,
                MemcmpEncodedBytes::Base58(auth_pubkey.to_string())
            )
        )
    ];

    let config = RpcProgramAccountsConfig {
        filters: Some(filters),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            ..Default::default()
        },
        with_context: None,
        sort_results: None,
    };
    let result = rpc.get_program_accounts_with_config(&LUT_PROGRAM_ID, config);

    match result {
        Ok(accounts) => {
            return Ok(accounts);
            // println!("Found {} LUT(s) created by {}", accounts.len(), auth_pubkey);
            // for (pubkey, _account) in accounts {
            //     println!("LUT Address: {}", pubkey);
            // }
        }
        Err(e) => {
            return Err(anyhow!("Error fetching LUTs: {:?}", e));
        }
    }
}

pub fn create_lut(payer: &Keypair, rpc_client: Arc<RpcClient>) -> Result<Pubkey> {
    let recent_slot = rpc_client.get_slot()?;
    let (create_lut_ix, lut_pubkey) = create_lookup_table(
        payer.pubkey(),
        payer.pubkey(),
        recent_slot
    );
    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(20000),
        ComputeBudgetInstruction::set_compute_unit_price(1000000)
    ];
    instructions.push(create_lut_ix);
    let recent_blockhash = rpc_client.get_latest_blockhash()?;
    let msg = Message::try_compile(&payer.pubkey(), &instructions, &[], recent_blockhash)?;
    let tx = VersionedTransaction::try_new(VersionedMessage::V0(msg), &[payer])?;
    rpc_client.send_transaction_with_config(&tx, RpcSendTransactionConfig {
        skip_preflight: true,
        ..Default::default()
    })?;
    // rpc_client.send_and_confirm_transaction(&tx)?;
    Ok(lut_pubkey)
}

pub fn extend_lut(
    lut_pubkey: &Pubkey,
    addresses_to_add: &Vec<Pubkey>,
    payer: &Keypair,
    rpc_client: Arc<RpcClient>
) -> Result<()> {
    match rpc_client.get_account(lut_pubkey) {
        Ok(account) => {
            match AddressLookupTable::deserialize(&account.data) {
                Ok(lookup_table) => {
                    let current_lut_addresses_len = lookup_table.addresses.len();
                    let filtered_addresses_to_add: Vec<Pubkey> = addresses_to_add
                        .iter()
                        .filter(|&f| { !lookup_table.addresses.contains(f) })
                        .cloned()
                        .collect();
                    let filtered_addresses_to_add_len = filtered_addresses_to_add.len();
                    if current_lut_addresses_len + filtered_addresses_to_add_len > 256 {
                        return Err(anyhow!("Overflowing of addresses in LUT account"));
                    }
                    if filtered_addresses_to_add_len > 20 {
                        for new_addresses in filtered_addresses_to_add
                            .chunks(25)
                            .map(|c| c.to_vec()) {
                            let extend_lut_ix = extend_lookup_table(
                                *lut_pubkey,
                                payer.pubkey(),
                                Some(payer.pubkey()),
                                new_addresses
                            );
                            let mut instructions = vec![
                                ComputeBudgetInstruction::set_compute_unit_limit(20000),
                                ComputeBudgetInstruction::set_compute_unit_price(1000000)
                            ];
                            instructions.push(extend_lut_ix);
                            let recent_blockhash = rpc_client.get_latest_blockhash()?;
                            let msg = VersionedMessage::V0(
                                Message::try_compile(
                                    &payer.pubkey(),
                                    &instructions,
                                    &[],
                                    recent_blockhash
                                )?
                            );
                            let tx = VersionedTransaction::try_new(msg, &[payer])?;
                            rpc_client.send_and_confirm_transaction(&tx)?;
                        }
                        info!(
                            "Successfully extended lookup table {} added {} lut addresses",
                            lut_pubkey,
                            filtered_addresses_to_add_len
                        );
                    }
                }
                Err(e) => {
                    println!("   Failed to deserialize lookup table {}: {}", lut_pubkey, e);
                    return Err(anyhow!("Failed to deserialize lookup table"));
                }
            }
        }
        Err(e) => {
            println!("Error while fetching lut account data: {e}");
            return Err(anyhow!("Error while fetching lut account data"));
        }
    }

    Ok(())
}
