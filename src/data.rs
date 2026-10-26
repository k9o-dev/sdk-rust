//! Data-plane DSL.
//!
//! Usage:
//!
//! ```ignore
//! let resp = k9o.from("posts").select().await?;
//! for row in resp.data { println!("{:?}", row); }
//! ```

use crate::envelope::Response;
use crate::http::{request, HttpClientConfig};
use crate::kerror::KError;
use crate::url_grammar::{enc, project_path};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

/// Entry point for the data-plane DSL — returned by `Client::from`.
pub struct FromBuilder<'a> {
    pub(crate) config: &'a HttpClientConfig,
    pub(crate) collection: String,
}

impl<'a> FromBuilder<'a> {
    /// Initiate a SELECT query. Returns a chainable filter/page builder.
    pub fn select(self) -> SelectBuilder<'a> {
        SelectBuilder {
            config: self.config,
            collection: self.collection,
            filters: Vec::new(),
            limit: None,
            cursor: None,
            count: None,
        }
    }

    /// Insert a single record. Returns the envelope with the inserted
    /// record + meta.
    pub async fn insert<T: DeserializeOwned, B: Serialize + ?Sized>(
        self,
        record: &B,
    ) -> Result<Response<T>, KError> {
        let path = collection_path(self.config, &self.collection)?;
        request::<T, B>(self.config, Method::POST, &path, Some(record), None).await
    }

    /// Initiate an UPDATE. Returns a builder; chain MUST include
    /// `.eq("id", ...)` before `.execute().await`.
    pub fn update<B: Serialize>(self, patch: B) -> UpdateBuilder<'a, B> {
        UpdateBuilder {
            config: self.config,
            collection: self.collection,
            patch,
            filters: Vec::new(),
        }
    }

    /// Initiate a DELETE. Same `.eq("id", ...)` discipline.
    pub fn delete(self) -> DeleteBuilder<'a> {
        DeleteBuilder {
            config: self.config,
            collection: self.collection,
            filters: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct Filter {
    column: String,
    op: String,
    value: Value,
}

/// SELECT query builder. Chainable; terminal via `.execute::<T>().await`.
pub struct SelectBuilder<'a> {
    config: &'a HttpClientConfig,
    collection: String,
    filters: Vec<Filter>,
    limit: Option<u64>,
    cursor: Option<String>,
    count: Option<String>,
}

macro_rules! filter_method {
    ($name:ident, $op:expr) => {
        pub fn $name<V: Into<Value>>(mut self, column: &str, value: V) -> Self {
            self.filters.push(Filter {
                column: column.into(),
                op: $op.into(),
                value: value.into(),
            });
            self
        }
    };
}

impl<'a> SelectBuilder<'a> {
    filter_method!(eq, "eq");
    filter_method!(neq, "neq");
    filter_method!(gt, "gt");
    filter_method!(gte, "gte");
    filter_method!(lt, "lt");
    filter_method!(lte, "lte");
    filter_method!(like, "like");
    filter_method!(ilike, "ilike");
    filter_method!(is_, "is");

    pub fn in_<V: Into<Value>>(mut self, column: &str, values: Vec<V>) -> Self {
        let arr: Vec<Value> = values.into_iter().map(Into::into).collect();
        self.filters.push(Filter {
            column: column.into(),
            op: "in".into(),
            value: Value::Array(arr),
        });
        self
    }

    pub fn not_in<V: Into<Value>>(mut self, column: &str, values: Vec<V>) -> Self {
        let arr: Vec<Value> = values.into_iter().map(Into::into).collect();
        self.filters.push(Filter {
            column: column.into(),
            op: "notIn".into(),
            value: Value::Array(arr),
        });
        self
    }

    pub fn filter<V: Into<Value>>(mut self, column: &str, op: &str, value: V) -> Self {
        self.filters.push(Filter {
            column: column.into(),
            op: op.into(),
            value: value.into(),
        });
        self
    }

    pub fn limit(mut self, n: u64) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn cursor(mut self, opaque: impl Into<String>) -> Self {
        self.cursor = Some(opaque.into());
        self
    }

    /// Count mode: "none" | "exact" | "estimated".
    pub fn count(mut self, mode: impl Into<String>) -> Self {
        self.count = Some(mode.into());
        self
    }

    fn to_query(&self) -> Vec<(String, String)> {
        let mut q = Vec::new();
        for f in &self.filters {
            let val_str = match &f.value {
                Value::Array(arr) => {
                    let parts: Vec<String> = arr.iter().map(value_to_string).collect();
                    format!("{}.({})", f.op, parts.join(","))
                }
                v => format!("{}.{}", f.op, value_to_string(v)),
            };
            q.push((f.column.clone(), val_str));
        }
        if let Some(n) = self.limit {
            q.push(("limit".into(), n.to_string()));
        }
        if let Some(c) = &self.cursor {
            q.push(("cursor".into(), c.clone()));
        }
        if let Some(m) = &self.count {
            q.push(("count".into(), m.clone()));
        }
        q
    }

    /// Terminal verb: execute the SELECT, returning `Result<Response<Vec<T>>, KError>`.
    pub async fn execute<T: DeserializeOwned>(self) -> Result<Response<Vec<T>>, KError> {
        let path = collection_path(self.config, &self.collection)?;
        let query = self.to_query();
        request::<Vec<T>, ()>(self.config, Method::GET, &path, None, Some(&query)).await
    }
}

/// UPDATE builder. Requires `.eq("id", ...)`.
pub struct UpdateBuilder<'a, B: Serialize> {
    config: &'a HttpClientConfig,
    collection: String,
    patch: B,
    filters: Vec<Filter>,
}

impl<'a, B: Serialize> UpdateBuilder<'a, B> {
    pub fn eq<V: Into<Value>>(mut self, column: &str, value: V) -> Self {
        self.filters.push(Filter {
            column: column.into(),
            op: "eq".into(),
            value: value.into(),
        });
        self
    }

    pub async fn execute<T: DeserializeOwned>(self) -> Result<Response<T>, KError> {
        let id_val = self
            .filters
            .iter()
            .find(|f| f.column == "id" && f.op == "eq")
            .map(|f| value_to_string(&f.value))
            .ok_or_else(|| {
                KError::new(
                    "K9O_INVALID_FIELD_VALUE",
                    "update() requires .eq(\"id\", ...) filter; changing many rows in one call is not supported",
                )
                .with_context("collection", self.collection.clone())
            })?;
        let path = format!(
            "{}/{}",
            collection_path(self.config, &self.collection)?,
            enc(&id_val)
        );
        request::<T, B>(self.config, Method::PATCH, &path, Some(&self.patch), None).await
    }
}

/// DELETE builder. Requires `.eq("id", ...)`.
pub struct DeleteBuilder<'a> {
    config: &'a HttpClientConfig,
    collection: String,
    filters: Vec<Filter>,
}

impl<'a> DeleteBuilder<'a> {
    pub fn eq<V: Into<Value>>(mut self, column: &str, value: V) -> Self {
        self.filters.push(Filter {
            column: column.into(),
            op: "eq".into(),
            value: value.into(),
        });
        self
    }

    pub async fn execute<T: DeserializeOwned>(self) -> Result<Response<T>, KError> {
        let id_val = self
            .filters
            .iter()
            .find(|f| f.column == "id" && f.op == "eq")
            .map(|f| value_to_string(&f.value))
            .ok_or_else(|| {
                KError::new(
                    "K9O_INVALID_FIELD_VALUE",
                    "delete() requires .eq(\"id\", ...) filter; deleting many rows in one call is not supported",
                )
            })?;
        let path = format!(
            "{}/{}",
            collection_path(self.config, &self.collection)?,
            enc(&id_val)
        );
        request::<T, ()>(self.config, Method::DELETE, &path, None, None).await
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

pub(crate) fn collection_path(
    config: &HttpClientConfig,
    collection: &str,
) -> Result<String, KError> {
    let slug = config.project_slug.as_ref().ok_or_else(|| {
        KError::new(
            "K9O_MISSING_PROJECT_CONTEXT",
            "data-plane operations require project_slug to be bound on the client",
        )
        .with_context("collection", collection)
        .with_recommendation("Pass project_slug in ClientOpts; or use a project-scoped sub-client.")
    })?;
    if collection.is_empty()
        || !collection
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_lowercase())
        || collection.len() > 63
        || !collection
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(KError::new(
            "K9O_INVALID_FIELD_VALUE",
            format!(
                "collection name {:?} does not match required format",
                collection
            ),
        )
        .with_context("collection", collection));
    }
    Ok(project_path(slug, &format!("data/{}", collection)))
}
