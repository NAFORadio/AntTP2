use std::path::PathBuf;
use actix_http::header::HeaderMap;
use actix_web::{Error, HttpRequest};
use actix_web::error::ErrorInternalServerError;
use autonomi::data::{DataAddress};
use autonomi::files::PublicArchive;
use chrono::DateTime;
use log::{debug, info, warn};
use xor_name::XorName;
use crate::xor_helper::XorHelper;

/// Helper for working with Autonomi archives.
///
/// This struct provides utility functions for interacting with PublicArchive objects,
/// including listing files, resolving data addresses, and handling archive-related requests.
#[derive(Clone)]
pub struct ArchiveHelper {
    /// The PublicArchive being managed by this helper.
    archive: PublicArchive
}

/// Information about an archive request.
///
/// This struct contains information about a request to an archive,
/// including the path, resolved XOR address, action to take, and data state.
#[derive(Clone)]
pub struct ArchiveInfo {
    /// The path string for the request.
    pub path_string: String,
    
    /// The resolved XOR address for the requested resource.
    pub resolved_xor_addr: XorName,
    
    /// The action to take for this request (e.g., serve data, list files).
    pub action: ArchiveAction,
    
    /// The state of the data (modified or not modified).
    pub state: DataState,
}

/// Actions that can be taken for an archive request.
#[derive(Clone, PartialEq, Eq)]
pub enum ArchiveAction {
    /// Serve data from the archive.
    Data,
    
    /// List the contents of the archive.
    Listing,
    
    /// Redirect to another location.
    Redirect,
    
    /// Resource not found in the archive.
    NotFound
}

/// State of data for caching purposes.
#[derive(Clone, PartialEq, Eq)]
pub enum DataState {
    /// Data has been modified since the client's last request.
    Modified,
    
    /// Data has not been modified since the client's last request.
    NotModified
}

impl ArchiveInfo {
    /// Creates a new ArchiveInfo instance.
    ///
    /// # Parameters
    /// * `path_string` - The path string for the request
    /// * `resolved_xor_addr` - The resolved XOR address for the requested resource
    /// * `action` - The action to take for this request
    /// * `state` - The state of the data
    ///
    /// # Returns
    /// A new ArchiveInfo instance.
    pub fn new(path_string: String, resolved_xor_addr: XorName, action: ArchiveAction, state: DataState) -> ArchiveInfo {
        ArchiveInfo { path_string, resolved_xor_addr, action, state }
    }
}

impl ArchiveHelper {
    /// Creates a new ArchiveHelper instance.
    ///
    /// # Parameters
    /// * `public` - The PublicArchive to manage
    ///
    /// # Returns
    /// A new ArchiveHelper instance.
    pub fn new(public: PublicArchive) -> ArchiveHelper {
        ArchiveHelper { archive: public }
    }
    
    /// Lists the files in the archive.
    ///
    /// This function returns a string representation of the files in the archive,
    /// either as HTML or JSON depending on the Accept header in the request.
    ///
    /// # Parameters
    /// * `header_map` - The HTTP request headers
    ///
    /// # Returns
    /// A string representation of the files in the archive.
    pub fn list_files(&self, header_map: &HeaderMap) -> String {
        // Check if the client accepts JSON
        let accepts_json = header_map.get("Accept")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.contains("json"))
            .unwrap_or(false);
            
        if accepts_json {
            self.list_files_json()
        } else {
            self.list_files_html()
        }
    }

    /// Lists the files in the archive as HTML.
    ///
    /// # Returns
    /// An HTML string representation of the files in the archive.
    fn list_files_html(&self) -> String {
        let mut output = "<html><body><h1>Archive Contents</h1><ul>".to_string();

        // Iterate through the archive entries
        for key in self.archive.map().keys() {
            if let Some(path_str) = key.to_str() {
                let filepath = path_str.trim_start_matches("./").to_string();
                output.push_str(&format!("<li><a href=\"{}\">{}</a></li>\n", filepath, filepath));
            } else {
                warn!("Found non-UTF8 path in archive");
            }
        }
        
        output.push_str("</ul></body></html>");
        output
    }

    /// Lists the files in the archive as JSON.
    ///
    /// # Returns
    /// A JSON string representation of the files in the archive.
    fn list_files_json(&self) -> String {
        let mut output = "[\n".to_string();

        let mut i = 1;
        let count = self.archive.map().keys().len();
        
        // Iterate through the archive entries
        for key in self.archive.map().keys() {
            if let Some(path_str) = key.to_str() {
                if let Some((metadata, _)) = self.archive.map().get(key) {
                    // Convert the modified timestamp to an ISO 8601 string
                    let mtime_datetime = match DateTime::from_timestamp_millis(metadata.modified as i64 * 1000) {
                        Some(dt) => dt,
                        None => {
                            warn!("Invalid timestamp for file: {}", path_str);
                            continue;
                        }
                    };
                    
                    let mtime_iso = mtime_datetime.format("%+");
                    let filepath = path_str.trim_start_matches("./").to_string();
                    
                    output.push_str("{");
                    output.push_str(&format!(
                        "\"name\": \"{}\", \"type\": \"file\", \"mtime\": \"{}\", \"size\": \"{}\"", 
                        filepath, mtime_iso, metadata.size
                    ));
                    output.push_str("}");
                    
                    if i < count {
                        output.push_str(",");
                    }
                    
                    output.push_str("\n");
                    i += 1;
                }
            } else {
                warn!("Found non-UTF8 path in archive");
            }
        }
        
        output.push_str("]");
        output
    }

    /// Resolves a data address from a path.
    ///
    /// This function looks up a path in the archive and returns the corresponding
    /// data address if found.
    ///
    /// # Parameters
    /// * `path_parts` - The parts of the path to resolve
    ///
    /// # Returns
    /// A Result containing the data address if found, or an error if not found.
    pub fn resolve_data_addr(&self, path_parts: Vec<String>) -> Result<DataAddress, Error> {
        // Log all archive entries for debugging
        self.archive.iter().for_each(|(path_buf, data_address, _)| 
            debug!("archive entry: [{}] at [{:x}]", path_buf.display(), data_address.xorname())
        );

        // Join the path parts (excluding the first part, which is the archive address)
        let path_parts_string = path_parts[1..].join("/");
        debug!("Looking for path: {}", path_parts_string);
        
        // Search for the path in the archive
        for key in self.archive.map().keys() {
            if let Some(key_str) = key.to_str() {
                let normalized_key = key_str.trim_start_matches("./");
                
                if normalized_key.ends_with(&path_parts_string) {
                    if let Some((data_addr, _)) = self.archive.map().get(key) {
                        info!("Found item [{}] in archive at [{:x}]", path_parts_string, data_addr.xorname());
                        return Ok(data_addr.clone());
                    }
                }
            }
        }
        
        // Path not found in the archive
        warn!("Failed to find item [{}] in archive", path_parts_string);
        Err(ErrorInternalServerError(format!("Failed to find item [{}] in archive", path_parts_string)))
    }

    /// Gets the index file for a directory.
    ///
    /// This function looks for an index file (e.g., index.html) in the specified directory.
    ///
    /// # Parameters
    /// * `request_path` - The path of the request
    /// * `resolved_filename_string` - The resolved filename
    ///
    /// # Returns
    /// A tuple containing the path string and XOR address of the index file, or empty values if not found.
    pub fn get_index(&self, request_path: String, resolved_filename_string: String) -> (String, XorName) {
        debug!("Looking for index file in directory: {}", resolved_filename_string);
        
        // Search for a file ending with the resolved filename
        for key in self.archive.map().keys() {
            if let Some(key_str) = key.to_str() {
                if key.ends_with(&resolved_filename_string) {
                    let path_string = request_path + key_str;
                    
                    if let Some((data_address, _)) = self.archive.map().get(key) {
                        info!("Found index file: {} at {:x}", path_string, data_address.xorname());
                        return (path_string, *data_address.xorname());
                    }
                }
            }
        }
        
        // Index file not found
        debug!("No index file found for: {}", resolved_filename_string);
        (String::new(), XorName::default())
    }

    /// Resolves information about an archive request.
    ///
    /// This function determines what action to take for a request to an archive,
    /// such as serving data, listing files, or redirecting.
    ///
    /// # Parameters
    /// * `path_parts` - The parts of the path
    /// * `request` - The HTTP request
    /// * `resolved_relative_path_route` - The resolved relative path route
    /// * `has_route_map` - Whether a route map is being used
    ///
    /// # Returns
    /// An ArchiveInfo instance with information about the request.
    pub fn resolve_archive_info(&self, path_parts: Vec<String>, request: HttpRequest, resolved_relative_path_route: String, has_route_map: bool) -> ArchiveInfo {
        let request_path = request.path();
        let xor_helper = XorHelper::new();
        
        // Check if the request needs to be redirected
        if self.has_moved_permanently(request_path, &resolved_relative_path_route) {
            debug!("Request has moved permanently, redirecting");
            return ArchiveInfo::new(
                resolved_relative_path_route, 
                XorName::default(), 
                ArchiveAction::Redirect, 
                DataState::Modified
            );
        }
        
        // Handle route map case
        if has_route_map {
            debug!("Using route map to resolve index file");
            let (resolved_path, resolved_xor_addr) = self.get_index(
                request_path.to_string(), 
                resolved_relative_path_route
            );
            
            return ArchiveInfo::new(
                resolved_path, 
                resolved_xor_addr, 
                ArchiveAction::Data, 
                xor_helper.get_data_state(request.headers(), &resolved_xor_addr)
            );
        }
        
        // Handle specific file request
        if !resolved_relative_path_route.is_empty() {
            debug!("Resolving specific file: {}", resolved_relative_path_route);
            
            match self.resolve_data_addr(path_parts.clone()) {
                Ok(resolved_data_address) => {
                    let path_buf = &PathBuf::from(resolved_relative_path_route.clone());
                    info!(
                        "Resolved path [{}], path_buf [{}] to xor address [{}]", 
                        resolved_relative_path_route, 
                        path_buf.display(), 
                        format!("{:x}", resolved_data_address.xorname())
                    );
                    
                    return ArchiveInfo::new(
                        resolved_relative_path_route, 
                        *resolved_data_address.xorname(), 
                        ArchiveAction::Data, 
                        xor_helper.get_data_state(request.headers(), resolved_data_address.xorname())
                    );
                }
                Err(err) => {
                    warn!("Failed to resolve data address: {}", err);
                    return ArchiveInfo::new(
                        resolved_relative_path_route, 
                        XorName::default(), 
                        ArchiveAction::NotFound, 
                        DataState::Modified
                    );
                }
            }
        }
        
        // Default case: list files in the archive
        info!("Listing files in archive");
        ArchiveInfo::new(
            resolved_relative_path_route, 
            XorName::default(), 
            ArchiveAction::Listing, 
            DataState::Modified
        )
    }

    /// Checks if a request has moved permanently.
    ///
    /// This function determines if a request should be redirected to a permanent location.
    ///
    /// # Parameters
    /// * `request_path` - The path of the request
    /// * `resolved_relative_path_route` - The resolved relative path route
    ///
    /// # Returns
    /// `true` if the request has moved permanently, `false` otherwise.
    fn has_moved_permanently(&self, request_path: &str, resolved_relative_path_route: &String) -> bool {
        resolved_relative_path_route.is_empty() && request_path.chars().last() != Some('/')
    }
}