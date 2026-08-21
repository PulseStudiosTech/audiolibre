use std::collections::HashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use crate::error::{AudioLibreError, Result};

/// Maps application names/pids to mixer channel IDs so per-app routing
/// can be enforced at the capture level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRoute {
    pub app_name: String,
    pub channel_id: String,
}

pub struct Router {
    routes: RwLock<HashMap<String, String>>, // app_name → channel_id
}

impl Router {
    pub fn new() -> Self {
        Self {
            routes: RwLock::new(HashMap::new()),
        }
    }

    pub fn add_route(&self, app_name: impl Into<String>, channel_id: impl Into<String>) {
        self.routes.write().insert(app_name.into(), channel_id.into());
    }

    pub fn remove_route(&self, app_name: &str) -> Result<()> {
        self.routes
            .write()
            .remove(app_name)
            .map(|_| ())
            .ok_or_else(|| AudioLibreError::MixerError(format!("No route for app '{}'", app_name)))
    }

    pub fn resolve(&self, app_name: &str) -> Option<String> {
        self.routes.read().get(app_name).cloned()
    }

    pub fn all_routes(&self) -> Vec<AppRoute> {
        self.routes
            .read()
            .iter()
            .map(|(app, ch)| AppRoute {
                app_name: app.clone(),
                channel_id: ch.clone(),
            })
            .collect()
    }
}
