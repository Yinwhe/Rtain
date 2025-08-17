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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test::io::Builder;

    #[test]
    fn test_msg_get_req() {
        let cli = CLI {
            command: crate::core::Commands::PS(crate::core::PSArgs { all: false }),
        };

        let msg = Msg::Req(cli.clone());
        assert!(msg.get_req().is_some());

        let ok_msg = Msg::Ok;
        assert!(ok_msg.get_req().is_none());

        let err_msg = Msg::Err("test error".to_string());
        assert!(err_msg.get_req().is_none());
    }

    #[tokio::test]
    async fn test_msg_serialization() {
        let cli = CLI {
            command: crate::core::Commands::PS(crate::core::PSArgs { all: false }),
        };

        // Test different message types
        let messages = vec![
            Msg::Req(cli),
            Msg::Ok,
            Msg::OkContent("test content".to_string()),
            Msg::Continue,
            Msg::Err("test error".to_string()),
        ];

        for original_msg in messages {
            // Serialize
            let serialized = bincode::serialize(&original_msg).unwrap();

            // Deserialize
            let deserialized: Msg = bincode::deserialize(&serialized).unwrap();

            // Compare (would need PartialEq trait for exact comparison)
            match (&original_msg, &deserialized) {
                (Msg::Ok, Msg::Ok) => {}
                (Msg::Continue, Msg::Continue) => {}
                (Msg::Err(e1), Msg::Err(e2)) => assert_eq!(e1, e2),
                (Msg::OkContent(c1), Msg::OkContent(c2)) => assert_eq!(c1, c2),
                _ => {}
            }
        }
    }

    #[tokio::test]
    async fn test_msg_send_recv() {
        let test_msg = Msg::OkContent("test message".to_string());
        let serialized_msg = bincode::serialize(&test_msg).unwrap();
        let msg_len = serialized_msg.len() as u64;
        let len_bytes = msg_len.to_le_bytes();

        // Create a mock stream
        let mut mock_stream = Builder::new()
            .write(&len_bytes) // length header
            .write(&serialized_msg) // message data
            .read(&len_bytes) // length header for reading
            .read(&serialized_msg) // message data for reading
            .build();

        // Test send
        test_msg.send_to(&mut mock_stream).await.unwrap();

        // Test receive
        let received_msg = Msg::recv_from(&mut mock_stream).await.unwrap();

        match received_msg {
            Msg::OkContent(content) => assert_eq!(content, "test message"),
            _ => panic!("Expected OkContent message"),
        }
    }
}
