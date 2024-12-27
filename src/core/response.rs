use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum Response {
    Ok,
    OkContent(String),
    Continue,
    Err(String),
}
impl Response {
    pub fn is_continue(&self) -> bool {
        match self {
            Self::Continue => true,
            _ => false,
        }
    }
}
