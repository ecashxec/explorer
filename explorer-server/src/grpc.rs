use anyhow::{anyhow, Result};
use bchrpc::bchrpc_client::BchrpcClient;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};

pub mod bchrpc {
    tonic::include_proto!("pb");
}

pub async fn connect_bchd() -> Result<BchrpcClient<Channel>> {
    use std::fs;
    use std::io::Read;
    let mut cert_file = fs::File::open("cert.crt")?;
    let mut cert = Vec::new();
    cert_file.read_to_end(&mut cert)?;
    let tls_config = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(&cert));
    let endpoint = Endpoint::from_static("https://api2.be.cash:8345").tls_config(tls_config)?;
    let client = BchrpcClient::connect(endpoint).await?;
    Ok(client)
}

pub async fn latest_blocks(bchd: &mut BchrpcClient<Channel>) -> Result<Vec<bchrpc::BlockInfo>> {
    use bchrpc::{GetBlockchainInfoRequest, GetBlockInfoRequest, get_block_info_request::HashOrHeight, GetHeadersRequest};
    let mut bchd = bchd.clone();
    let number_of_blocks = 2000;
    let blockchain_info = bchd.get_blockchain_info(GetBlockchainInfoRequest {}).await?;
    let blockchain_info = blockchain_info.get_ref();

    let first_block_height = blockchain_info.best_height - number_of_blocks;
    let first_block_info = bchd.get_block_info(GetBlockInfoRequest {
        hash_or_height: Some(HashOrHeight::Height(first_block_height))
    }).await?;
    let first_block_info = first_block_info.get_ref();
    let first_block_info = first_block_info.info.as_ref()
        .ok_or_else(|| anyhow!("No block info"))?;
        
    let latest_headers = bchd.get_headers(GetHeadersRequest {
        block_locator_hashes: vec![first_block_info.hash.clone()],
        stop_hash: blockchain_info.best_block_hash.clone(),
    }).await?;
    let latest_headers = latest_headers.get_ref();
    let latest_headers = latest_headers.headers.clone();
    Ok(latest_headers)
}
