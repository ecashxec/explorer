use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use anyhow::{Result, Context};
use db::Db;
use server::Server;
use warp::{Filter, Rejection, Reply, hyper::StatusCode};
use serde::Serialize;

mod blockchain;
mod db;
mod grpc;
mod server;

type ServerRef = Arc<Server>;

fn with_server(
    server: &ServerRef,
) -> impl Filter<Extract = (ServerRef,), Error = std::convert::Infallible> + Clone {
    let server = Arc::clone(&server);
    warp::any().map(move || Arc::clone(&server))
}

#[tokio::main]
async fn main() -> Result<()> {
    let host: SocketAddr = "127.0.0.1:3035"
        .parse()
        .with_context(|| "Invalid host in config")?;

    let db = Db::open("../db.sled")?;

    let server = Arc::new(Server::setup(db).await?);

    let dashboard = warp::path::end()
        .and(with_server(&server))
        .and_then(|server: ServerRef| async move {
            server.dashboard().await.map_err(err)
        });

    let data_blocks =
        warp::path!("data" / "blocks" / i32 / i32 / "dat.js")
        .and(with_server(&server))
        .and_then(|start_height, end_height, server: ServerRef| async move {
            server.blocks(start_height, end_height).await.map_err(err)
        });

    let js = warp::path("code")
        .and(warp::fs::dir("./code"));

    let favicon = warp::path!("favicon.ico")
        .and(warp::fs::file("./assets/favicon.png"));

    let assets = warp::path!("assets" / "logo.png")
        .and(warp::fs::file("./assets/logo.png"));

    let routes = dashboard
        .or(data_blocks)
        .or(js)
        .or(favicon)
        .or(assets)
        .recover(handle_rejection);

    warp::serve(routes).run(host).await;

    Ok(())
}

#[derive(Debug)]
struct AnyhowError(anyhow::Error);
impl warp::reject::Reject for AnyhowError {}
fn err(err: anyhow::Error) -> Rejection {
    warp::reject::custom(AnyhowError(err))
}

#[derive(Serialize)]
struct ErrorMessage {
    success: bool,
    msg: String,
}

async fn handle_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
    let msg;
    if let Some(AnyhowError(anyhow_error)) = err.find::<AnyhowError>() {
        println!("Anyhow error: {:?}", anyhow_error);
        msg = anyhow_error.to_string();
    } else {
        println!("Other error: {:?}", err);
        msg = "Unknown message".to_string();
    }
    return Ok(warp::reply::with_status(
        warp::reply::json(&ErrorMessage {
            success: false,
            msg,
        }),
        StatusCode::INTERNAL_SERVER_ERROR,
    ));
}
