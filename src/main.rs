use tibber_status::v1::run::{
    get_api_endpoint, get_db_info, get_instant, get_logger, get_token, tick,
};
use tokio::time;

#[tokio::main]
async fn main() {
    let (subscriber, _guard) = get_logger();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");
    tracing::trace!("Log setup complete");

    let (db_addr, db_name) = get_db_info();
    let api_endpoint = get_api_endpoint();
    let auth = get_token().await;

    loop {
        let instant = get_instant();
        time::sleep_until(instant).await;
        for i in 0..10 {
            let res = tick(
                auth.clone(),
                api_endpoint.clone(),
                db_addr.clone(),
                db_name.clone(),
            )
            .await;
            if res.is_ok() {
                break;
            } else {
                tracing::warn!("Failed attempt {} to tick: {}", i, res.err().unwrap());
                let b = (2 as u64).pow(i);
                tracing::debug!("Exponential backoff: {} seconds", b);
                time::sleep(time::Duration::from_secs(b)).await;
            }
        }
    }
}
