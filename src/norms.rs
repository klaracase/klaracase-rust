//! Statute norms: `GET /norms/{reference}`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::{Klaracase, Request};
use crate::common::Credits;
use crate::encode::{Query, path_segment};
use crate::error::Error;
use crate::response::{Response, impl_extras};

/// Parameters of [`Norms::get`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GetNormParams {
    /// `DE` or `EU`. Read only when the abbreviation is ambiguous.
    pub jurisdiction: Option<String>,
}

impl GetNormParams {
    fn to_query(&self) -> Query {
        let mut query = Query::new();
        query.push_opt("jurisdiction", &self.jurisdiction);
        query
    }
}

/// The statute a norm belongs to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormLaw {
    /// The statute id.
    #[serde(default)]
    pub id: String,
    /// The abbreviation, for example `BGB`.
    #[serde(default)]
    pub abbreviation: String,
    /// The abbreviation in the source.
    #[serde(default)]
    pub source_abbreviation: Option<String>,
    /// The official name.
    #[serde(default)]
    pub official_name: Option<String>,
    /// For example `BUND`.
    #[serde(default)]
    pub law_level: Option<String>,
    /// The jurisdiction code.
    #[serde(default)]
    pub jurisdiction: Option<String>,
    /// Where the text comes from.
    #[serde(default)]
    pub source_url: Option<String>,
    /// For example `confirmed`.
    #[serde(default)]
    pub upstream_status: Option<String>,
    /// When the source was last seen.
    #[serde(default)]
    pub upstream_last_seen_at: Option<String>,
    /// Since when the source has been missing, if it is.
    #[serde(default)]
    pub upstream_missing_since: Option<String>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The text of one norm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormBody {
    /// The number, for example `§ 535`.
    #[serde(default)]
    pub number: String,
    /// The title.
    #[serde(default)]
    pub title: Option<String>,
    /// For example `current`.
    #[serde(default)]
    pub lifecycle: Option<String>,
    /// The full current text.
    #[serde(default)]
    pub text: String,
    /// The Absatz numbers.
    #[serde(default)]
    pub absatz_identifiers: Vec<String>,
    /// The length of the text.
    #[serde(default)]
    pub chars: u64,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The version of the statute the text was served from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormVersion {
    /// The version id.
    #[serde(default)]
    pub id: String,
    /// The date of the last amendment.
    #[serde(default)]
    pub last_amendment_date: Option<String>,
    /// Where `last_amendment_date` comes from.
    #[serde(default)]
    pub last_amendment_date_basis: Option<String>,
    /// When the source was fetched.
    #[serde(default)]
    pub source_fetched_at: Option<String>,
    /// When the source was last seen.
    #[serde(default)]
    pub source_last_seen_at: Option<String>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The norms next to this one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormNeighbours {
    /// The previous norm.
    #[serde(default)]
    pub previous: Option<String>,
    /// The next norm.
    #[serde(default)]
    pub next: Option<String>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Links related to a norm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormLinks {
    /// The API path that finds decisions citing this norm.
    #[serde(default)]
    pub decisions_citing_url: Option<String>,
    /// The API path of this norm.
    #[serde(default)]
    pub self_url: Option<String>,
    /// Keys this crate does not model, `decisions_citing` included.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The answer of [`Norms::get`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Norm {
    /// The canonical reference, for example `§ 535 BGB`.
    #[serde(default)]
    pub reference: String,
    /// The reference as you sent it.
    #[serde(default)]
    pub requested_reference: String,
    /// The statute.
    #[serde(default)]
    pub law: Option<NormLaw>,
    /// The text of the norm.
    #[serde(default)]
    pub norm: Option<NormBody>,
    /// The Absatz or Satz qualifiers you asked for, and whether they were found.
    #[serde(default)]
    pub requested_qualifiers: Vec<Value>,
    /// The version served.
    #[serde(default)]
    pub version: Option<NormVersion>,
    /// The norms next to this one.
    #[serde(default)]
    pub neighbours: Option<NormNeighbours>,
    /// Related links.
    #[serde(default)]
    pub links: Option<NormLinks>,
    /// The API version of the answer.
    #[serde(default)]
    pub api_version: Option<String>,
    /// Link to the changelog.
    #[serde(default)]
    pub changelog_url: Option<String>,
    /// What the call cost.
    #[serde(default)]
    pub credits: Option<Credits>,
    /// Keys this crate does not model.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl_extras!(
    NormLaw,
    NormBody,
    NormVersion,
    NormNeighbours,
    NormLinks,
    Norm
);

/// The norm operations. Get it with [`Klaracase::norms`].
#[derive(Debug, Clone, Copy)]
pub struct Norms<'a> {
    client: &'a Klaracase,
}

impl Klaracase {
    /// The norm operations.
    #[must_use]
    pub fn norms(&self) -> Norms<'_> {
        Norms { client: self }
    }
}

impl Norms<'_> {
    /// Reads the current text of one norm. `GET /norms/{reference}`. The reference is a citation such
    /// as `§ 535 BGB` or `Art. 6 DSGVO`, up to 120 characters. One credit per norm per month.
    ///
    /// ```no_run
    /// use klaracase::{GetNormParams, Klaracase};
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let norm = client.norms().get("Art. 6 DSGVO", GetNormParams::default()).await?;
    /// println!("{}", norm.norm.as_ref().map(|n| n.text.as_str()).unwrap_or(""));
    /// # Ok(()) }
    /// ```
    pub async fn get(
        &self,
        reference: &str,
        params: GetNormParams,
    ) -> Result<Response<Norm>, Error> {
        let path = format!("/norms/{}", path_segment("reference", reference)?);
        self.client
            .execute(Request::get(path, params.to_query()))
            .await
    }
}
