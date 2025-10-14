// src/icrc21.rs
// This file defines the ICRC-21 types based on the provided DID specification.

use candid::{CandidType, Deserialize, Nat};

#[derive(CandidType, Deserialize, Clone)]
pub struct ConsentMessageMetadata {
    pub language: String,
    pub utc_offset_minutes: Option<i16>,
}

#[derive(CandidType, Deserialize, Clone)]
pub enum DeviceSpec {
    GenericDisplay,
    FieldsDisplay,
}

#[derive(CandidType, Deserialize, Clone)]
pub struct ConsentMessageSpec {
    pub metadata: ConsentMessageMetadata,
    pub device_spec: Option<DeviceSpec>,
}

#[derive(CandidType, Deserialize, Clone)]
pub struct ConsentMessageRequest {
    pub method: String,
    pub arg: Vec<u8>,
    pub user_preferences: ConsentMessageSpec,
}

#[derive(CandidType, Deserialize, Clone)]
pub struct TokenAmount {
    pub decimals: u8,
    pub amount: u64,
    pub symbol: String,
}

#[derive(CandidType, Deserialize, Clone)]
pub struct TimestampSeconds {
    pub amount: u64,
}

#[derive(CandidType, Deserialize, Clone)]
pub struct DurationSeconds {
    pub amount: u64,
}

#[derive(CandidType, Deserialize, Clone)]
pub struct TextValue {
    pub content: String,
}

#[derive(CandidType, Deserialize, Clone)]
pub enum Value {
    TokenAmount(TokenAmount),
    TimestampSeconds(TimestampSeconds),
    DurationSeconds(DurationSeconds),
    Text(TextValue),
}

#[derive(CandidType, Deserialize, Clone)]
pub enum ConsentMessage {
    GenericDisplayMessage(String),
    FieldsDisplayMessage {
        intent: String,
        fields: Vec<(String, Value)>,
    },
}

#[derive(CandidType, Deserialize, Clone)]
pub struct ConsentInfo {
    pub consent_message: ConsentMessage,
    pub metadata: ConsentMessageMetadata,
}

#[derive(CandidType, Deserialize, Clone)]
pub struct ErrorInfo {
    pub description: String,
}

#[derive(CandidType, Deserialize, Clone)]
pub enum Error {
    UnsupportedCanisterCall(ErrorInfo),
    ConsentMessageUnavailable(ErrorInfo),
    InsufficientPayment(ErrorInfo),
    GenericError {
        error_code: Nat,
        description: String,
    },
}

pub type ConsentMessageResponse = Result<ConsentInfo, Error>;
