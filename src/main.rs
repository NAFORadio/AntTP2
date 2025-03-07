mod anttp_config;
mod caching_client;
mod app_config;
mod archive_helper;
mod xor_helper;
mod archive_client;
mod file_client;

use actix_web::{web, App, HttpServer, Responder, middleware::Logger, HttpRequest, Error};
use actix_files::Files;
use log::{info, error, debug, warn};
use ::autonomi::Client;
use actix_multipart::Multipart;
use actix_web::dev::{ConnectionInfo};
use actix_web::web::Data;
use ant_evm::EvmNetwork::ArbitrumOne;
use ant_evm::EvmWallet;
use awc::Client as AwcClient;
use crate::caching_client::CachingClient;
use crate::anttp_config::AntTpConfig;
use crate::archive_client::ArchiveClient;
use crate::file_client::FileClient;
use crate::xor_helper::XorHelper;

/// Default logging configuration for the application.
/// 
/// This sets the log levels for various components:
/// - anttp: info level
/// - ant_api: warn level
/// - ant_client: warn level
/// - ant_networking: off (no logging)
/// - ant_bootstrap: error level
const DEFAULT_LOGGING: &'static str = "info,anttp=info,ant_api=warn,ant_client=warn,ant_networking=off,ant_bootstrap=error";

/// Main entry point for the AntTP application.
///
/// This function initializes the Autonomi client, sets up the HTTP server with
/// the appropriate routes, and starts listening for incoming connections.
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging from RUST_LOG env var with info as default
    env_logger::Builder::from_env(env_logger::Env::default()
        .default_filter_or(DEFAULT_LOGGING))
        .init();

    // Read configuration from command-line arguments
    let app_config = AntTpConfig::read_args();
    let bind_socket_addr = app_config.bind_socket_addr;
    let wallet_private_key = app_config.wallet_private_key.clone();

    // Initialize Autonomi network connection
    info!("Connecting to Autonomi Network...");
    let autonomi_client = match Client::init().await {
        Ok(client) => {
            info!("Successfully connected to Autonomi Network");
            client
        },
        Err(err) => {
            error!("Failed to connect to Autonomi Network: {}", err);
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                format!("Failed to connect to Autonomi Network: {}", err)
            ));
        }
    };

    // Initialize EVM wallet
    let evm_wallet = if !wallet_private_key.is_empty() {
        match EvmWallet::new_from_private_key(ArbitrumOne, wallet_private_key.as_str()) {
            Ok(wallet) => {
                info!("Successfully initialized EVM wallet from private key");
                wallet
            },
            Err(err) => {
                error!("Failed to instantiate EvmWallet from private key: {}", err);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Failed to instantiate EvmWallet: {}", err)
                ));
            }
        }
    } else {
        info!("No private key provided, creating random wallet");
        EvmWallet::new_with_random_wallet(ArbitrumOne)
    };

    info!("Starting HTTP server on {}", bind_socket_addr);

    // Create and start the HTTP server
    HttpServer::new(move || {
        let logger = Logger::default();

        App::new()
            .wrap(logger)
            .service(Files::new("/static", app_config.static_dir.clone()))
            .route("/api/v1/archive", web::post().to(post_public_archive))
            .route("/{path:.*}", web::get().to(get_public_data))
            .app_data(Data::new(app_config.clone()))
            .app_data(Data::new(autonomi_client.clone()))
            .app_data(Data::new(AwcClient::default()))
            .app_data(Data::new(evm_wallet.clone()))
    })
        .bind(bind_socket_addr)?
        .run()
        .await
}

/// Handles POST requests to create a new public archive.
///
/// This endpoint accepts multipart form data and creates a new archive in the
/// Autonomi network.
///
/// # Parameters
/// * `payload` - The multipart form data containing the archive contents
/// * `autonomi_client_data` - The Autonomi client
/// * `evm_wallet_data` - The EVM wallet
/// * `conn` - Connection information
///
/// # Returns
/// An HTTP response indicating the result of the archive creation
async fn post_public_archive(
    payload: Multipart,
    autonomi_client_data: Data<Client>,
    evm_wallet_data: Data<EvmWallet>,
    conn: ConnectionInfo)
-> impl Responder {
    let autonomi_client = autonomi_client_data.get_ref().clone();
    let caching_autonomi_client = CachingClient::new(autonomi_client.clone());
    let evm_wallet = evm_wallet_data.get_ref().clone();
    let xor_helper = XorHelper::new();
    let file_client = FileClient::new(autonomi_client.clone(), xor_helper.clone(), conn);

    let archive_client = ArchiveClient::new(autonomi_client, caching_autonomi_client, file_client, xor_helper.clone());

    info!("Creating new archive from multipart POST");
    match archive_client.post_data(payload, evm_wallet).await {
        Ok(response) => {
            info!("Successfully created archive");
            response
        },
        Err(err) => {
            error!("Failed to create archive: {}", err);
            err.into()
        }
    }
}

/// Handles GET requests to retrieve data from the Autonomi network.
///
/// This endpoint retrieves data from either an archive or a direct XOR address.
///
/// # Parameters
/// * `request` - The HTTP request
/// * `path` - The path to the requested resource
/// * `autonomi_client_data` - The Autonomi client
/// * `conn` - Connection information
///
/// # Returns
/// An HTTP response containing the requested data
async fn get_public_data(
    request: HttpRequest,
    path: web::Path<String>,
    autonomi_client_data: Data<Client>,
    conn: ConnectionInfo
) -> impl Responder {
    // Parse the path into parts
    let path_str = path.into_inner();
    debug!("Received request for path: {}", path_str);
    let path_parts = get_path_parts(&conn.host(), &path_str);
    
    let xor_helper = XorHelper::new();
    let (archive_addr, archive_file_name) = xor_helper.assign_path_parts(path_parts.clone());

    let autonomi_client = autonomi_client_data.get_ref().clone();
    let caching_autonomi_client = CachingClient::new(autonomi_client.clone());
    
    // Resolve the archive or file
    let (is_found, archive, is_archive, xor_addr) = xor_helper
        .resolve_archive_or_file(&caching_autonomi_client, &archive_addr, &archive_file_name)
        .await;
    
    let file_client = FileClient::new(autonomi_client.clone(), xor_helper.clone(), conn);
    
    // Handle the request based on whether it's an archive or a direct file
    if !is_archive {
        info!("Retrieving file from XOR [{:x}]", xor_addr);
        file_client.get_data(path_parts, request, xor_addr, is_found).await
    } else {
        info!("Retrieving file from archive [{:x}]", xor_addr);
        let archive_client = ArchiveClient::new(autonomi_client, caching_autonomi_client.clone(), file_client, xor_helper);
        archive_client.get_data(archive, xor_addr, request, path_parts).await
    }
}

/// Parses a hostname and path into a vector of path parts.
///
/// This function handles different formats of hostnames and paths:
/// - If the hostname ends with ".autonomi", it extracts the subdomain as the first path part
/// - If the hostname is a XOR address, it uses the hostname as the first path part
/// - Otherwise, it just splits the path by "/"
///
/// # Parameters
/// * `hostname` - The hostname from the request
/// * `path` - The path from the request
///
/// # Returns
/// A vector of path parts
fn get_path_parts(hostname: &str, path: &str) -> Vec<String> {
    let xor_helper = XorHelper::new();
    
    // Handle hostname ending with ".autonomi"
    if hostname.ends_with(".autonomi") {
        debug!("Handling .autonomi hostname: {}", hostname);
        let mut subdomain_parts = hostname.split(".")
            .map(str::to_string)
            .collect::<Vec<String>>();
        subdomain_parts.pop(); // discard 'autonomi' suffix
        
        let path_parts = path.split("/")
            .filter(|s| !s.is_empty()) // Filter out empty strings
            .map(str::to_string)
            .collect::<Vec<String>>();
        
        subdomain_parts.extend(path_parts);
        subdomain_parts
    } 
    // Handle hostname as XOR address
    else if xor_helper.is_xor(&hostname.to_string()) {
        debug!("Handling XOR hostname: {}", hostname);
        let mut parts = Vec::new();
        parts.push(hostname.to_string());
        
        let path_parts = path.split("/")
            .filter(|s| !s.is_empty()) // Filter out empty strings
            .map(str::to_string)
            .collect::<Vec<String>>();
        
        parts.extend(path_parts);
        parts
    } 
    // Handle regular path
    else {
        debug!("Handling regular path: {}", path);
        path.split("/")
            .filter(|s| !s.is_empty()) // Filter out empty strings
            .map(str::to_string)
            .collect::<Vec<String>>()
    }
}