pub mod api;

use std::collections::HashMap;
use anyhow::Result;

pub struct ExtensionHost {
    _engine: wasmtime::Engine,
    extensions: HashMap<String, LoadedExtension>,
}

struct LoadedExtension {
    name: String,
    version: String,
    _store: wasmtime::Store<api::ExtState>,
    _instance: wasmtime::Instance,
}

impl ExtensionHost {
    pub fn new() -> Result<Self> {
        let engine = wasmtime::Engine::default();
        Ok(Self {
            _engine: engine,
            extensions: HashMap::new(),
        })
    }

    pub fn load(&mut self, _path: &str) -> Result<()> {
        Ok(())
    }

    pub fn list(&self) -> Vec<String> {
        self.extensions.keys().cloned().collect()
    }
}
