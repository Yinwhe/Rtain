use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum Response {
    Ok(String),
    CONT,
    Err(String),
}
impl Response {
    pub fn is_cont(&self) -> bool {
        match self {
            Self::CONT => true,
            _ => false,
        }
    }
}
