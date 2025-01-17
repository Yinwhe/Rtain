use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[derive(Serialize, Deserialize, Debug)]
pub enum Response {
    Ok,
    OkContent(String),
    Continue,
    Err(String),
}

impl Response {
    pub async fn send_to(
        self,
        stream: &mut (impl AsyncWriteExt + std::marker::Unpin),
    ) -> tokio::io::Result<()> {
        let msg = self.prepare();
        stream.write_all(msg.as_bytes()).await
    }

    pub async fn recv_from(
        stream: &mut (impl AsyncReadExt + std::marker::Unpin),
    ) -> tokio::io::Result<Self> {
        let mut bufreader = BufReader::new(stream);
        let mut buf = String::new();

        bufreader.read_line(&mut buf).await?;

        serde_json::from_str(&buf)
            .map_err(|e| tokio::io::Error::new(tokio::io::ErrorKind::InvalidData, e))
    }

    #[inline]
    fn prepare(self) -> String {
        let mut msg = serde_json::to_string(&self).unwrap();
        msg.push('\n');

        msg
    }
}
