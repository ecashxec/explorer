use anyhow::Result;
use maud::{DOCTYPE, Markup, PreEscaped, html};
use warp::Reply;
use serde::Serialize;
use chrono::{Utc, TimeZone};
use chrono_humanize::HumanTime;
use std::{collections::{HashMap, hash_map::Entry}, convert::TryInto};

use crate::{blockchain::{BlockHeader, from_le_hex, to_le_hex}, db::{Db, SlpAction, TxMetaVariant}, formatting::{format_byte_size, format_difficulty, format_integer}, grpc::Bchd};

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
                (self.toolbar())

                #blocks {
                    #blocks-table {}
                }
                
                script type="text/javascript" src={"/data/blocks/" (current_page_height) "/" (current_page_end) "/dat.js"} {}
                script type="text/javascript" src={"/data/blocks/" (last_page_height) "/" (last_page_end) "/dat.js"} {}
                script type="text/javascript" src="/code/blocks.js" {}
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

    pub async fn block_txs(&self, block_hash: &str) -> Result<impl Reply> {
        let block_hash = from_le_hex(block_hash)?;
        let block_txs = self.bchd.block_txs(&block_hash).await?;
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Tx {
            tx_hash: String,
            block_height: i32,
            is_coinbase: bool,
            size: i32,
            num_inputs: u32,
            num_outputs: u32,
            sats_input: i64,
            sats_output: i64,
            token_idx: Option<usize>,
            is_burned_slp: bool,
            token_input: u64,
            token_output: u64,
            slp_action: Option<SlpAction>,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Token {
            token_id: String,
            token_type: u32,
            token_ticker: String,
            token_name: String,
            decimals: u32,
            group_id: Option<String>,
        }
        let mut json_txs = Vec::with_capacity(block_txs.len());
        let mut token_indices = HashMap::<&[u8; 32], usize>::new();
        for (tx_hash, tx_meta) in block_txs.iter() {
            let mut tx = Tx {
                tx_hash: to_le_hex(&tx_hash),
                block_height: tx_meta.block_height,
                is_coinbase: tx_meta.is_coinbase,
                size: tx_meta.size,
                num_inputs: tx_meta.num_inputs,
                num_outputs: tx_meta.num_outputs,
                sats_input: tx_meta.sats_input,
                sats_output: tx_meta.sats_output,
                token_idx: None,
                is_burned_slp: false,
                token_input: 0,
                token_output: 0,
                slp_action: None,
            };
            let mut tx_token_id = None;
            match &tx_meta.variant {
                TxMetaVariant::Normal => {},
                TxMetaVariant::InvalidSlp { token_id, token_input } => {
                    tx_token_id = Some(token_id);
                    tx.is_burned_slp = true;
                    tx.token_input = *token_input;
                }
                TxMetaVariant::Slp { token_id, token_input, token_output, action } => {
                    tx_token_id = Some(token_id);
                    tx.token_input = *token_input;
                    tx.token_output = *token_output;
                    tx.slp_action = Some(*action);
                }
            }
            if let Some(token_id) = tx_token_id {
                let num_tokens = token_indices.len();
                match token_indices.entry(token_id) {
                    Entry::Vacant(vacant) => {
                        vacant.insert(num_tokens);
                        tx.token_idx = Some(num_tokens);
                    },
                    Entry::Occupied(occupied) => {
                        tx.token_idx = Some(*occupied.get());
                    }
                }
            }
            json_txs.push(tx);
        }
        let tokens = self.bchd.tokens(token_indices.keys().map(|key| &key[..])).await?;
        let mut token_data = tokens.into_iter().zip(token_indices).collect::<Vec<_>>();
        token_data.sort_unstable_by_key(|&(_, (_, idx))| idx);
        let json_tokens = token_data.into_iter().map(|(token_meta, (token_id, _))| {
            let token_ticker = String::from_utf8_lossy(&token_meta.token_ticker);
            let token_name = String::from_utf8_lossy(&token_meta.token_name);
            Token {
                token_id: hex::encode(token_id),
                token_type: token_meta.token_type,
                token_ticker: html! { (token_ticker) }.into_string(),
                token_name: html! { (token_name) }.into_string(),
                decimals: token_meta.decimals,
                group_id: token_meta.group_id.map(|group_id| hex::encode(&group_id)),
            }
        }).collect::<Vec<_>>();
        let encoded_txs = serde_json::to_string(&json_txs)?;
        let encoded_tokens = serde_json::to_string(&json_tokens)?;
        let reply = format!(r#"
            if (window.txData === undefined)
                window.txData = [];
            {{
                var txs = JSON.parse('{encoded_txs}');
                var tokens = JSON.parse('{encoded_tokens}');
                var startIdx = window.txData.length;
                window.txData.length += txs.length;
                for (var i = 0; i < txs.length; ++i) {{
                    var tx = txs[i];
                    tx.token = tx.tokenIdx === null ? null : tokens[tx.tokenIdx];
                    window.txData[startIdx + i] = tx;
                }}
            }}
        "#, encoded_txs = encoded_txs, encoded_tokens = encoded_tokens);
        let reply = warp::reply::with_header(reply, "content-type", "application/javascript");
        let reply = warp::reply::with_header(reply, "last-modified", "Tue, 29 Dec 2020 06:31:27 GMT");
        Ok(reply)
    }

    pub async fn block(&self, block_hash_str: &str) -> Result<impl Reply> {
        let block_hash = from_le_hex(block_hash_str)?;
        let block_meta_info = self.bchd.block_meta_info(&block_hash).await?;
        let block_info = block_meta_info.block_info;
        let block_meta = block_meta_info.block_meta;
        let timestamp = Utc.timestamp(block_info.timestamp, 0);
        let mut block_header = BlockHeader::default();
        block_header.version = block_info.version;
        block_header.previous_block = block_info.previous_block.as_slice().try_into()?;
        block_header.merkle_root = block_info.merkle_root.as_slice().try_into()?;
        block_header.timestamp = block_info.timestamp.try_into()?;
        block_header.bits = block_info.bits;
        block_header.nonce = block_info.nonce;
        
        let markup = html! {
            (DOCTYPE)
            head {
                title { "be.cash Block Explorer" }
                (self.head_common())
                script type="text/javascript" src="/code/common.js" {}
            }
            body {
                (self.toolbar())

                .ui.container {
                    h1 {
                        "Block #"
                        (block_info.height)
                    }
                    .ui.segment {
                        strong { "Hash: " }
                        span.hex { (block_hash_str) }
                    }
                    .ui.grid {
                        .six.wide.column {
                            table.ui.table {
                                tbody {
                                    tr {
                                        td { "Age" }
                                        td { (HumanTime::from(timestamp)) }
                                    }
                                    tr {
                                        td { "Mined on" }
                                        td { (PreEscaped(format!(r#"<script type="text/javascript">
                                            document.write(moment({timestamp}).format('L LTS'));
                                            document.write(' <small>(UTC' + tzOffset + ')</small>');
                                        </script>"#, timestamp=block_info.timestamp * 1000))) }
                                    }
                                    tr {
                                        td { "Unix Timestamp" }
                                        td { (format_integer(block_info.timestamp as u64)) }
                                    }
                                    tr {
                                        td { "Mined by" }
                                        td { "Unknown" }
                                    }
                                    tr {
                                        td { "Confirmations" }
                                        td { (block_info.confirmations) }
                                    }
                                    tr {
                                        td { "Size" }
                                        td { (format_byte_size(block_meta.size)) }
                                    }
                                    tr {
                                        td { "Transactions" }
                                        td { (block_meta.num_txs) }
                                    }
                                }
                            }
                        }
                        .ten.wide.column {
                            table.ui.table {
                                tbody {
                                    tr {
                                        td { "Difficulty" }
                                        td { (format_difficulty(block_info.difficulty)) }
                                    }
                                    tr {
                                        td { "Header" }
                                        td {
                                            .hex {
                                                (hex::encode(block_header.as_slice()))
                                            }
                                        }
                                    }
                                    tr {
                                        td { "Nonce" }
                                        td { (block_info.nonce) }
                                    }
                                    tr {
                                        td { "Coinbase data" }
                                        td {
                                            (String::from_utf8_lossy(&block_meta.coinbase_data))
                                        }
                                    }
                                    tr {
                                        td { "Coinbase hex" }
                                        td {
                                            .hex {
                                                (hex::encode(&block_meta.coinbase_data))
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    .ui.segment {
                        h2.ui.header { "Transactions" }
                        #txs-table {}
                    }
                }
                script type="text/javascript" src={"/data/block/" (block_hash_str) "/dat.js"} {}
                script type="text/javascript" src="/code/txs.js" {}
            }
        };
        Ok(warp::reply::html(markup.into_string()))
    }

    fn head_common(&self) -> Markup {
        html! {
            meta charset="utf-8";
            script
                src="https://code.jquery.com/jquery-3.1.1.min.js"
                integrity="sha256-hVVnYaiADRTO2PzUGmuLJr8BLUSjGIZsDYGmIJLv2b8="
                crossorigin="anonymous" {}
            script type="text/javascript" src="/code/semantic-ui/semantic.js?v=0" {}
            script type="text/javascript" src="/code/webix/webix.js?v=8.1.0" {}
            script type="text/javascript" src="/code/moment.min.js?v=0" {}
            link rel="stylesheet" href="/code/webix/webix.css";
            link rel="stylesheet" href="/code/semantic-ui/semantic.css";
            link rel="stylesheet" href="/code/styles/index.css";
            link rel="preconnect" href="https://fonts.gstatic.com";
            link href="https://fonts.googleapis.com/css2?family=Ubuntu+Mono&display=swap" rel="stylesheet";
        }
    }

    fn toolbar(&self) -> Markup {
        html! {
            .ui.main.menu {
                .header.item {
                    img.logo src="/assets/logo.png" {}
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
        }
    }
}
