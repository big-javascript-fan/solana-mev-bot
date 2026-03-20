use solana_sdk::pubkey::{ Pubkey };
use solana_sdk::pubkey;
use std::str::FromStr;

pub const PUMP_PROGRAM_ID: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";
pub const PUMP_FEE_WALLET: &str = "JCRGumoE9Qi5BBgULTgdgTLjSgkCMSbF62ZZfGs84JeU";
pub const PUMP_MAYHEM_FEE_WALLET: &str = "GesfTA3X2arioaHp8bbKdjG9vJtskViWACZoYvxp4twS";

pub const PUMP_GLOBAL_CONFIG: Pubkey = pubkey!("ADyA8hdefvWN2dbGGWFotbzWxrAvLW83WG6QCVXvJKqw");
pub const PUMP_AUTHORITY: Pubkey = pubkey!("GS4CU59F31iL7aR2Q8zVS8DRrcRnXX1yjQ66TqNVQnaR");
pub const PUMP_FEE_CONFIG: Pubkey = pubkey!("5PHirr8joyTMp9JMm6nW7hNDVyEYdkzDqazxPD7RaTjx");
pub const PUMP_FEE_PROGRAM: Pubkey = pubkey!("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
pub const PUMP_GLOBAL_VOLUME_ACCUMULATOR: Pubkey = pubkey!(
    "C2aFPdENg4A2HQsmrd5rTw5TaYBX5Ku887cWjbFKtZpw"
);

pub fn pump_program_id() -> Pubkey {
    Pubkey::from_str(PUMP_PROGRAM_ID).unwrap()
}

pub fn pump_fee_wallet() -> Pubkey {
    Pubkey::from_str(PUMP_FEE_WALLET).unwrap()
}

pub fn pump_mayhem_fee_wallet() -> Pubkey {
    Pubkey::from_str(PUMP_MAYHEM_FEE_WALLET).unwrap()
}
