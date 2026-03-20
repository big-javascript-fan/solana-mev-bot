use solana_sdk::pubkey::Pubkey;
use solana_sdk::pubkey;
use std::str::FromStr;

pub const SOL_MINT: &str = "So11111111111111111111111111111111111111112";
pub const WSOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");

pub fn sol_mint() -> Pubkey {
    Pubkey::from_str(SOL_MINT).unwrap()
}

pub const BOT_PROGRAM_ID: Pubkey = pubkey!("4Qv3mbzcq1bKmrhGG4voS3EemfPd7f838FLUU7wBHSyi");
pub const FEE_COLLECTOR: Pubkey = pubkey!("7Qk9jBNNwpwg2oH2yRLq8CDqEiT11Evod29dQZgsnYzA");
pub const VAULT_AUTH: Pubkey = pubkey!("J4pc3HHq7r3TrcTTHMXeyo14vKvHU7Xgr4PBVTQY6mpM");
pub const VAULT_AUTH_WSOL_ATA: Pubkey = pubkey!("FYWUytY7r9XiUYQ9Le6mRmQzRJuAjV6ff9zphKf1rcVa");
pub const LUT_PROGRAM_ID: Pubkey = pubkey!("AddressLookupTab1e1111111111111111111111111");
pub const MEMO_PROGRAM_V2_ID: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
pub const SYSVAR_INSTRUCTION_ID: Pubkey = pubkey!("Sysvar1nstructions1111111111111111111111111");
pub const ASSOCIATED_TOKEN_PROGRAM_ID: Pubkey = pubkey!(
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
);

pub const DLMM_PROGRAM_ID: Pubkey = pubkey!("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");
pub const PUMP_AMM_PROGRAM_ID: Pubkey = pubkey!("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
pub const DAMMV2_PROGRAM_ID: Pubkey = pubkey!("cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG");
pub const CPMM_PROGRAM_ID: Pubkey = pubkey!("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");
pub const AMM_PROGRAM_ID: Pubkey = pubkey!("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");

pub const MINTS_LIMIT_DEFAULT_VALUE: u64 = 2;
pub const MIN_TX_LEN_DEFAULT_VALUE: u64 = 20;
pub const MIN_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE: u64 = 6;
pub const MAX_POOL_WSOL_LIQUIDITY_DEFAULT_VALUE: u64 = 780000;
pub const DURATION_DEFAULT_VALUE: u64 = 60;
pub const MIN_PROFIT_DEFAULT_VALUE: u64 = 200_000_000;
pub const MIN_PROFIT_PER_ARB_DEFAULT_VALUE: u64 = 0;
pub const MIN_ROI_DEFAULT_VALUE: f64 = 0.0;
pub const WHITELIST_ONLY_DEFAULT_VALUE: bool = false;

pub static JITO_TIP_ACCOUNTS: &[Pubkey] = &[
    pubkey!("96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5"),
    pubkey!("DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh"),
    pubkey!("3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT"),
    pubkey!("ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49"),
    pubkey!("ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt"),
    pubkey!("Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY"),
    pubkey!("HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe"),
    pubkey!("DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL"),
];

// pub static SENDER_TIP_ACCOUNTS: &[Pubkey] = &[
//     pubkey!("4ACfpUFoaSD9bfPdeu6DBt89gB6ENTeHBXCAi87NhDEE"),
//     pubkey!("D2L6yPZ2FmmmTKPgzaMKdhu6EWZcTpLy1Vhx8uvZe7NZ"),
//     pubkey!("9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta"),
//     pubkey!("5VY91ws6B2hMmBFRsXkoAAdsPHBJwRfBht4DXox3xkwn"),
//     pubkey!("2nyhqdwKcJZR2vcqCyrYsaPVdAnFoJjiksCXJ7hfEYgD"),
//     pubkey!("2q5pghRs6arqVjRvT5gfgWfWcHWmw1ZuCzphgd5KfWGJ"),
//     pubkey!("wyvPkWjVZz1M8fHQnMMCDTQDbkManefNNhweYk5WkcF"),
//     pubkey!("3KCKozbAaF75qEU33jtzozcJ29yJuaLJTy2jFdzUY8bT"),
//     pubkey!("4vieeGHPYPG2MmyPRcYjdiDmmhN3ww7hsFNap8pVN3Ey"),
//     pubkey!("4TQLFNWK8AovT1gFvda5jfw2oJeRMKEmw7aH6MGBJ3or"),
// ];
