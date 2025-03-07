use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use globset::Glob;
use log::{debug, info};
use serde_json::Value;
use xor_name::XorName;
use globset::{GlobSet, GlobSetBuilder};

/// Configuration for web applications served by AntTP.
///
/// This struct holds the route mapping configuration for web applications,
/// allowing for custom routing rules such as SPA (Single Page Application) routing.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppConfig {
    /// Mapping of route patterns to target files.
    /// 
    /// For example, a mapping of "blog/*" to "index.html" would route all paths
    /// starting with "blog/" to the index.html file, which is useful for SPAs.
    route_map: HashMap<String, String>
}

impl AppConfig {
    /// Creates a new AppConfig with default settings.
    ///
    /// # Returns
    /// A new AppConfig instance with an empty route map.
    pub fn default() -> Self {
        Self {
            route_map: HashMap::new(),
        }
    }
    
    /// Resolves a route based on the application's route mapping.
    ///
    /// This function checks if the given relative path matches any of the route
    /// patterns in the route map. If a match is found, it returns the target file
    /// for that route.
    ///
    /// # Parameters
    /// * `relative_path` - The relative path to resolve
    /// * `archive_file_name` - The name of the archive file
    ///
    /// # Returns
    /// A tuple containing:
    /// * The resolved path or file name
    /// * A boolean indicating whether a route mapping was found
    pub fn resolve_route(&self, relative_path: String, archive_file_name: String) -> (String, bool) {
        // If the route map is empty, return the original path
        if self.route_map.is_empty() {
            debug!("Route map is empty, returning original path: {}", relative_path);
            return (relative_path, false);
        }

        // Build a GlobSet from the route patterns
        let mut builder = GlobSetBuilder::new();
        for key in self.route_map.keys() {
            match Glob::new(key) {
                Ok(glob) => { builder.add(glob); },
                Err(err) => {
                    info!("Invalid glob pattern '{}': {}", key, err);
                    continue;
                }
            }
        }

        let glob_set = match builder.build() {
            Ok(set) => set,
            Err(err) => {
                info!("Failed to build glob set: {}", err);
                return (relative_path, false);
            }
        };

        // Check if the relative path matches any of the patterns
        if let Some(match_index) = glob_set.matches(&relative_path).iter().next() {
            let pattern = self.route_map.keys().nth(match_index).unwrap();
            let target = self.route_map.get(pattern).unwrap();
            debug!("Route matched: {} -> {}", pattern, target);
            return (target.clone(), true);
        }

        // No match found, return the original path
        debug!("No route match found for: {}", relative_path);
        (archive_file_name, false)
    }

    /// Creates a new AppConfig from a JSON value.
    ///
    /// # Parameters
    /// * `json_value` - The JSON value containing the route map configuration
    ///
    /// # Returns
    /// A new AppConfig instance with the route map from the JSON value
    pub fn from_json(json_value: Value) -> Self {
        let mut route_map = HashMap::new();

        // Extract the routeMap from the JSON value
        if let Some(route_map_value) = json_value.get("routeMap") {
            if let Some(route_map_obj) = route_map_value.as_object() {
                for (key, value) in route_map_obj {
                    if let Some(value_str) = value.as_str() {
                        route_map.insert(key.clone(), value_str.to_string());
                        debug!("Added route mapping: {} -> {}", key, value_str);
                    }
                }
            }
        }

        AppConfig {
            route_map
        }
    }
}