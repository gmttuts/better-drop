use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileInfo {
  pub name: String,
  /// -1 nếu không biết.
  pub size: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PathResponse {
  pub path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct UriArgs<'a> {
  pub uri: &'a str,
}

#[derive(Debug, Serialize)]
pub(crate) struct PathArgs<'a> {
  pub path: &'a str,
}
