use solana_sdk::pubkey::Pubkey;
use anyhow::Result;

const AMM_CONFIG_OFFSET: usize = 8; // amm_config
const TOKEN_0_VAULT_OFFSET: usize = 72; // token_0_vault
const TOKEN_1_VAULT_OFFSET: usize = 104; // token_1_vault
const TOKEN_0_MINT_OFFSET: usize = 168; // token_0_mint
const TOKEN_1_MINT_OFFSET: usize = 200; // token_1_mint
const OBSERVATION_KEY_OFFSET: usize = 296; // observation_key

#[derive(Debug)]
pub struct RaydiumCpAmmInfo {
    pub token_0_mint: Pubkey,
    pub token_1_mint: Pubkey,
    pub token_0_vault: Pubkey,
    pub token_1_vault: Pubkey,
    pub amm_config: Pubkey,
    pub observation_key: Pubkey,
}

impl RaydiumCpAmmInfo {
    pub fn load_checked(data: &[u8]) -> Result<Self> {
        if data.len() < OBSERVATION_KEY_OFFSET + 32 {
            return Err(anyhow::anyhow!("Invalid data length for RaydiumCpAmmInfo"));
        }

        let token_0_vault = Pubkey::try_from(
            &data[TOKEN_0_VAULT_OFFSET..TOKEN_0_VAULT_OFFSET + 32]
        )?;
        let token_1_vault = Pubkey::try_from(
            &data[TOKEN_1_VAULT_OFFSET..TOKEN_1_VAULT_OFFSET + 32]
        )?;
        let token_0_mint = Pubkey::try_from(&data[TOKEN_0_MINT_OFFSET..TOKEN_0_MINT_OFFSET + 32])?;
        let token_1_mint = Pubkey::try_from(&data[TOKEN_1_MINT_OFFSET..TOKEN_1_MINT_OFFSET + 32])?;
        let amm_config = Pubkey::try_from(&data[AMM_CONFIG_OFFSET..AMM_CONFIG_OFFSET + 32])?;
        let observation_key = Pubkey::try_from(
            &data[OBSERVATION_KEY_OFFSET..OBSERVATION_KEY_OFFSET + 32]
        )?;

        Ok(Self {
            token_0_mint,
            token_1_mint,
            token_0_vault,
            token_1_vault,
            amm_config,
            observation_key,
        })
    }
}
