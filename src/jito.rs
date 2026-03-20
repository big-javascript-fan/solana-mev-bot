use reqwest::Client;
use serde_json::{ json };
use solana_sdk::transaction::VersionedTransaction;
use base64::{ engine::general_purpose, Engine as _ };
use tracing::{ info, warn };
use futures::future::join_all;

pub async fn _send_tx_using_jito(tx: &VersionedTransaction, jito_urls: &[&str]) -> Option<String> {
    let client = Client::new();
    let tx_base64 = general_purpose::STANDARD.encode(bincode::serialize(tx).unwrap());

    for &url in jito_urls {
        let payload =
            json!({
            "id": 1,
            "jsonrpc": "2.0",
            "method": "sendTransaction",
            "params": [tx_base64, { "encoding": "base64" }]
        });

        let res = client
            .post(format!("{}/api/v1/transactions", url))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send().await;

        if let Ok(response) = res {
            if let Ok(data) = response.json::<serde_json::Value>().await {
                if let Some(result) = data.get("result") {
                    println!("jito: {}", result);
                    return Some(result.to_string());
                }
            }
        }
    }

    None
}

pub async fn _send_txs_using_jito_all_at_once(
    txs: &[VersionedTransaction],
    jito_urls: &Vec<String>,
    uuid: String
) -> Vec<Option<(String, usize)>> {
    let client = Client::new();
    let txs_base64: Vec<String> = txs
        .iter()
        .map(|tx| general_purpose::STANDARD.encode(bincode::serialize(tx).unwrap()))
        .collect();

    let tasks = jito_urls
        .iter()
        .enumerate()
        .map(|(i, url)| {
            let client = client.clone();
            let txs_base64 = txs_base64.clone();
            let url = url.clone();
            let uuid = uuid.clone();

            tokio::spawn(async move {
                let payload =
                    json!({
                "id": rand::random::<u16>(),
                "jsonrpc": "2.0",
                "method": "sendBundle",
                "params": [txs_base64, { "encoding": "base64" }]
                });
                let mut req_builder = client
                    .post(format!("{}/bundles", url))
                    .header("Content-Type", "application/json")
                    .json(&payload);
                if !uuid.is_empty() {
                    req_builder = req_builder.header("x-jito-auth", uuid);
                }
                match req_builder.send().await {
                    Ok(response) => {
                        if let Ok(data) = response.json::<serde_json::Value>().await {
                            if let Some(result) = data.get("result") {
                                info!("jito: {:?} (index {})", result, i);
                                return Some((result.to_string(), i));
                            } else {
                                info!("{:#?}", data);
                            }
                        }
                    }
                    Err(err) => {
                        warn!("Failed to send to {}: {}", url, err);
                    }
                }
                None
            })
        });

    // Collect all results
    let results = join_all(tasks).await;

    // Unwrap the JoinHandles safely
    results
        .into_iter()
        .map(|res| res.unwrap_or(None))
        .collect()
}

pub async fn send_tx_using_jito_all_at_once(
    tx: &VersionedTransaction,
    jito_urls: &Vec<String>,
    uuid: &String
) -> Vec<Option<(String, usize)>> {
    // info!("Sending Jito transaction to {} block engine URLs", jito_urls.len());

    let client = Client::new();
    let tx_base64: String = general_purpose::STANDARD.encode(bincode::serialize(&tx).unwrap());
    // let tx_base58: String = bs58::encode(bincode::serialize(&tx).unwrap()).into_string();

    let tasks = jito_urls
        .iter()
        .enumerate()
        .map(|(i, url)| {
            let client = client.clone();
            let tx_base64 = tx_base64.clone();
            // let tx_base58 = tx_base58.clone();
            let url = url.clone();
            let uuid = uuid.clone();

            tokio::spawn(async move {
                let payload =
                    json!({
                "id": rand::random::<u16>(),
                "jsonrpc": "2.0",
                "method": "sendTransaction",
                "params": [tx_base64, { "encoding": "base64" }]
                // "params": [tx_base58, { "encoding": "base58" }]

                });
                let mut req_builder = client
                    .post(format!("{}/transactions", url))
                    .header("Content-Type", "application/json")
                    .json(&payload);
                if !uuid.is_empty() {
                    req_builder = req_builder.header("x-jito-auth", uuid);
                }
                match req_builder.send().await {
                    Ok(response) => {
                        if let Ok(data) = response.json::<serde_json::Value>().await {
                            if let Some(result) = data.get("result") {
                                info!("jito: {:?} (index {})", result, i);
                                return Some((result.to_string(), i));
                            } else {
                                // warn!("Can't get result field: {:?}", data);
                            }
                        }
                    }
                    Err(_err) => {
                        // error!("Failed to send to {}: {}", url, err);
                    }
                }
                None
            })
        });

    // Collect all results
    let results = join_all(tasks).await;

    // Unwrap the JoinHandles safely
    results
        .into_iter()
        .map(|res| res.unwrap_or(None))
        .collect()
}

pub async fn _send_txs_using_jito_one_by_one(
    txs: &[VersionedTransaction],
    jito_url: &str,
    uuid: String
) -> Option<String> {
    let client = Client::new();
    let txs_base64: Vec<String> = txs
        .iter()
        .map(|tx| general_purpose::STANDARD.encode(bincode::serialize(tx).unwrap()))
        .collect();
    let payload =
        json!({
            "id": 1,
            "jsonrpc": "2.0",
            "method": "sendBundle",
            "params": [txs_base64, { "encoding": "base64" }]
        });
    let mut req_builder = client
        .post(format!("{}/bundles", jito_url))
        .header("Content-Type", "application/json")
        .json(&payload);
    if !uuid.is_empty() {
        req_builder = req_builder.header("x-jito-auth", uuid);
    }
    let res = req_builder.send().await;

    if let Ok(response) = res {
        if let Ok(data) = response.json::<serde_json::Value>().await {
            if let Some(result) = data.get("result") {
                info!("jito: {:?} through {}", result, jito_url);
                return Some(result.to_string());
            }
        }
    }
    None
}
