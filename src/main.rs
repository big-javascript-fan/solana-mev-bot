mod bot;
mod config;
mod constants;
mod dex;
mod refresh;
mod lut;
mod jito;
use clap::{ Parser, Subcommand };
use tracing::{ info, Level };
use tracing_subscriber::{ FmtSubscriber };

///Solana Arbitrage Onchain Bot
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct CliArgs {
    #[command(subcommand)]
    run: RunCommand,
}

#[derive(Subcommand, Debug)]
enum RunCommand {
    /// Run bot
    Run {
        ///Sets a custom config file
        #[arg(long, short, default_value = "config.toml")]
        config: String,
    },
    /// Wrap Sol
    Wrap {
        ///Sets a custom config file
        #[arg(long, short)]
        amount: f64,
        ///Sets a custom config file
        #[arg(long, short, default_value = "config.toml")]
        config: String,
    },
    /// Generate new routing.toml file which includes recent active arb-enabled token lists based on [auto] settings in your config.toml
    Token {
        ///Sets a custom config file
        #[arg(long, short, default_value = "config.toml")]
        config: String,
    },
    /// Find all lookup tables owned by the current wallet
    FindLookupTables {
        ///Sets a custom config file
        #[arg(long, short, default_value = "config.toml")]
        config: String,
    },
    /// Create a new lookup table owned by the current wallet
    CreateNewLookupTable {
        ///Sets a custom config file
        #[arg(long, short, default_value = "config.toml")]
        config: String,
    },
    /// Close all the empty atas
    CloseAllEmptyAtas {
        ///Sets a custom config file
        #[arg(long, short, default_value = "config.toml")]
        config: String,
    },
    /// Update fee_wallets
    UpdateVaultAuthInfo {
        ///Set second claimer address
        #[arg(long, short, default_value = "config.toml")]
        claimer: String,
        ///Sets a custom config file
        #[arg(long, short, default_value = "config.toml")]
        config: String,
    },
    /// Claim Fees
    ClaimFee {
        ///Sets a custom config file
        #[arg(long, short, default_value = "config.toml")]
        config: String,
    },
    /// Create markets.toml from tokens list
    CreateMarketsFile {
        ///Sets a custom config file
        #[arg(long, short, default_value = "config.toml")]
        config: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // let file_appender = File::create("app.log").unwrap();
    // let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        // .with_writer(non_blocking)
        .finish();
    tracing::subscriber
        ::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");

    info!("Starting ZavodMevBot");

    let args = CliArgs::parse();

    match args.run {
        RunCommand::Run { config } => {
            info!("Using config file: {}", config);
            bot::run_bot(&config).await?;
        }
        RunCommand::Wrap { amount, config } => {
            info!("Using config file: {}", config);
            bot::wrap_sol(&amount, &config).await?;
        }
        RunCommand::Token { config } => {
            info!("Using config file: {}", config);
            bot::generate_token_list(&config).await?;
        }
        RunCommand::FindLookupTables { config } => {
            info!("Using config file: {}", config);
            bot::find_all_lookup_tables(&config).await?;
        }
        RunCommand::CreateNewLookupTable { config } => {
            info!("Using config file: {}", config);
            bot::create_new_lookup_table(&config).await?;
        }
        RunCommand::CloseAllEmptyAtas { config } => {
            info!("Using config file: {}", config);
            bot::close_all_empty_atas(&config).await?;
        }
        RunCommand::UpdateVaultAuthInfo { claimer, config } => {
            info!("Using config file: {}", config);
            bot::update_vault_auth_info(&claimer, &config).await?;
        }
        RunCommand::ClaimFee { config } => {
            info!("Using config file: {}", config);
            bot::claim_fees(&config).await?;
        }
        RunCommand::CreateMarketsFile { config } => {
            info!("Using config file: {}", config);
            bot::create_markets(&config).await?;
        }
    }
    Ok(())
}
