use candid::{CandidType, Principal};
use ic_cdk::call::{Call, CallResult};
//use ic_cdk::api::call::{call_with_payment128, CallResult};
use ic_cdk::management_canister::{HttpHeader, HttpMethod, HttpRequestResult, TransformContext};
use serde::Deserialize;

/// Argument type of [super::http_request].
#[derive(CandidType, Deserialize, Debug, PartialEq, Eq, Clone, Default)]
pub struct IcHttpRequest {
    /// The requested URL.
    pub url: String,
    /// The maximal size of the response in bytes. If None, 2MiB will be the limit.
    /// This value affects the cost of the http request and it is highly recommended
    /// to set it as low as possible to avoid unnecessary extra costs.
    /// See also the [pricing section of HTTP outcalls documentation](https://internetcomputer.org/docs/current/developer-docs/integrations/http_requests/http_requests-how-it-works#pricing).
    pub max_response_bytes: Option<u64>,
    /// The method of HTTP request.
    pub method: HttpMethod,
    /// List of HTTP request headers and their corresponding values.
    pub headers: Vec<HttpHeader>,
    /// Optionally provide request body.
    pub body: Option<Vec<u8>>,
    /// Name of the transform function which is `func (transform_args) -> (http_response) query`.
    /// Set to `None` if you are using `http_request_with` or `http_request_with_cycles_with`.
    pub transform: Option<TransformContext>,
    /// If the call should go through consensus
    pub is_replicated: Option<bool>,
}

/// Make an HTTP request to a given URL and return the HTTP response, possibly after a transformation.
///
/// See [IC method `http_request`](https://internetcomputer.org/docs/current/references/ic-interface-spec/#ic-http_request).
///
/// This call requires cycles payment. The required cycles is a function of the request size and max_response_bytes.
/// Check [Gas and cycles cost](https://internetcomputer.org/docs/current/developer-docs/gas-cost) for more details.
pub async fn unreplicated_http_request(
    arg: IcHttpRequest,
    cycles: u128,
) -> CallResult<HttpRequestResult> {
    Ok(
        Call::unbounded_wait(Principal::management_canister(), "http_request")
            .with_arg(arg)
            .with_cycles(cycles)
            .await?
            .candid::<HttpRequestResult>()?,
    )
}
