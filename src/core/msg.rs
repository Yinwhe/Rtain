use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::CLI;

#[derive(Serialize, Deserialize, Debug)]
pub enum Msg {
    /// Client Request
    Req(CLI),

    /// Server Response
    Ok,
    OkContent(String),
    Continue,
    Err(String),
}

impl Msg {
    pub async fn send_to(
        self,
        stream: &mut (impl AsyncWriteExt + std::marker::Unpin),
    ) -> tokio::io::Result<()> {
        let msg = bincode::serialize(&self).unwrap();
        let len = (msg.len() as u64).to_le_bytes().to_vec();

        stream.write_all(&len).await?;
        stream.write_all(&msg).await
    }

    pub async fn recv_from(
        stream: &mut (impl AsyncReadExt + std::marker::Unpin),
    ) -> tokio::io::Result<Self> {
        let mut len_buf = [0; 8];
        stream.read_exact(&mut len_buf).await?;

        let buf_len = u64::from_le_bytes(len_buf);
        let mut buf = vec![0u8; buf_len as usize];
        stream.read_exact(&mut buf).await?;

        bincode::deserialize(&buf)
            .map_err(|e| tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, e))
    }

    pub fn get_req(self) -> Option<CLI> {
        match self {
            Msg::Req(cli) => Some(cli),
            _ => None,
        }
    }
}
