use anyhow::Result;
use maud::{DOCTYPE, html, PreEscaped};
use warp::Reply;
use serde::Serialize;

use crate::{blockchain::to_le_hex, db::Db, grpc::Bchd};

pub struct Server {
    bchd: Bchd,
}

impl Server {
    pub async fn setup(db: Db) -> Result<Self> {
        let bchd = Bchd::connect(db).await?;
        Ok(Server {
            bchd,
        })
    }
}

impl Server {
    pub async fn dashboard(&self) -> Result<impl Reply> {
        let blockchain_info = self.bchd.blockchain_info().await?;
        let page_size = 2000;
        let current_page_height = (blockchain_info.best_height / page_size) * page_size;
        let current_page_end = blockchain_info.best_height;
        let last_page_height = current_page_height - page_size;
        let last_page_end = current_page_height - 1;
        let markup = html! {
            (DOCTYPE)
            head {
                meta charset="utf-8";
                title { "be.cash Block Explorer" }
                script
                    src="https://code.jquery.com/jquery-3.1.1.min.js"
                    integrity="sha256-hVVnYaiADRTO2PzUGmuLJr8BLUSjGIZsDYGmIJLv2b8="
                    crossorigin="anonymous" {}
                script type="text/javascript" src="code/semantic-ui/semantic.js?v=0" {}
                script type="text/javascript" src="code/webix/webix.js?v=8.1.0" {}
                script type="text/javascript" src="code/moment.min.js?v=0" {}
                link rel="stylesheet" href="code/webix/webix.css";
                link rel="stylesheet" href="code/semantic-ui/semantic.css";
                link rel="stylesheet" href="code/styles/index.css";
                link rel="preconnect" href="https://fonts.gstatic.com";
                link href="https://fonts.googleapis.com/css2?family=Ubuntu+Mono&display=swap" rel="stylesheet";
            }
            body {
                .ui.main.menu {
                    .header.item {
                        img.logo src="assets/logo.png" {}
                        "be.cash Explorer"
                    }
                    a.item href="/blocks" { "Blocks" }
                    .item {
                        #search-box.ui.transparent.icon.input {
                            input type="text" placeholder="Search blocks, transactions, adddresses, tokens..." {}
                            i.search.link.icon {}
                        }
                    }
                    .ui.right.floated.dropdown.item href="#" {
                        "Bitcoin ABC"
                        i.dropdown.icon {}
                        .menu {
                            .item { "Bitcoin ABC" }
                        }
                    }
                }
                script { (PreEscaped(r#"
                    $('.main.menu  .ui.dropdown').dropdown({
                        on: 'hover'
                    });
                "#)) }

                #blocks {
                    #blocks-table {}
                }
                
                script type="text/javascript" src={"data/blocks/" (current_page_height) "/" (current_page_end) "/dat.js"} {}
                script type="text/javascript" src={"data/blocks/" (last_page_height) "/" (last_page_end) "/dat.js"} {}
                script type="text/javascript" src="code/blocks.js" {}
            }
        };
        Ok(warp::reply::html(markup.into_string()))
    }

    pub async fn blocks(&self, start_height: i32, _end_height: i32) -> Result<impl Reply> {
        let blocks = self.bchd.blocks_above(start_height - 1).await?;
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Block {
            hash: String,
            height: i32,

            version: i32,
            timestamp: i64,

            difficulty: f64,
            size: u64,
            num_txs: u64,
            median_time: i64,
        }
        let mut json_blocks = Vec::with_capacity(blocks.len());
        for block in blocks.into_iter().rev() {
            json_blocks.push(Block {
                hash: to_le_hex(&block.block_info.hash),
                height: block.block_info.height,
                version: block.block_info.version,
                timestamp: block.block_info.timestamp,
                difficulty: block.block_info.difficulty,
                size: block.block_meta.size,
                median_time: block.block_meta.median_time,
                num_txs: block.block_meta.num_txs,
            });
        }
        let encoded_blocks = serde_json::to_string(&json_blocks)?;
        let reply = format!(r#"
            if (window.blockData === undefined)
                window.blockData = [];
            {{
                var blocks = JSON.parse('{encoded_blocks}');
                var startIdx = window.blockData.length;
                window.blockData.length += blocks.length;
                for (var i = 0; i < blocks.length; ++i) {{
                    var block = blocks[i];
                    window.blockData[startIdx + i] = {{
                        hash: block.hash,
                        height: block.height,
                        version: block.version,
                        timestamp: new Date(block.timestamp * 1000),
                        difficulty: block.difficulty,
                        size: block.size,
                        medianTime: block.medianTime,
                        numTxs: block.numTxs,
                    }};
                }}
            }}
        "#, encoded_blocks = encoded_blocks);
        let reply = warp::reply::with_header(reply, "content-type", "application/javascript");
        let reply = warp::reply::with_header(reply, "last-modified", "Tue, 29 Dec 2020 06:31:27 GMT");
        Ok(reply)
    }
}

/*
        #[derive(Serialize)]
        struct Block {
            hash: String,
            height: i32,

            version: i32,
            previous_block: String,
            merkle_root: String,
            timestamp: i64,
            bits: u32,
            nonce: u32,
            header: String,

            confirmations: i32,
            difficulty: f64,
            next_block_hash: Option<String>,
            size: i32,
            median_time: i64,
        }

            blockheader.version = block.version;
            blockheader.previous_block = block.previous_block.try_into().map_err(|_| anyhow!("No previous block"))?;
            blockheader.merkle_root = block.merkle_root.try_into().map_err(|_| anyhow!("No merkle root"))?;
            blockheader.timestamp = block.timestamp.try_into()?;
            blockheader.bits = block.bits;
            blockheader.nonce = block.nonce;
*/