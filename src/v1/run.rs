use std::{env, sync::Arc};

use chrono::{DateTime, Utc};
use influxdb::{Client, InfluxDbWriteable};
use tokio::time;
use tracing::{instrument, metadata::LevelFilter, Level};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::{
    fmt::format::{DefaultFields, FmtSpan, Format},
    FmtSubscriber,
};

const DEFAULT_RETRIES: u32 = 10;
const DEFAULT_UPDATE_TIME: &'static str = "11";

use super::tibber::get_price_info;

#[instrument]
pub fn get_db_info() -> (Arc<String>, Arc<String>) {
    let db_addr = env::var("INFLUXDB_ADDR").expect("INFLUXDB_ADDR not set");
    tracing::info!("INFLUXDB_ADDR: {}", db_addr);

    let db_name = env::var("INFLUXDB_DB_NAME").expect("INFLUXDB_DB_NAME not set");
    tracing::info!("INFLUXDB_DB_NAME: {}", db_name);

    (Arc::new(db_addr), Arc::new(db_name))
}

#[instrument]
pub fn get_api_endpoint() -> Arc<String> {
    let api_endpoint = env::var("TIBBER_API_ENDPOINT").expect("TIBBER_API_ENDPOINT not set");
    tracing::info!("TIBBER_API_ENDPOINT: {}", api_endpoint);

    Arc::new(api_endpoint)
}

pub fn get_retries() -> u32 {
    let retries = env::var("RETRIES")
        .ok()
        .unwrap_or(DEFAULT_RETRIES.to_string());
    tracing::info!("RETRIES: {}", retries);

    retries.parse().unwrap_or_else(|e| {
        tracing::warn!(
            "Failed to parse {}, using default: {}",
            retries,
            DEFAULT_RETRIES
        );
        tracing::debug!("{}", e);
        10
    })
}

#[instrument]
pub async fn get_token() -> Arc<String> {
    let token;
    let token_res = env::var("TIBBER_TOKEN");
    match token_res {
        Ok(t) => {
            token = t;
        }
        Err(_) => {
            tracing::info!("TIBBER_TOKEN not set");
            tracing::info!("Attempting to read from file");
            let token_file = env::var("TOKEN_FILE").ok();
            let token_file_str = token_file.as_ref().map(|s| s.as_str());
            let credentials = local_credentials::async_get_credentials(token_file_str).await;
            match credentials {
                Ok(c) => {
                    token = c.password;
                }
                Err(e) => {
                    tracing::error!("Failed to read credentials: {}", e);
                    panic!("TIBBER_TOKEN not set and fallback failed");
                }
            }
        }
    }

    Arc::new(token)
}

pub fn get_logger() -> (
    FmtSubscriber<DefaultFields, Format, LevelFilter, NonBlocking>,
    WorkerGuard,
) {
    let appender = tracing_appender::rolling::daily("./var/log", "tibber-status-server");
    let (non_blocking_appender, guard) = tracing_appender::non_blocking(appender);

    let level = match env::var("LOG_LEVEL") {
        Ok(l) => match l.as_str() {
            "trace" => Level::TRACE,
            "debug" => Level::DEBUG,
            "info" => Level::INFO,
            "warn" => Level::WARN,
            "error" => Level::ERROR,
            _ => Level::INFO,
        },
        Err(_) => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_span_events(FmtSpan::NONE)
        .with_ansi(false)
        .with_max_level(level)
        .with_writer(non_blocking_appender)
        // completes the builder.
        .finish();

    (subscriber, guard)
}

#[instrument(skip_all, level = "trace")]
pub async fn tick(
    auth: Arc<String>,
    api_url: Arc<String>,
    db_addr: Arc<String>,
    db_name: Arc<String>,
) -> Result<(), String> {
    tracing::debug!("tick");
    let prices_opt = get_price_info(auth.as_str(), api_url.as_str()).await;
    match prices_opt {
        Ok(prices) => {
            let date_tomorrow = Utc::now().date().succ().and_hms(0, 0, 0).to_rfc3339();
            tracing::info!("Writing price info for {}", date_tomorrow);
            let client = Client::new(db_addr.as_str(), db_name.as_str());
            let n = prices.len();
            for (i, price) in prices.into_iter().enumerate() {
                write_to_db(&client, price, i as u8, date_tomorrow.clone(), "price_info").await;
            }
            tracing::info!("Done writing price info for {}; {} written", date_tomorrow, n);
            Ok(())
        }
        Err(e) => {
            tracing::error!("error getting charger state: {}", e);
            Err(e)
        }
    }
}

#[derive(InfluxDbWriteable)]
struct DBPriceInfo {
    pub time: DateTime<Utc>,
    pub price: f64,
    pub hour: u8,
    #[influxdb(tag)]
    pub date: String,
}

#[instrument(skip(client), level = "trace")]
async fn write_to_db(client: &Client, price: f64, hour: u8, date: String, measurement: &str) {
    let variable = DBPriceInfo {
        time: Utc::now(),
        price,
        hour,
        date,
    };

    let write_result = client.query(variable.into_query(measurement)).await;
    match write_result {
        Ok(_) => {
            tracing::trace!("Writing {} success", hour);
        }
        Err(e) => {
            tracing::error!("Writing {} failed: {}", hour, e);
        }
    }
}

pub fn get_instant() -> time::Instant {
    let time = env::var("UPDATE_TIME").ok().unwrap_or(DEFAULT_UPDATE_TIME.to_string());
    let time = time.parse().unwrap();
    let when = chrono::Utc::now().date().succ().and_hms(time, 0, 0);
    tracing::info!("Next update time: {}", when);
    let next_day = when.signed_duration_since(chrono::Utc::now());
    let std_next_day = match next_day.to_std() {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Failed to convert signed duration to std: {}", e);
            panic!("Failed to convert signed duration to std");
        }
    };

    let instant = match time::Instant::now().checked_add(std_next_day) {
        Some(i) => i,
        None => {
            tracing::error!("Failed to add signed duration to instant");
            panic!("Failed to add signed duration to instant");
        }
    };
    instant
}
