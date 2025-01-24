use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

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
        let msg = self.prepare();
        stream.write_all(&msg).await
    }

    pub async fn recv_from(
        stream: &mut (impl AsyncReadExt + std::marker::Unpin),
    ) -> tokio::io::Result<Self> {
        let mut bufreader = BufReader::new(stream);
        let mut buf = String::new();

        bufreader.read_line(&mut buf).await?;

        bincode::deserialize(buf.as_bytes())
            .map_err(|e| tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, e))
    }

    pub fn get_req(self) -> Option<CLI> {
        match self {
            Msg::Req(cli) => Some(cli),
            _ => None,
        }
    }

    pub fn get_okcont(self) -> Option<String> {
        match self {
            Msg::OkContent(cont) => Some(cont),
            _ => None,
        }
    }

    #[inline]
    fn prepare(self) -> Vec<u8> {
        let mut msg = bincode::serialize(&self).unwrap();
        msg.push(b'\n');

        msg
    }
}
