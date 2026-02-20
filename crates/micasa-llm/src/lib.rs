// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Context, Result, anyhow, bail};
use reqwest::StatusCode;
use reqwest::blocking::{Client as HttpClient, Response};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Lines};
use std::time::Duration;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    const fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamChunk {
    pub content: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PullChunk {
    pub status: Option<String>,
    pub digest: Option<String>,
    pub total: Option<i64>,
    pub completed: Option<i64>,
    pub error: Option<String>,
}

pub struct PullScanner {
    lines: Lines<BufReader<Response>>,
}

impl PullScanner {
    pub fn next_chunk(&mut self) -> Result<Option<PullChunk>> {
        for line in self.lines.by_ref() {
            let line = line.context("read pull stream")?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let chunk: PullChunk = match serde_json::from_str(trimmed) {
                Ok(chunk) => chunk,
                Err(_) => continue,
            };
            return Ok(Some(chunk));
        }
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    base_url: String,
    model: String,
    timeout: Duration,
    http: HttpClient,
}

impl Client {
    pub fn new(base_url: &str, model: &str, timeout: Duration) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/').to_owned();
        if base_url.is_empty() {
            bail!("llm.base_url must not be empty");
        }
        if model.trim().is_empty() {
            bail!("llm.model must not be empty");
        }

        let http = HttpClient::builder()
            .timeout(timeout)
            .build()
            .context("build HTTP client")?;

        Ok(Self {
            base_url,
            model: model.to_owned(),
            timeout,
            http,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn set_model(&mut self, model: &str) {
        self.model = model.to_owned();
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn list_models(&self) -> Result<Vec<String>> {
        let response = self
            .http
            .get(format!("{}/models", self.base_url))
            .send()
            .map_err(|error| connection_error(&self.base_url, error))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(clean_error_response(status, &body));
        }

        let parsed: ModelsResponse = response.json().context("decode model list")?;
        Ok(parsed.data.into_iter().map(|model| model.id).collect())
    }

    pub fn ping(&self) -> Result<()> {
        let models = self.list_models()?;
        let exists = models
            .iter()
            .any(|name| name == &self.model || name.starts_with(&format!("{}:", self.model)));
        if !exists {
            bail!(
                "model {:?} not found -- pull it with `ollama pull {}`",
                self.model,
                self.model
            );
        }
        Ok(())
    }

    pub fn pull_model(&self, model: &str) -> Result<PullScanner> {
        let ollama_base = self
            .base_url
            .trim_end_matches("/v1")
            .trim_end_matches('/')
            .to_owned();

        let response = self
            .http
            .post(format!("{ollama_base}/api/pull"))
            .json(&serde_json::json!({ "name": model }))
            .send()
            .map_err(|error| connection_error(&ollama_base, error))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(clean_error_response(status, &body));
        }

        Ok(PullScanner {
            lines: BufReader::new(response).lines(),
        })
    }

    pub fn chat_complete(&self, messages: &[Message]) -> Result<String> {
        let request = ChatRequest::new(&self.model, messages, false);
        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .json(&request)
            .send()
            .map_err(|error| connection_error(&self.base_url, error))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(clean_error_response(status, &body));
        }

        let parsed: ChatCompletionResponse = response.json().context("decode chat response")?;
        let content = parsed
            .choices
            .first()
            .map(|choice| choice.message.content.clone())
            .ok_or_else(|| anyhow!("no choices in chat response"))?;
        Ok(content)
    }

    pub fn chat_stream(&self, messages: &[Message]) -> Result<ChatStream> {
        let request = ChatRequest::new(&self.model, messages, true);
        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .json(&request)
            .send()
            .map_err(|error| connection_error(&self.base_url, error))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(clean_error_response(status, &body));
        }

        Ok(ChatStream {
            done: false,
            lines: BufReader::new(response).lines(),
        })
    }
}

pub struct ChatStream {
    done: bool,
    lines: Lines<BufReader<Response>>,
}

impl Iterator for ChatStream {
    type Item = Result<StreamChunk>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        loop {
            let line = match self.lines.next() {
                None => {
                    self.done = true;
                    return None;
                }
                Some(Ok(line)) => line,
                Some(Err(error)) => {
                    self.done = true;
                    return Some(Err(error).context("read stream"));
                }
            };

            let trimmed = line.trim();
            if !trimmed.starts_with("data: ") {
                continue;
            }

            let payload = trimmed.trim_start_matches("data: ");
            if payload == "[DONE]" {
                self.done = true;
                return Some(Ok(StreamChunk {
                    content: String::new(),
                    done: true,
                }));
            }

            let chunk: ChatCompletionChunk = match serde_json::from_str(payload) {
                Ok(chunk) => chunk,
                Err(error) => {
                    self.done = true;
                    return Some(Err(error).context("decode stream chunk"));
                }
            };

            let Some(choice) = chunk.choices.into_iter().next() else {
                continue;
            };

            let content = choice.delta.content.unwrap_or_default();
            let done = choice.finish_reason.is_some();
            if done {
                self.done = true;
            }

            if content.is_empty() && !done {
                continue;
            }

            return Some(Ok(StreamChunk { content, done }));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableInfo {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnInfo {
    pub name: String,
    pub column_type: String,
    pub not_null: bool,
    pub primary_key: bool,
}

pub fn build_sql_prompt(
    tables: &[TableInfo],
    now: OffsetDateTime,
    column_hints: Option<&str>,
    extra_context: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str(
        "You are a SQL generator for a SQLite database. Output only a single SELECT statement.\n",
    );
    out.push_str("\n## Current date\n\n");
    out.push_str(&format!(
        "Today is {}.\n",
        now.date()
            .format(&time::macros::format_description!(
                "[weekday repr:long], [month repr:long] [day], [year]"
            ))
            .unwrap_or_else(|_| "today".to_owned())
    ));
    out.push_str("\n## Schema\n\n```sql\n");
    for table in tables {
        out.push_str(&format_ddl(table));
        out.push('\n');
    }
    out.push_str("```\n");
    out.push_str("\nRules:\n");
    out.push_str("1. Never emit INSERT/UPDATE/DELETE/DDL.\n");
    out.push_str("2. Exclude soft-deleted rows (`deleted_at IS NULL`) unless asked.\n");
    out.push_str("3. Money columns are cents; divide by 100.0 for display.\n");
    if let Some(hints) = column_hints
        && !hints.is_empty()
    {
        out.push_str("\n## Known values\n\n");
        out.push_str(hints);
        out.push('\n');
    }
    if let Some(context) = extra_context
        && !context.is_empty()
    {
        out.push_str("\n## Additional context\n\n");
        out.push_str(context);
        out.push('\n');
    }
    out
}

pub fn build_summary_prompt(
    question: &str,
    sql: &str,
    results_table: &str,
    now: OffsetDateTime,
    extra_context: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str("You are a helpful assistant that summarizes SQL results.\n");
    out.push_str(&format!("Current date: {}\n\n", now.date()));
    out.push_str("## User question\n\n");
    out.push_str(question);
    out.push_str("\n\n## SQL executed\n\n```sql\n");
    out.push_str(sql);
    out.push_str("\n```\n\n## Results\n\n```\n");
    out.push_str(results_table);
    out.push_str("\n```\n");
    out.push_str("\nKeep the answer concise and do not invent data.\n");
    if let Some(context) = extra_context
        && !context.is_empty()
    {
        out.push_str("\n## Additional context\n\n");
        out.push_str(context);
        out.push('\n');
    }
    out
}

pub fn build_fallback_prompt(
    tables: &[TableInfo],
    data_summary: &str,
    now: OffsetDateTime,
    extra_context: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str("You are micasa-assistant. Answer only from provided data.\n");
    out.push_str(&format!("Current date: {}\n\n", now.date()));
    out.push_str("## Database schema\n\n");
    for table in tables {
        out.push_str(&format_table(table));
        out.push('\n');
    }
    out.push_str("\n## Current data\n\n");
    out.push_str(data_summary);
    out.push('\n');
    if let Some(context) = extra_context
        && !context.is_empty()
    {
        out.push_str("\n## Additional context\n\n");
        out.push_str(context);
        out.push('\n');
    }
    out
}

pub fn format_results_table(columns: &[String], rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return "(no rows)\n".to_owned();
    }

    let mut out = String::new();
    out.push_str(&columns.join(" | "));
    out.push('\n');
    for row in rows {
        out.push_str(&row.join(" | "));
        out.push('\n');
    }
    out
}

pub fn extract_sql(raw: &str) -> String {
    let mut sql = raw.trim().to_owned();
    if sql.starts_with("```") {
        let mut lines: Vec<&str> = sql.lines().collect();
        if !lines.is_empty() {
            lines.remove(0);
        }
        if let Some(idx) = lines.iter().rposition(|line| line.trim() == "```") {
            lines.truncate(idx);
        }
        sql = lines.join("\n").trim().to_owned();
    }
    sql.trim_end_matches(';').trim().to_owned()
}

fn format_ddl(table: &TableInfo) -> String {
    let mut out = String::new();
    out.push_str(&format!("CREATE TABLE {} (\n", table.name));
    for (index, column) in table.columns.iter().enumerate() {
        out.push_str("  ");
        out.push_str(&column.name);
        out.push(' ');
        out.push_str(&column.column_type);
        if column.primary_key {
            out.push_str(" PRIMARY KEY");
        }
        if column.not_null {
            out.push_str(" NOT NULL");
        }
        if index + 1 < table.columns.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(");\n");
    out
}

fn format_table(table: &TableInfo) -> String {
    let mut out = String::new();
    out.push_str(&format!("### {}\n", table.name));
    for column in &table.columns {
        out.push_str(&format!(
            "- {} {}{}{}\n",
            column.name,
            column.column_type,
            if column.primary_key { " PK" } else { "" },
            if column.not_null { " NOT NULL" } else { "" }
        ));
    }
    out
}

fn connection_error(base_url: &str, error: reqwest::Error) -> anyhow::Error {
    anyhow!(
        "cannot reach {} -- start it with `ollama serve` ({} )",
        base_url,
        error
    )
}

fn clean_error_response(status: StatusCode, body: &str) -> anyhow::Error {
    if let Ok(parsed) = serde_json::from_str::<OpenAIErrorEnvelope>(body)
        && let Some(error) = parsed.error
        && !error.message.is_empty()
    {
        return anyhow!("server error ({}): {}", status.as_u16(), error.message);
    }

    if let Ok(parsed) = serde_json::from_str::<OllamaErrorEnvelope>(body)
        && let Some(error) = parsed.error
        && !error.is_empty()
    {
        return anyhow!("server error ({}): {}", status.as_u16(), error);
    }

    if body.len() < 100 && !body.contains('{') {
        return anyhow!("server error ({}): {}", status.as_u16(), body);
    }

    anyhow!("server returned {}", status.as_u16())
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    stream: bool,
    temperature: f32,
}

impl<'a> ChatRequest<'a> {
    fn new(model: &'a str, messages: &'a [Message], stream: bool) -> Self {
        Self {
            model,
            messages: messages
                .iter()
                .map(|message| ChatMessage {
                    role: message.role.as_str(),
                    content: &message.content,
                })
                .collect(),
            stream,
            temperature: 0.0,
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChunkChoice>,
}

#[derive(Debug, Deserialize)]
struct ChunkChoice {
    delta: ChunkDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChunkDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelRow>,
}

#[derive(Debug, Deserialize)]
struct ModelRow {
    id: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIErrorEnvelope {
    error: Option<OpenAIErrorBody>,
}

#[derive(Debug, Deserialize)]
struct OpenAIErrorBody {
    message: String,
}

#[derive(Debug, Deserialize)]
struct OllamaErrorEnvelope {
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        ColumnInfo, Message, Role, TableInfo, build_fallback_prompt, build_sql_prompt,
        build_summary_prompt, extract_sql, format_results_table,
    };
    use anyhow::Result;
    use time::OffsetDateTime;

    #[test]
    fn extract_sql_handles_fenced_blocks() {
        let raw = "```sql\nSELECT * FROM projects;\n```";
        assert_eq!(extract_sql(raw), "SELECT * FROM projects");
    }

    #[test]
    fn format_results_table_handles_empty_rows() {
        let rendered = format_results_table(&["name".to_owned()], &[]);
        assert_eq!(rendered, "(no rows)\n");
    }

    #[test]
    fn build_sql_prompt_includes_context() {
        let prompt = build_sql_prompt(
            &[TableInfo {
                name: "projects".to_owned(),
                columns: vec![ColumnInfo {
                    name: "id".to_owned(),
                    column_type: "INTEGER".to_owned(),
                    not_null: true,
                    primary_key: true,
                }],
            }],
            OffsetDateTime::UNIX_EPOCH,
            Some("status=underway"),
            Some("House is built in 1940."),
        );
        assert!(prompt.contains("CREATE TABLE projects"));
        assert!(prompt.contains("status=underway"));
        assert!(prompt.contains("House is built in 1940."));
    }

    #[test]
    fn build_sql_prompt_includes_expected_rules() {
        let prompt = build_sql_prompt(
            &[TableInfo {
                name: "projects".to_owned(),
                columns: vec![ColumnInfo {
                    name: "id".to_owned(),
                    column_type: "INTEGER".to_owned(),
                    not_null: true,
                    primary_key: true,
                }],
            }],
            OffsetDateTime::UNIX_EPOCH,
            None,
            None,
        );
        assert!(prompt.contains("Output only a single SELECT statement"));
        assert!(prompt.contains("Never emit INSERT/UPDATE/DELETE/DDL."));
        assert!(prompt.contains("Exclude soft-deleted rows"));
        assert!(prompt.contains("divide by 100.0"));
    }

    #[test]
    fn build_sql_prompt_includes_known_values_heading_when_hints_present() {
        let prompt = build_sql_prompt(
            &[TableInfo {
                name: "projects".to_owned(),
                columns: vec![ColumnInfo {
                    name: "status".to_owned(),
                    column_type: "TEXT".to_owned(),
                    not_null: false,
                    primary_key: false,
                }],
            }],
            OffsetDateTime::UNIX_EPOCH,
            Some("- project statuses: planned, underway"),
            None,
        );
        assert!(prompt.contains("## Known values"));
        assert!(prompt.contains("planned, underway"));
    }

    #[test]
    fn build_sql_prompt_omits_known_values_heading_when_hints_empty() {
        let prompt = build_sql_prompt(
            &[TableInfo {
                name: "projects".to_owned(),
                columns: vec![ColumnInfo {
                    name: "status".to_owned(),
                    column_type: "TEXT".to_owned(),
                    not_null: false,
                    primary_key: false,
                }],
            }],
            OffsetDateTime::UNIX_EPOCH,
            Some(""),
            None,
        );
        assert!(!prompt.contains("## Known values"));
    }

    #[test]
    fn build_summary_prompt_includes_question_sql_results_and_context() {
        let prompt = build_summary_prompt(
            "How many active projects?",
            "SELECT COUNT(*) AS count FROM projects",
            "count\n2",
            OffsetDateTime::UNIX_EPOCH,
            Some("Only include non-deleted rows."),
        );
        assert!(prompt.contains("How many active projects?"));
        assert!(prompt.contains("SELECT COUNT(*) AS count FROM projects"));
        assert!(prompt.contains("count\n2"));
        assert!(prompt.contains("Only include non-deleted rows."));
    }

    #[test]
    fn build_fallback_prompt_includes_schema_data_and_context() {
        let prompt = build_fallback_prompt(
            &[TableInfo {
                name: "projects".to_owned(),
                columns: vec![
                    ColumnInfo {
                        name: "id".to_owned(),
                        column_type: "INTEGER".to_owned(),
                        not_null: true,
                        primary_key: true,
                    },
                    ColumnInfo {
                        name: "title".to_owned(),
                        column_type: "TEXT".to_owned(),
                        not_null: true,
                        primary_key: false,
                    },
                ],
            }],
            "projects\n- title: Deck",
            OffsetDateTime::UNIX_EPOCH,
            Some("House has original 1940 wiring."),
        );
        assert!(prompt.contains("### projects"));
        assert!(prompt.contains("projects\n- title: Deck"));
        assert!(prompt.contains("House has original 1940 wiring."));
    }

    #[test]
    fn format_results_table_renders_rows() {
        let rendered = format_results_table(
            &["name".to_owned(), "status".to_owned()],
            &[vec!["Deck".to_owned(), "underway".to_owned()]],
        );
        assert_eq!(rendered, "name | status\nDeck | underway\n");
    }

    #[test]
    fn extract_sql_trims_whitespace_and_semicolons() {
        assert_eq!(extract_sql("  SELECT 1;  "), "SELECT 1");
        assert_eq!(
            extract_sql("\nSELECT * FROM projects;;\n"),
            "SELECT * FROM projects"
        );
    }

    #[test]
    fn extract_sql_handles_bare_fenced_blocks() {
        let raw = "```\nSELECT COUNT(*) FROM appliances\n```";
        assert_eq!(extract_sql(raw), "SELECT COUNT(*) FROM appliances");
    }

    #[test]
    fn chat_request_serializes_roles() -> Result<()> {
        let client = super::Client::new(
            "http://localhost:11434/v1",
            "qwen3",
            std::time::Duration::from_secs(5),
        )?;
        assert_eq!(client.model(), "qwen3");

        let messages = [Message {
            role: Role::User,
            content: "hello".to_owned(),
        }];
        let request = super::ChatRequest::new("qwen3", &messages, false);
        let encoded = serde_json::to_string(&request)?;
        assert!(encoded.contains("\"role\":\"user\""));
        Ok(())
    }
}
