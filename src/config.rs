use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

fn default_version() -> u16 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u16,
    #[serde(default)]
    pub supervisor: Option<SupervisorDefinition>,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
}

impl Config {
    pub fn from_toml_str(input: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(input)
    }

    pub fn from_file(path: &Path) -> Result<Self, ConfigLoadError> {
        let content = fs::read_to_string(path).map_err(ConfigLoadError::Io)?;
        let parsed = Self::from_toml_str(&content).map_err(ConfigLoadError::Toml)?;
        Ok(parsed)
    }

    pub fn tool(&self, tool_id: &str) -> Option<&ToolDefinition> {
        self.tools.iter().find(|tool| tool.id == tool_id)
    }

    pub fn supervisor(&self) -> Option<&SupervisorDefinition> {
        self.supervisor.as_ref()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub parser: ToolParser,
    #[serde(default)]
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolParser {
    pub entrypoint: String,
    pub script: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupervisorDefinition {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<String>,
    pub parser: ToolParser,
}

#[derive(Debug)]
pub enum ConfigLoadError {
    Io(std::io::Error),
    Toml(toml::de::Error),
}

impl std::fmt::Display for ConfigLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Toml(err) => write!(f, "TOML parse error: {err}"),
        }
    }
}

impl std::error::Error for ConfigLoadError {}
