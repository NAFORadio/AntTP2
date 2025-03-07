use std::env::args;
use std::net::SocketAddr;
use anyhow::{anyhow, Result};
use log::{info, warn};

/// Configuration for the AntTP server.
///
/// This struct holds the configuration parameters for the AntTP server,
/// including the socket address to bind to, the directory for static files,
/// and the wallet private key for Autonomi Network operations.
#[derive(Clone)]
pub struct AntTpConfig {
    /// The socket address to bind the server to.
    pub bind_socket_addr: SocketAddr,
    
    /// The directory to serve static files from.
    pub static_dir: String,
    
    /// The private key for the Autonomi Network wallet.
    pub wallet_private_key: String,
}

impl AntTpConfig {
    /// Reads configuration from command-line arguments.
    ///
    /// The arguments are expected in the following order:
    /// 1. Bind socket address (optional, default: "0.0.0.0:8080")
    /// 2. Static file directory (optional, default: "static")
    /// 3. Wallet private key (optional, default: "")
    ///
    /// # Returns
    /// A new AntTpConfig instance with the parsed configuration.
    ///
    /// # Panics
    /// This function will panic if the bind socket address cannot be parsed.
    /// In a production environment, this should be changed to return a Result.
    pub fn read_args() -> AntTpConfig {
        // Skip executable name from args
        let mut args_received = args();
        args_received.next();

        // Read the network contact socket address from first arg passed
        let bind_addr = args_received.next().unwrap_or_else(|| {
            info!("No bind address provided, using default");
            "0.0.0.0:8080".to_string()
        });
        
        // Parse the bind socket address
        let bind_socket_addr: SocketAddr = match bind_addr.parse() {
            Ok(addr) => addr,
            Err(err) => {
                // In a production environment, this should return a Result instead of panicking
                let error_msg = format!("Invalid bind socket address '{}': {}", bind_addr, err);
                warn!("{}", error_msg);
                panic!("{}", error_msg);
            }
        };
        info!("Bind address [{}]", bind_socket_addr);

        // Read the static file directory from second arg passed
        let static_dir = args_received.next().unwrap_or_else(|| {
            info!("No static directory provided, using default");
            "static".to_string()
        });
        info!("Static file directory: [{}]", static_dir);

        // Read the wallet private key from third arg passed
        let wallet_private_key = args_received.next().unwrap_or_else(|| {
            info!("No wallet private key provided, using default (empty)");
            "".to_string()
        });
        
        // Don't log the actual private key for security reasons
        if !wallet_private_key.is_empty() {
            info!("Wallet private key provided: [*****]");
        } else {
            info!("No wallet private key provided");
        }

        AntTpConfig {
            bind_socket_addr,
            static_dir,
            wallet_private_key,
        }
    }
    
    /// Creates a new AntTpConfig with the specified parameters.
    ///
    /// # Parameters
    /// * `bind_socket_addr` - The socket address to bind the server to
    /// * `static_dir` - The directory to serve static files from
    /// * `wallet_private_key` - The private key for the Autonomi Network wallet
    ///
    /// # Returns
    /// A new AntTpConfig instance.
    pub fn new(bind_socket_addr: SocketAddr, static_dir: String, wallet_private_key: String) -> Self {
        AntTpConfig {
            bind_socket_addr,
            static_dir,
            wallet_private_key,
        }
    }
}