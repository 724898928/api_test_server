use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value};
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub(crate) struct RouteResponse {
    pub(crate) url: String,
    pub(crate) method: String,
    pub(crate) headers: Option<HashMap<String, String>>,
    pub(crate) response: Option<Value>,
}