use anyhow::{Result};
use maud::html;
use tonic::transport::Channel;
use warp::Reply;

use crate::grpc::{bchrpc::{bchrpc_client::BchrpcClient}, connect_bchd, latest_blocks};

pub struct Server {
    bchd: BchrpcClient<Channel>,
}

impl Server {
    pub async fn setup() -> Result<Self> {
        let bchd = connect_bchd().await?;
        Ok(Server {
            bchd,
        })
    }
}

impl Server {
    pub async fn latest_blocks(&self) -> Result<impl Reply> {
        let mut bchd = self.bchd.clone();
        let latest_blocks = latest_blocks(&mut bchd).await?;
        let markup = html! {
            ol {
                @for block in &latest_blocks {
                    li { (hex::encode(&block.hash)) }
                }
            } 
        };
        Ok(warp::reply::html(markup.into_string()))
    }
}
