use std::net::Ipv4Addr;
use tempfile::TempDir;
use tokio::net::{UnixListener, UnixStream};
use rtain::core::{Msg, CLI, Commands, PSArgs, NetCreateArgs, NetworkCommands};

/// Integration test for client-server communication
#[tokio::test]
async fn test_client_server_communication() {
    let temp_dir = TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("test.sock");
    
    // Start a mock server
    let listener = UnixListener::bind(&socket_path).unwrap();
    
    let server_handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        
        // Receive message from client
        let msg = Msg::recv_from(&mut stream).await.unwrap();
        
        match msg {
            Msg::Req(cli) => {
                match cli.command {
                    Commands::PS(_) => {
                        // Send back a mock response
                        Msg::OkContent("CONTAINER ID\tNAME\tSTATUS\n".to_string())
                            .send_to(&mut stream)
                            .await
                            .unwrap();
                    }
                    _ => {
                        Msg::Err("Unsupported command".to_string())
                            .send_to(&mut stream)
                            .await
                            .unwrap();
                    }
                }
            }
            _ => {
                Msg::Err("Invalid message type".to_string())
                    .send_to(&mut stream)
                    .await
                    .unwrap();
            }
        }
    });
    
    // Connect as client
    let mut client_stream = UnixStream::connect(&socket_path).await.unwrap();
    
    // Send a PS command
    let cli = CLI {
        command: Commands::PS(PSArgs { all: false }),
    };
    
    Msg::Req(cli).send_to(&mut client_stream).await.unwrap();
    
    // Receive response
    let response = Msg::recv_from(&mut client_stream).await.unwrap();
    
    match response {
        Msg::OkContent(content) => {
            assert!(content.contains("CONTAINER ID"));
        }
        _ => panic!("Expected OkContent response"),
    }
    
    server_handle.await.unwrap();
}

/// Test network creation workflow
#[tokio::test]
async fn test_network_creation_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("test_network.sock");
    
    let listener = UnixListener::bind(&socket_path).unwrap();
    
    let server_handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        
        let msg = Msg::recv_from(&mut stream).await.unwrap();
        
        match msg {
            Msg::Req(cli) => {
                match cli.command {
                    Commands::Network(NetworkCommands::Create(args)) => {
                        // Validate network creation parameters
                        assert_eq!(args.name, "test-network");
                        assert_eq!(args.subnet, "192.168.100.0/24");
                        assert_eq!(args.driver, "bridge");
                        
                        Msg::OkContent(format!("Network {} created", args.name))
                            .send_to(&mut stream)
                            .await
                            .unwrap();
                    }
                    _ => {
                        Msg::Err("Unexpected command".to_string())
                            .send_to(&mut stream)
                            .await
                            .unwrap();
                    }
                }
            }
            _ => {
                Msg::Err("Invalid message".to_string())
                    .send_to(&mut stream)
                    .await
                    .unwrap();
            }
        }
    });
    
    let mut client_stream = UnixStream::connect(&socket_path).await.unwrap();
    
    let network_cmd = CLI {
        command: Commands::Network(NetworkCommands::Create(NetCreateArgs {
            name: "test-network".to_string(),
            subnet: "192.168.100.0/24".to_string(),
            driver: "bridge".to_string(),
        })),
    };
    
    Msg::Req(network_cmd).send_to(&mut client_stream).await.unwrap();
    
    let response = Msg::recv_from(&mut client_stream).await.unwrap();
    
    match response {
        Msg::OkContent(content) => {
            assert!(content.contains("Network test-network created"));
        }
        _ => panic!("Expected success response"),
    }
    
    server_handle.await.unwrap();
}

/// Test error handling in client-server communication
#[tokio::test]
async fn test_error_handling() {
    let temp_dir = TempDir::new().unwrap();
    let socket_path = temp_dir.path().join("test_error.sock");
    
    let listener = UnixListener::bind(&socket_path).unwrap();
    
    let server_handle = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        
        // Always send an error response
        Msg::Err("Test error message".to_string())
            .send_to(&mut stream)
            .await
            .unwrap();
    });
    
    let mut client_stream = UnixStream::connect(&socket_path).await.unwrap();
    
    let cli = CLI {
        command: Commands::PS(PSArgs { all: false }),
    };
    
    Msg::Req(cli).send_to(&mut client_stream).await.unwrap();
    
    let response = Msg::recv_from(&mut client_stream).await.unwrap();
    
    match response {
        Msg::Err(error) => {
            assert_eq!(error, "Test error message");
        }
        _ => panic!("Expected error response"),
    }
    
    server_handle.await.unwrap();
}