use anyhow::Result;
use solana_sdk::pubkey::Pubkey;

use crate::dex::pump::pump_program_id;

#[derive(Debug)]
pub struct PumpAmmInfo {
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub pool_base_token_account: Pubkey,
    pub pool_quote_token_account: Pubkey,
    pub coin_creator_vault_authority: Pubkey,
    pub is_mayhem_mode: bool,
}

impl PumpAmmInfo {
    pub fn load_checked(data: &[u8]) -> Result<Self> {
        let data = &data[8..];
        let base_mint_offset = 1 + 2 + 32; // bump + index + creator
        let quote_mint_offset = base_mint_offset + 32;
        let pool_base_offset = quote_mint_offset + 32 + 32; // + lp mint
        let pool_quote_offset = pool_base_offset + 32;
        let min_len = pool_quote_offset + 32;

        if data.len() < min_len {
            return Err(anyhow::anyhow!("Invalid data length for PumpAmmInfo"));
        }

        let base_mint = Pubkey::try_from(&data[base_mint_offset..base_mint_offset + 32]).unwrap();
        let quote_mint = Pubkey::try_from(
            &data[quote_mint_offset..quote_mint_offset + 32]
        ).unwrap();
        let pool_base_token_account = Pubkey::try_from(
            &data[pool_base_offset..pool_base_offset + 32]
        ).unwrap();
        let pool_quote_token_account = Pubkey::try_from(
            &data[pool_quote_offset..pool_quote_offset + 32]
        ).unwrap();

        let coin_creator_offset = pool_quote_offset + 8 + 32; // lp_supply + last_trade_timestamp
        let is_mayhem_mode_offset = coin_creator_offset + 32;

        let coin_creator = if coin_creator_offset + 32 > data.len() {
            Pubkey::default()
        } else {
            Pubkey::try_from(&data[coin_creator_offset..coin_creator_offset + 32]).unwrap()
        };

        let is_mayhem_mode = if is_mayhem_mode_offset >= data.len() {
            false
        } else {
            data[is_mayhem_mode_offset] != 0
        };

        let coin_creator_vault_authority = if coin_creator == Pubkey::default() {
            Pubkey::default()
        } else {
            Pubkey::find_program_address(
                &[b"creator_vault", coin_creator.as_ref()],
                &pump_program_id()
            ).0
        };

        // println!("coin_creator: {:?}", key.0);

        Ok(Self {
            base_mint,
            quote_mint,
            pool_base_token_account,
            pool_quote_token_account,
            coin_creator_vault_authority,
            is_mayhem_mode,
        })
    }
}
