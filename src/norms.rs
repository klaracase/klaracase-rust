//! Statute norms: `GET /norms/{reference}`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::{Klaracase, Request};
use crate::common::Credits;
use crate::encode::{Query, path_segment};
use crate::error::Error;
use crate::open::{Open, response_struct};
use crate::params::option_setters;
use crate::response::Response;
use crate::types::{Jurisdiction, Lifecycle};

/// Parameters of [`Norms::get`] and [`Norms::get_with`].
///
/// ```
/// use klaracase::{GetNormParams, Jurisdiction};
///
/// let params = GetNormParams::default().jurisdiction(Jurisdiction::Eu);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(::schemars::JsonSchema))]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct GetNormParams {
    /// `DE` or `EU`. Read only when the abbreviation is ambiguous.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jurisdiction: Option<Open<Jurisdiction>>,
}

impl GetNormParams {
    option_setters! {
        direct {
            /// Picks the jurisdiction when the abbreviation is ambiguous.
            open(Jurisdiction) jurisdiction,
        }
    }

    fn to_query(&self) -> Query {
        let mut query = Query::new();
        query.push_opt("jurisdiction", &self.jurisdiction);
        query
    }
}

response_struct! {
    /// The statute a norm belongs to.
    NormLaw {
        /// The artifact id of the statute.
        req id: String,
        /// The abbreviation, for example `BGB`.
        opt abbreviation: String,
        /// The abbreviation the source uses.
        opt source_abbreviation: String,
        /// The official name.
        opt official_name: String,
        /// The level of the law, for example `BUND`.
        opt law_level: String,
        /// The jurisdiction code.
        opt jurisdiction: Open<Jurisdiction>,
        /// Where the text comes from.
        opt source_url: String,
        /// For example `confirmed`.
        opt upstream_status: String,
        /// When the source was last seen.
        opt upstream_last_seen_at: String,
        /// Since when the source has been missing, if it is.
        opt upstream_missing_since: String,
    }
}

response_struct! {
    /// The text of one norm.
    NormBody {
        /// The number, for example `§ 535`.
        opt number: String,
        /// The title.
        opt title: String,
        /// Whether the text is current.
        opt lifecycle: Open<Lifecycle>,
        /// The full current text.
        opt text: String,
        /// The Absatz numbers.
        list absatz_identifiers: String,
        /// The length of the text in characters.
        opt chars: u64,
    }
}

response_struct! {
    /// The version of a norm the API served.
    NormVersion {
        /// The version id.
        req id: String,
        /// The date of the last amendment.
        opt last_amendment_date: String,
        /// Where the amendment date comes from.
        opt last_amendment_date_basis: String,
        /// When the source was fetched.
        opt source_fetched_at: String,
        /// When the source was last seen.
        opt source_last_seen_at: String,
    }
}

response_struct! {
    /// The norms next to this one.
    NormNeighbours {
        /// The previous norm.
        opt previous: String,
        /// The next norm.
        opt next: String,
    }
}

response_struct! {
    /// A ready-made follow-up call: an MCP tool name and its arguments.
    ToolHint {
        /// The MCP tool name.
        opt tool: String,
        /// The arguments to pass.
        opt arguments: Map<String, Value>,
    }
}

response_struct! {
    /// Related links.
    NormLinks {
        /// A ready-made search for decisions that cite this norm.
        opt decisions_citing: ToolHint,
        /// The REST path of that search.
        opt decisions_citing_url: String,
        /// The REST path of this norm.
        opt self_url: String,
    }
}

response_struct! {
    /// Whether a requested Absatz or Satz qualifier exists in the norm.
    QualifierCheck {
        /// The qualifier kind, for example `Abs.`.
        #[serde(rename = "type")]
        opt kind: String,
        /// The qualifier value, for example `1`.
        opt value: String,
        /// Whether the API looked for it.
        opt checked: bool,
        /// Whether it exists in the text.
        opt present: bool,
    }
}

response_struct! {
    /// The answer of [`Norms::get`].
    Norm {
        /// The reference as the corpus spells it.
        req reference: String,
        /// The reference you asked for.
        opt requested_reference: String,
        /// The statute.
        opt law: NormLaw,
        /// The text of the norm.
        opt norm: NormBody,
        /// The Absatz or Satz qualifiers you asked for, and whether they were found.
        list requested_qualifiers: QualifierCheck,
        /// The version served.
        opt version: NormVersion,
        /// The norms next to this one.
        opt neighbours: NormNeighbours,
        /// Related links.
        opt links: NormLinks,
        /// The API version of the answer.
        opt api_version: String,
        /// Link to the changelog.
        opt changelog_url: String,
        /// What the call cost.
        opt credits: Credits,
    }
}

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
    /// as `§ 535 BGB` or `Art. 6 DSGVO`, up to 120 characters. One credit per norm per credit period.
    ///
    /// ```no_run
    /// use klaracase::Klaracase;
    ///
    /// # async fn run(client: Klaracase) -> Result<(), klaracase::Error> {
    /// let norm = client.norms().get("Art. 6 DSGVO").await?;
    /// println!("{:?}", norm.norm.as_ref().and_then(|n| n.text.as_deref()));
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    ///
    /// Any [`Error`] the call returns.
    pub async fn get(&self, reference: &str) -> Result<Response<Norm>, Error> {
        self.get_with(reference, GetNormParams::default()).await
    }

    /// Reads a norm with ready [`GetNormParams`], for example to pick the jurisdiction when the
    /// abbreviation is ambiguous.
    ///
    /// # Errors
    ///
    /// Any [`Error`] the call returns.
    pub async fn get_with(
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
