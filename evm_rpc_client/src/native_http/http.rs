use crate::evm_rpc_types::{HttpOutcallError, ProviderError, RpcError, RpcResult};

use crate::native_http::{
    accounting::{get_cost_with_collateral, get_http_request_cost},
    constants::{CONTENT_TYPE_HEADER_LOWERCASE, CONTENT_TYPE_VALUE},
    http_request::{unreplicated_http_request, IcHttpRequest},
    util::canonicalize_json,
};
use crate::{RejectionCode, RpcApi};
use ic_cdk::management_canister::{
    transform_context_from_query, HttpHeader, HttpMethod, HttpRequestResult, TransformArgs,
};

use ic_cdk::query;
use num_traits::ToPrimitive;

pub async fn json_rpc_request(
    service: RpcApi,
    json_rpc_payload: &str,
    max_response_bytes: u64,
    cycles_available: u128,
) -> RpcResult<HttpRequestResult> {
    let cycles_cost = get_http_request_cost(json_rpc_payload.len() as u64, max_response_bytes);
    let api = service.clone();
    let mut request_headers = api.headers.unwrap_or_default();
    if !request_headers
        .iter()
        .any(|header| header.name.to_lowercase() == CONTENT_TYPE_HEADER_LOWERCASE)
    {
        request_headers.push(HttpHeader {
            name: CONTENT_TYPE_HEADER_LOWERCASE.to_string(),
            value: CONTENT_TYPE_VALUE.to_string(),
        });
    }
    let request = IcHttpRequest {
        url: api.url,
        max_response_bytes: Some(max_response_bytes),
        method: HttpMethod::POST,
        headers: request_headers,
        body: Some(json_rpc_payload.as_bytes().to_vec()),
        transform: Some(transform_context_from_query(
            "__transform_json_rpc".to_string(),
            vec![],
        )),
        is_replicated: Some(false),
    };
    http_request(request, cycles_cost, cycles_available).await
}

pub async fn http_request(
    request: IcHttpRequest,
    cycles_cost: u128,
    cycles_available: u128,
) -> RpcResult<HttpRequestResult> {
    let cycles_cost_with_collateral = get_cost_with_collateral(cycles_cost);
    if cycles_available < cycles_cost_with_collateral {
        return Err(ProviderError::TooFewCycles {
            expected: cycles_cost_with_collateral,
            received: cycles_available,
        }
        .into());
    }
    match unreplicated_http_request(request, cycles_cost).await {
        Ok(response) => Ok(response),
        Err(err) => {
            let (code, message) = match err {
                ic_cdk::call::Error::InsufficientLiquidCycleBalance(error) => {
                    (RejectionCode::CanisterError, error.to_string())
                }
                ic_cdk::call::Error::CallPerformFailed(error) => {
                    (RejectionCode::CanisterReject, error.to_string())
                }
                ic_cdk::call::Error::CallRejected(error) => {
                    (RejectionCode::CanisterReject, error.to_string())
                }
                ic_cdk::call::Error::CandidDecodeFailed(error) => {
                    (RejectionCode::CanisterError, error.to_string())
                }
            };

            Err(HttpOutcallError::IcError { code, message }.into())
        }
    }
}

pub fn transform_http_request(args: TransformArgs) -> HttpRequestResult {
    HttpRequestResult {
        status: args.response.status,
        body: canonicalize_json(&args.response.body).unwrap_or(args.response.body),
        // Remove headers (which may contain a timestamp) for consensus
        headers: vec![],
    }
}

pub fn get_http_response_status(status: candid::Nat) -> u16 {
    status.0.to_u16().unwrap_or(u16::MAX)
}

pub fn get_http_response_body(response: HttpRequestResult) -> Result<String, RpcError> {
    String::from_utf8(response.body).map_err(|e| {
        HttpOutcallError::InvalidHttpJsonRpcResponse {
            status: get_http_response_status(response.status),
            body: "".to_string(),
            parsing_error: Some(format!("{e}")),
        }
        .into()
    })
}

#[query(name = "__transform_json_rpc", hidden = true)]
fn transform(args: TransformArgs) -> HttpRequestResult {
    transform_http_request(args)
}
