use std::convert::TryInto;
use actix_http::header::{HeaderMap, IF_NONE_MATCH};
use autonomi::files::archive_public::ArchiveAddress;
use autonomi::files::PublicArchive;
use log::{debug, info, warn};
use xor_name::XorName;
use crate::caching_client::CachingClient;
use crate::archive_helper::{DataState};

/// XorHelper provides utility functions for working with XOR addresses in the Autonomi network.
/// It handles conversion between string representations and XorName objects, as well as
/// determining if data has been modified based on HTTP headers.
#[derive(Clone)]
pub struct XorHelper {
}

impl XorHelper {
    /// Creates a new XorHelper instance.
    ///
    /// # Returns
    /// A new XorHelper instance.
    pub fn new() -> XorHelper {
        XorHelper {}
    }

    /// Determines if data has been modified by comparing the If-None-Match header with the XorName.
    ///
    /// This function checks if the client already has the current version of the data by
    /// comparing the ETag (which is the hex representation of the XorName) with the
    /// If-None-Match header value.
    ///
    /// # Parameters
    /// * `headers` - The HTTP request headers
    /// * `xor_name` - The XorName of the requested resource
    ///
    /// # Returns
    /// `DataState::Modified` if the data has been modified or the header is not present
    /// `DataState::NotModified` if the data has not been modified
    pub fn get_data_state(&self, headers: &HeaderMap, xor_name: &XorName) -> DataState {
        if headers.contains_key(IF_NONE_MATCH) {
            // Safely extract the If-None-Match header value
            let e_tag = match headers.get(IF_NONE_MATCH).and_then(|v| v.to_str().ok()) {
                Some(tag) => tag,
                None => {
                    debug!("Invalid If-None-Match header value, treating as modified");
                    return DataState::Modified;
                }
            };
            
            let source_e_tag = e_tag.to_string().replace("\"", "");
            let target_e_tag = format!("{:x}", xor_name);
            
            debug!("is_modified == [{}], source_e_tag = [{}], target_e_tag = [{}], IF_NONE_MATCH present", 
                   source_e_tag == target_e_tag, source_e_tag, target_e_tag);
            
            if source_e_tag != target_e_tag {
                DataState::Modified
            } else {
                DataState::NotModified
            }
        } else {
            debug!("is_modified == [true], IF_NONE_MATCH absent");
            DataState::Modified
        }
    }

    /// Resolves a string to either an archive or a file XOR address.
    ///
    /// This function attempts to interpret the provided strings as XOR addresses.
    /// It first checks if the archive_addr is a valid XOR address and tries to retrieve
    /// the archive. If that fails, it checks if archive_file_name is a valid XOR address.
    ///
    /// # Parameters
    /// * `caching_autonomi_client` - The client used to retrieve archives
    /// * `archive_addr` - The potential archive address as a string
    /// * `archive_file_name` - The potential file name or address as a string
    ///
    /// # Returns
    /// A tuple containing:
    /// * `bool` - Whether a valid XOR address was found
    /// * `PublicArchive` - The retrieved archive (if any)
    /// * `bool` - Whether the address refers to an archive
    /// * `XorName` - The resolved XorName
    pub async fn resolve_archive_or_file(&self, caching_autonomi_client: &CachingClient, archive_addr: &String, archive_file_name: &String) -> (bool, PublicArchive, bool, XorName) {
        if self.is_xor(&archive_addr) {
            // Try to parse the archive address as a XorName
            let archive_addr_xorname = match self.str_to_xor_name(&archive_addr) {
                Ok(name) => name,
                Err(err) => {
                    warn!("Failed to parse archive address as XorName: {}", err);
                    return (false, PublicArchive::new(), false, XorName::default());
                }
            };
            
            let archive_address = ArchiveAddress::new(archive_addr_xorname);
            
            // Try to retrieve the archive
            match caching_autonomi_client.archive_get_public(archive_address).await {
                Ok(public_archive) => {
                    info!("Found archive at [{:x}]", archive_addr_xorname);
                    (true, public_archive, true, archive_addr_xorname)
                }
                Err(err) => {
                    info!("No archive found at [{:x}]. Treating as XOR address. Error: {}", 
                          archive_addr_xorname, err);
                    (true, PublicArchive::new(), false, archive_addr_xorname)
                }
            }
        } else if self.is_xor(&archive_file_name) {
            // Try to parse the file name as a XorName
            let archive_file_name_xorname = match self.str_to_xor_name(&archive_file_name) {
                Ok(name) => name,
                Err(err) => {
                    warn!("Failed to parse file name as XorName: {}", err);
                    return (false, PublicArchive::new(), false, XorName::default());
                }
            };
            
            info!("Found XOR address [{:x}]", archive_file_name_xorname);
            (true, PublicArchive::new(), false, archive_file_name_xorname)
        } else {
            warn!("Failed to find archive or filename [{:?}]", archive_file_name);
            (false, PublicArchive::new(), false, XorName::default())
        }
    }

    /// Checks if a string has the correct length for a XOR address.
    ///
    /// # Parameters
    /// * `chunk_address` - The string to check
    ///
    /// # Returns
    /// `true` if the string has the correct length (64 characters), `false` otherwise
    fn is_xor_len(&self, chunk_address: &String) -> bool {
        chunk_address.len() == 64
    }

    /// Checks if a string is a valid XOR address.
    ///
    /// A valid XOR address must have the correct length and be parseable as a XorName.
    ///
    /// # Parameters
    /// * `chunk_address` - The string to check
    ///
    /// # Returns
    /// `true` if the string is a valid XOR address, `false` otherwise
    pub fn is_xor(&self, chunk_address: &String) -> bool {
        self.is_xor_len(chunk_address) && self.str_to_xor_name(chunk_address).is_ok()
    }

    /// Converts a hexadecimal string to a XorName.
    ///
    /// # Parameters
    /// * `str` - The hexadecimal string to convert
    ///
    /// # Returns
    /// A Result containing the XorName if successful, or an error if the conversion failed
    fn str_to_xor_name(&self, str: &String) -> color_eyre::Result<XorName> {
        // Decode the hex string to bytes
        let bytes = hex::decode(str)?;
        
        // Convert the bytes to a XorName
        let xor_name_bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| color_eyre::eyre::eyre!("Failed to parse XorName from hex string: incorrect length"))?;
        
        Ok(XorName(xor_name_bytes))
    }

    /// Extracts the archive address and file name from a vector of path parts.
    ///
    /// # Parameters
    /// * `path_parts` - The vector of path parts
    ///
    /// # Returns
    /// A tuple containing the archive address and file name
    pub fn assign_path_parts(&self, path_parts: Vec<String>) -> (String, String) {
        if path_parts.len() > 1 {
            (path_parts[0].to_string(), path_parts[1].to_string())
        } else if !path_parts.is_empty() {
            (path_parts[0].to_string(), "".to_string())
        } else {
            ("".to_string(), "".to_string())
        }
    }
}