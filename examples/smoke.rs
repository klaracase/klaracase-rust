//! A smoke test against the real API. It calls the two free endpoints only.
//!
//! Run it with `make smoke`. It needs `KLARACASE_API_TOKEN` and the network. CI does not run it.

use klaracase::{CoverageParams, FacetField, FacetsParams, Klaracase};

#[tokio::main]
async fn main() -> Result<(), klaracase::Error> {
    let client = Klaracase::from_env()?;

    let coverage = client
        .corpus()
        .coverage(CoverageParams {
            top: Some(3),
            ..Default::default()
        })
        .await?;
    println!(
        "coverage: {} courts, API release {:?}, cost {:?}",
        coverage.courts_total,
        coverage.meta.api_release,
        coverage.credits.as_ref().and_then(|c| c.cost),
    );

    let facets = client
        .corpus()
        .facets(
            FacetField::CourtName,
            FacetsParams {
                prefix: Some("BG".into()),
                ..Default::default()
            },
        )
        .await?;
    println!(
        "court_name facets: {} values, BGH count {:?}",
        facets.total_values,
        facets.count("BGH")
    );
    Ok(())
}
