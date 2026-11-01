//! Searches court decisions and prints the hits.
//!
//! Run it with a real token: `KLARACASE_API_TOKEN=<token> cargo run --example search`.
//! A search costs one credit.

use klaracase::Klaracase;

#[tokio::main]
async fn main() -> Result<(), klaracase::Error> {
    let client = Klaracase::from_env()?;

    let found = client
        .decisions()
        .search("Vermögensbetreuungspflicht Untreue")
        .court("BGH")
        .limit(5)
        .send()
        .await?;

    for hit in &found.results {
        println!(
            "{} {} {}",
            hit.court.as_deref().unwrap_or("-"),
            hit.aktenzeichen.as_deref().unwrap_or("-"),
            hit.date.as_deref().unwrap_or("-"),
        );
    }
    if let Some(credits) = &found.credits {
        println!(
            "cost: {:?}, remaining: {:?}",
            credits.cost, credits.credits_remaining
        );
    }
    Ok(())
}
