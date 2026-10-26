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
            search_query: Vec::new(),
            search_error: None,
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
    search_query: Vec<(String, String)>,
    search_error: Option<KError>,
}

/// Query syntax of `text_search`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextSearchType {
    #[default]
    Plain,
    Phrase,
    Websearch,
}

impl TextSearchType {
    fn as_str(self) -> &'static str {
        match self {
            TextSearchType::Plain => "plain",
            TextSearchType::Phrase => "phrase",
            TextSearchType::Websearch => "websearch",
        }
    }
}

/// Options of `text_search`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextSearchOptions<'a> {
    /// A Postgres text-search configuration name; default `english`.
    pub language: Option<&'a str>,
    /// `Plain` (default), `Phrase` or `Websearch`.
    pub search_type: TextSearchType,
}

/// Distance metric of `vector_search`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VectorMetric {
    #[default]
    Cosine,
    Euclidean,
    Dot,
}

impl VectorMetric {
    fn as_str(self) -> &'static str {
        match self {
            VectorMetric::Cosine => "cosine",
            VectorMetric::Euclidean => "euclidean",
            VectorMetric::Dot => "dot",
        }
    }
}

/// Options of `vector_search`.
#[derive(Debug, Clone, Copy, Default)]
pub struct VectorSearchOptions {
    /// `Cosine` (default), `Euclidean` or `Dot`.
    pub metric: VectorMetric,
    /// Rows to return, 1 to 1000 (gateway default 10). Results come back closest first.
    pub limit: Option<u32>,
}

fn valid_search_column(column: &str) -> bool {
    let mut chars = column.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    column.len() <= 63 && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
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

    fn fail_search(mut self, message: &str) -> Self {
        if self.search_error.is_none() {
            self.search_error = Some(KError::new("K9O_INVALID_FIELD_VALUE", message));
        }
        self
    }

    fn set_search(&mut self, key: &str, value: Option<String>) {
        self.search_query.retain(|(k, _)| k != key);
        if let Some(v) = value {
            self.search_query.push((key.into(), v));
        }
    }

    /// Full-text search over one text column. Composes with filters, `limit` and `cursor`. An invalid
    /// argument fails `execute` with `K9O_INVALID_FIELD_VALUE` before any request.
    pub fn text_search(
        mut self,
        column: &str,
        query: &str,
        options: TextSearchOptions<'_>,
    ) -> Self {
        if !valid_search_column(column) {
            return self.fail_search(&format!(
                "textSearch column '{column}' is not a valid identifier"
            ));
        }
        if query.is_empty() {
            return self.fail_search("textSearch query must be non-empty");
        }
        let language = options.language.unwrap_or("english");
        if language.is_empty()
            || !language
                .chars()
                .all(|c| c.is_ascii_alphabetic() || c == '_')
        {
            return self
                .fail_search("textSearch language must be a text-search configuration name");
        }
        self.set_search("textSearch", Some(format!("{column}:{query}")));
        self.set_search(
            "textSearchLanguage",
            (language != "english").then(|| language.to_string()),
        );
        self.set_search(
            "textSearchType",
            (options.search_type != TextSearchType::Plain)
                .then(|| options.search_type.as_str().to_string()),
        );
        self
    }

    /// Semantic search: the rows nearest to an embedding in one vector column, closest first.
    pub fn vector_search(
        mut self,
        column: &str,
        embedding: &[f64],
        options: VectorSearchOptions,
    ) -> Self {
        if !valid_search_column(column) {
            return self.fail_search(&format!(
                "vectorSearch column '{column}' is not a valid identifier"
            ));
        }
        if embedding.is_empty() || embedding.iter().any(|n| !n.is_finite()) {
            return self.fail_search("vectorSearch embedding must be a non-empty array of numbers");
        }
        if let Some(l) = options.limit {
            if !(1..=1000).contains(&l) {
                return self.fail_search("vectorSearch limit must be an integer from 1 to 1000");
            }
        }
        let json = serde_json::to_string(embedding).unwrap_or_default();
        self.set_search("vectorSearch", Some(format!("{column}:{json}")));
        self.set_search(
            "vectorMetric",
            (options.metric != VectorMetric::Cosine).then(|| options.metric.as_str().to_string()),
        );
        self.set_search("vectorLimit", options.limit.map(|l| l.to_string()));
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
        q.extend(self.search_query.iter().cloned());
        q
    }

    /// Terminal verb: execute the SELECT, returning `Result<Response<Vec<T>>, KError>`.
    pub async fn execute<T: DeserializeOwned>(self) -> Result<Response<Vec<T>>, KError> {
        if let Some(e) = &self.search_error {
            return Err(e.clone());
        }
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
