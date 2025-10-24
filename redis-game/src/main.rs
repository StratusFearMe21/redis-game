use std::{collections::HashSet, time::Duration};

use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::Response,
    routing::get,
};
use bebop::Record;
use color_eyre::eyre::{self, Context, OptionExt, eyre};
use redis::{AsyncConnectionConfig, AsyncTypedCommands, Client, PushInfo, Value};
use tokio::{
    net::TcpListener,
    signal,
    time::{self, Instant},
};
use tower_http::{catch_panic::CatchPanicLayer, services::ServeDir};
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::error::WithStatusCode;

mod error;
mod messages;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;

    let redis_client = Client::open("redis://localhost?protocol=resp3").unwrap();
    redis_client
        .get_multiplexed_async_connection()
        .await
        .wrap_err("Failed to open redis connection")?
        .flushall()
        .await
        .wrap_err("Failed to flushall the database")?;

    tracing_subscriber::registry()
        .with(ErrorLayer::default())
        .with(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("info"))
                .unwrap(),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new()
        .route("/ws", get(game_server))
        .fallback_service(
            ServeDir::new("dist")
                .precompressed_gzip()
                .precompressed_br(),
        )
        .layer(CatchPanicLayer::custom(error::PanicHandler))
        .with_state(redis_client);

    let addr = "[::]:3000".parse::<std::net::SocketAddr>().unwrap();
    let listener = TcpListener::bind(addr)
        .await
        .wrap_err_with(|| format!("Failed to open listener on {}", addr))?;
    tracing::info!("Listening on {}", addr);
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .wrap_err("Failed to serve make service")
}

async fn game_server(
    State(pool): State<Client>,
    ws: WebSocketUpgrade,
) -> Result<Response, error::Error> {
    Ok(ws.on_upgrade(move |mut socket| async move {
        if let Err(e) = handle_socket(&mut socket, pool).await {
            tracing::error!(?e);
            let _ = socket.send(Message::Text(format!("{}", e).into())).await;
        }
    }))
}

async fn handle_socket(socket: &mut WebSocket, pool: Client) -> Result<(), error::Error> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let config = AsyncConnectionConfig::new().set_push_sender(tx);

    let mut db = pool
        .get_multiplexed_async_connection_with_config(&config)
        .await
        .wrap_err("Failed to get connection to Redis")
        .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;

    let Message::Text(name) = socket
        .recv()
        .await
        .ok_or_eyre("Did not receive any name message")
        .with_status_code(StatusCode::BAD_REQUEST)?
        .wrap_err("Failed to receive name message")
        .with_status_code(StatusCode::BAD_REQUEST)?
    else {
        return Err(eyre!("Name message was not a text message"))
            .with_status_code(StatusCode::BAD_REQUEST);
    };

    let bradshaw = name == "Bradshaw";

    if !bradshaw {
        db.set(name.as_str(), 0i32)
            .await
            .wrap_err("Failed to set initial name key")
            .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let mut keys_to_watch_set = HashSet::new();
    let mut keys_to_watch = Vec::new();

    {
        let mut scan: redis::AsyncIter<'_, String> = db
            .scan()
            .await
            .wrap_err("Failed to open scan on Redis connection")
            .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;
        while let Some(key) = scan.next_item().await {
            let key = key
                .wrap_err("Failed to get key from Redis scan")
                .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;
            keys_to_watch_set.insert(key.clone());
            keys_to_watch.push(key);
        }
    }

    if !bradshaw {
        db.publish("joins", name.as_str())
            .await
            .wrap_err("Failed to publish join to join channel")
            .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    db.subscribe(&["joins", "leaves"])
        .await
        .wrap_err("Failed to subscribe to channels")
        .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;

    let sleep = time::sleep(Duration::from_millis(50));
    tokio::pin!(sleep);

    let mut clear = true;

    loop {
        tokio::select! {
            Some(msg) = socket.recv() => {
                let msg = if let Ok(msg) = msg {
                    msg
                } else {
                    // client disconnected
                    break;
                };

                let Message::Binary(msg) = msg else {
                    // client disconnected
                    break;
                };

                if let Ok(msg) = messages::redis_game::GameMessage::deserialize(&msg) {
                    if let Some(clicks) = msg.clicks {
                        for click in clicks {
                            db.incr(click.key, click.value)
                                .await
                                .wrap_err("Failed to increment key on click")
                                .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;
                        }
                    }
                };
            }
            _ = &mut sleep => {
                let mut key_values = Vec::new();
                if !keys_to_watch.is_empty() {
                    let values = db
                        .mget_ints(&keys_to_watch)
                        .await
                        .wrap_err("Failed to mget keys")
                        .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;
                    for (key, value) in keys_to_watch.iter().zip(values) {
                        key_values.push(messages::redis_game::KeyValue {
                            key,
                            value: value.unwrap_or(0) as i64,
                        });
                    }
                }
                let mut buf = Vec::new();
                messages::redis_game::GameMessage {
                    updates: Some(key_values),
                    clear: Some(clear),
                    ..Default::default()
                }
                .serialize(&mut buf)
                .wrap_err("Failed to serialize game message")
                .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;
                clear = false;
                socket.send(Message::Binary(buf.into()))
                    .await
                    .wrap_err("Failed to send binary game message")
                    .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;
                sleep
                    .as_mut()
                    .reset(Instant::now() + Duration::from_millis(50));
            }
            Some(msg) = rx.recv() => {
                match msg {
                    PushInfo {
                        kind: redis::PushKind::Message,
                        mut data,
                    } => {
                        if let (Some(Value::BulkString(new_name)), Some(Value::BulkString(channel))) =
                            (data.pop(), data.pop())
                        {
                            match String::from_utf8_lossy(&channel).as_ref() {
                                "joins" => {
                                    let new_name = String::from_utf8(new_name)
                                        .wrap_err("Join name was not valid UTF-8")
                                        .with_status_code(StatusCode::BAD_REQUEST)?;
                                    if keys_to_watch_set.insert(new_name.clone()) {
                                        keys_to_watch.push(new_name);
                                    }
                                    clear = true;
                                }
                                "leaves" => {
                                    let name = String::from_utf8(new_name)
                                        .wrap_err("Join name was not valid UTF-8")
                                        .with_status_code(StatusCode::BAD_REQUEST)?;
                                    keys_to_watch_set.remove(&name);
                                    if let Some(name_pos) =
                                        keys_to_watch.iter().position(|key| key == &name)
                                    {
                                        keys_to_watch.swap_remove(name_pos);
                                        clear = true;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if !bradshaw {
        db.del(name.as_str())
            .await
            .wrap_err("Failed to delete name key")
            .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;
        db.publish("leaves", name.as_str())
            .await
            .wrap_err("Failed to publish name to leaves channel")
            .with_status_code(StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
