use graphql_client::{reqwest::post_graphql, GraphQLQuery};

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "tibber_schema.json",
    query_path = "tibber_query.graphql",
    response_derives = "Debug"
)]
struct PriceInfo;

pub async fn get_price_info(auth: &str, url: &str) -> Result<Vec<f64>, String> {
    let client = reqwest::Client::builder()
        .user_agent("graphql-rust/0.10.0")
        .default_headers(
            std::iter::once((
                reqwest::header::AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(&format!("Bearer {}", auth)).unwrap(),
            ))
            .collect(),
        )
        .build()
        .unwrap();
    let response_body = post_graphql::<PriceInfo, _>(&client, url, price_info::Variables)
        .await
        .map_err(|e| {
            tracing::error!("{:?}", e);
            format!("{}", e)
        })?;
    let response_data = response_body.data.ok_or_else(|| {
        tracing::warn!("No data");
        "No data"
    })?;

    let home = response_data
        .viewer
        .homes
        .iter()
        .flatten()
        .next()
        .ok_or_else(|| {
            tracing::warn!("No homes");
            "No homes"
        })?;
    match home.current_subscription {
        Some(ref subscription) => match subscription.price_info {
            Some(ref price_info) => {
                let mut prices = Vec::new();
                for price in price_info.tomorrow.iter() {
                    prices.push(
                        price
                            .as_ref()
                            .ok_or_else(|| {
                                tracing::warn!("Missing price");
                                "Missing price"
                            })?
                            .total
                            .ok_or_else(|| {
                                tracing::warn!("Missing total");
                                "Missing total"
                            })?,
                    )
                }
                Ok(prices)
            }
            None => {
                tracing::warn!("No price info");
                Err("No price info".to_string())
            }
        },
        None => {
            tracing::warn!("No current subscription");
            Err("No current subscription".to_string())
        }
    }
}
