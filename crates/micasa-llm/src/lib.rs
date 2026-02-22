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
    out.push_str(ENTITY_RELATIONSHIPS);
    out.push('\n');
    out.push_str(SQL_SCHEMA_NOTES);
    out.push_str("\nRules:\n");
    out.push_str("1. Never emit INSERT/UPDATE/DELETE/DDL.\n");
    out.push_str("2. Exclude soft-deleted rows (`deleted_at IS NULL`) unless asked.\n");
    out.push_str("3. Money columns are cents; divide by 100.0 for display.\n");
    if let Some(hints) = column_hints
        && !hints.is_empty()
    {
        out.push_str("\n## Known values in the database\n\n");
        out.push_str(hints);
        out.push('\n');
    }
    out.push('\n');
    out.push_str(SQL_FEW_SHOT_EXAMPLES);
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
    out.push_str(&format!("Current date: {}\n\n", format_human_date(now)));
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
    out.push_str(
        "You are micasa-assistant, a factual Q&A bot for a home management app. Answer only from provided data.\n",
    );
    out.push_str(&format!("Current date: {}\n\n", format_human_date(now)));
    out.push_str("## Database schema\n\n");
    for table in tables {
        out.push_str(&format_table(table));
        out.push('\n');
    }
    out.push_str(ENTITY_RELATIONSHIPS);
    out.push('\n');
    out.push_str(FALLBACK_SCHEMA_NOTES);
    if !data_summary.trim().is_empty() {
        out.push_str("\n## Current data\n\n");
        out.push_str(data_summary);
        out.push('\n');
    }
    out.push('\n');
    out.push_str(FALLBACK_GUIDELINES);
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

pub fn format_sql(sql: &str, max_width: usize) -> String {
    let tokens = tokenize_sql(sql);
    if tokens.is_empty() {
        return sql.to_owned();
    }

    let mut formatted = layout_clauses(&tokens);
    if max_width > 0 {
        formatted = wrap_long_lines(&formatted, max_width);
    }
    formatted
}

fn wrap_long_lines(text: &str, max_width: usize) -> String {
    text.lines()
        .flat_map(|line| {
            if line.len() <= max_width {
                vec![line.to_owned()]
            } else {
                soft_wrap_line(line, max_width)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn soft_wrap_line(line: &str, max_width: usize) -> Vec<String> {
    let trimmed = line.trim_start_matches(' ');
    let base_indent = line.len().saturating_sub(trimmed.len());
    let cont_indent = " ".repeat(base_indent + 4);
    let min_cut = cont_indent.len() + 1;

    let mut lines = Vec::new();
    let mut remaining = line.to_owned();
    while remaining.len() > max_width {
        let mut cut_at = remaining[..max_width].rfind(' ').unwrap_or(0);
        if cut_at < min_cut {
            if let Some(next_space) = remaining[max_width..].find(' ') {
                cut_at = max_width + next_space;
            } else {
                break;
            }
        }

        lines.push(remaining[..cut_at].trim_end_matches(' ').to_owned());
        remaining = format!(
            "{cont_indent}{}",
            remaining[cut_at..].trim_start_matches(' ')
        );
    }
    lines.push(remaining);
    lines
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SqlTokenKind {
    Word,
    Number,
    String,
    Symbol,
    Space,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SqlToken {
    kind: SqlTokenKind,
    text: String,
}

fn tokenize_sql(input: &str) -> Vec<SqlToken> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];

        if ch.is_whitespace() {
            while index < chars.len() && chars[index].is_whitespace() {
                index += 1;
            }
            tokens.push(SqlToken {
                kind: SqlTokenKind::Space,
                text: " ".to_owned(),
            });
            continue;
        }

        if ch == '\'' {
            let mut next = index + 1;
            while next < chars.len() {
                if chars[next] == '\'' {
                    if next + 1 < chars.len() && chars[next + 1] == '\'' {
                        next += 2;
                        continue;
                    }
                    next += 1;
                    break;
                }
                next += 1;
            }
            tokens.push(SqlToken {
                kind: SqlTokenKind::String,
                text: chars[index..next].iter().collect(),
            });
            index = next;
            continue;
        }

        if ch.is_ascii_digit() {
            let mut next = index;
            let mut seen_dot = false;
            while next < chars.len()
                && (chars[next].is_ascii_digit() || (chars[next] == '.' && !seen_dot))
            {
                if chars[next] == '.' {
                    seen_dot = true;
                }
                next += 1;
            }
            tokens.push(SqlToken {
                kind: SqlTokenKind::Number,
                text: chars[index..next].iter().collect(),
            });
            index = next;
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut next = index;
            while next < chars.len() && (chars[next].is_ascii_alphanumeric() || chars[next] == '_')
            {
                next += 1;
            }
            tokens.push(SqlToken {
                kind: SqlTokenKind::Word,
                text: chars[index..next].iter().collect(),
            });
            index = next;
            continue;
        }

        if index + 1 < chars.len() {
            let pair = [chars[index], chars[index + 1]];
            let pair_text: String = pair.iter().collect();
            if matches!(pair_text.as_str(), "<=" | ">=" | "<>" | "!=" | "||") {
                tokens.push(SqlToken {
                    kind: SqlTokenKind::Symbol,
                    text: pair_text,
                });
                index += 2;
                continue;
            }
        }

        tokens.push(SqlToken {
            kind: SqlTokenKind::Symbol,
            text: ch.to_string(),
        });
        index += 1;
    }

    tokens
}

fn clause_level(keyword: &str) -> i8 {
    match keyword {
        "SELECT" | "FROM" | "WHERE" | "ORDER BY" | "GROUP BY" | "HAVING" | "LIMIT" | "OFFSET"
        | "UNION" | "UNION ALL" | "INTERSECT" | "EXCEPT" | "INSERT" | "UPDATE" | "DELETE"
        | "SET" | "VALUES" | "LEFT JOIN" | "RIGHT JOIN" | "INNER JOIN" | "CROSS JOIN"
        | "FULL JOIN" | "JOIN" | "ON" => 0,
        "AND" | "OR" => 1,
        _ => -1,
    }
}

const MULTI_WORD_CLAUSES: [&str; 8] = [
    "UNION ALL",
    "ORDER BY",
    "GROUP BY",
    "LEFT JOIN",
    "RIGHT JOIN",
    "INNER JOIN",
    "CROSS JOIN",
    "FULL JOIN",
];

fn is_sql_keyword(word: &str) -> bool {
    matches!(
        word,
        "SELECT"
            | "DISTINCT"
            | "FROM"
            | "WHERE"
            | "AND"
            | "OR"
            | "NOT"
            | "IN"
            | "EXISTS"
            | "BETWEEN"
            | "LIKE"
            | "IS"
            | "NULL"
            | "AS"
            | "ON"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "INNER"
            | "CROSS"
            | "FULL"
            | "OUTER"
            | "ORDER"
            | "BY"
            | "ASC"
            | "DESC"
            | "GROUP"
            | "HAVING"
            | "LIMIT"
            | "OFFSET"
            | "UNION"
            | "ALL"
            | "INTERSECT"
            | "EXCEPT"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "ELSE"
            | "END"
            | "COUNT"
            | "SUM"
            | "AVG"
            | "MIN"
            | "MAX"
            | "COALESCE"
            | "CAST"
            | "IFNULL"
            | "INSERT"
            | "INTO"
            | "UPDATE"
            | "DELETE"
            | "SET"
            | "VALUES"
            | "TRUE"
            | "FALSE"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClauseToken {
    token: SqlToken,
    keyword: Option<String>,
    level: i8,
}

fn build_clause_tokens(tokens: &[SqlToken]) -> Vec<ClauseToken> {
    let mut normalized = tokens.to_vec();
    for token in &mut normalized {
        if token.kind == SqlTokenKind::Word {
            let upper = token.text.to_ascii_uppercase();
            if is_sql_keyword(&upper) {
                token.text = upper;
            }
        }
    }

    let mut result = Vec::new();
    let mut index = 0;
    while index < normalized.len() {
        if normalized[index].kind == SqlTokenKind::Space {
            result.push(ClauseToken {
                token: normalized[index].clone(),
                keyword: None,
                level: -1,
            });
            index += 1;
            continue;
        }

        if normalized[index].kind == SqlTokenKind::Word {
            let mut matched = false;
            for clause in MULTI_WORD_CLAUSES {
                let parts = clause.split_whitespace().collect::<Vec<_>>();
                if matches_word_sequence(&normalized, index, &parts) {
                    result.push(ClauseToken {
                        token: SqlToken {
                            kind: SqlTokenKind::Word,
                            text: clause.to_owned(),
                        },
                        keyword: Some(clause.to_owned()),
                        level: clause_level(clause),
                    });
                    index = advance_past_sequence(&normalized, index, parts.len());
                    matched = true;
                    break;
                }
            }
            if matched {
                continue;
            }

            let upper = normalized[index].text.to_ascii_uppercase();
            let level = clause_level(&upper);
            result.push(ClauseToken {
                token: normalized[index].clone(),
                keyword: if level >= 0 { Some(upper) } else { None },
                level,
            });
            index += 1;
            continue;
        }

        result.push(ClauseToken {
            token: normalized[index].clone(),
            keyword: None,
            level: -1,
        });
        index += 1;
    }

    result
}

fn matches_word_sequence(tokens: &[SqlToken], mut index: usize, parts: &[&str]) -> bool {
    for part in parts {
        while index < tokens.len() && tokens[index].kind == SqlTokenKind::Space {
            index += 1;
        }
        if index >= tokens.len() || tokens[index].kind != SqlTokenKind::Word {
            return false;
        }
        if tokens[index].text.to_ascii_uppercase() != *part {
            return false;
        }
        index += 1;
    }
    true
}

fn advance_past_sequence(tokens: &[SqlToken], mut index: usize, word_count: usize) -> usize {
    let mut seen = 0;
    while index < tokens.len() && seen < word_count {
        if tokens[index].kind != SqlTokenKind::Space {
            seen += 1;
        }
        index += 1;
    }
    index
}

fn trim_trailing_space(out: &mut String) {
    if out.ends_with(' ') {
        out.pop();
    }
}

fn layout_clauses(raw_tokens: &[SqlToken]) -> String {
    let tokens = build_clause_tokens(raw_tokens);
    let mut out = String::new();
    let indent_unit = "  ";
    let mut at_line_start = true;
    let mut in_select = false;
    let mut paren_depth = 0_usize;
    let mut base_indent = 0_usize;

    for (index, token) in tokens.iter().enumerate() {
        if token.token.kind == SqlTokenKind::Symbol && token.token.text == "(" {
            out.push_str(&token.token.text);
            paren_depth += 1;

            let mut next = index + 1;
            while next < tokens.len() && tokens[next].token.kind == SqlTokenKind::Space {
                next += 1;
            }
            if next < tokens.len() && tokens[next].keyword.as_deref() == Some("SELECT") {
                base_indent += 1;
            }
            at_line_start = false;
            continue;
        }

        if token.token.kind == SqlTokenKind::Symbol && token.token.text == ")" {
            if paren_depth > 0 {
                paren_depth -= 1;
                base_indent = base_indent.saturating_sub(1);
            }
            out.push_str(&token.token.text);
            at_line_start = false;
            continue;
        }

        if token.level >= 0 && paren_depth == 0 {
            let keyword = token.keyword.as_deref().unwrap_or("");
            if keyword == "SELECT" {
                if !out.is_empty() {
                    trim_trailing_space(&mut out);
                    out.push('\n');
                    out.push_str(&indent_unit.repeat(base_indent));
                }
                out.push_str(&token.token.text);
                at_line_start = false;
                in_select = true;
                continue;
            }

            if token.level == 1 {
                trim_trailing_space(&mut out);
                out.push('\n');
                out.push_str(&indent_unit.repeat(base_indent + 1));
                out.push_str(&token.token.text);
                at_line_start = false;
                continue;
            }

            trim_trailing_space(&mut out);
            out.push('\n');
            out.push_str(&indent_unit.repeat(base_indent));
            out.push_str(&token.token.text);
            at_line_start = false;
            in_select = false;
            continue;
        }

        if in_select
            && token.token.kind == SqlTokenKind::Symbol
            && token.token.text == ","
            && paren_depth == 0
        {
            out.push(',');
            out.push('\n');
            out.push_str(&indent_unit.repeat(base_indent + 1));
            at_line_start = true;
            continue;
        }

        if token.token.kind == SqlTokenKind::Space && at_line_start {
            continue;
        }

        out.push_str(&token.token.text);
        at_line_start = false;
    }

    out.trim().to_owned()
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

fn format_human_date(now: OffsetDateTime) -> String {
    now.date()
        .format(&time::macros::format_description!(
            "[weekday repr:long], [month repr:long] [day], [year]"
        ))
        .unwrap_or_else(|_| now.date().to_string())
}

const ENTITY_RELATIONSHIPS: &str = r#"
## Entity Relationships

Foreign key relationships between tables:
- projects.project_type_id -> project_types.id
- quotes.project_id -> projects.id
- quotes.vendor_id -> vendors.id
- maintenance_items.category_id -> maintenance_categories.id
- maintenance_items.appliance_id -> appliances.id
- service_log_entries.maintenance_item_id -> maintenance_items.id
- service_log_entries.vendor_id -> vendors.id
- incidents.appliance_id -> appliances.id
- incidents.vendor_id -> vendors.id

NO direct FK between projects and appliances.
"#;

const SQL_SCHEMA_NOTES: &str = r#"
## Schema notes

- case-insensitive matching: use LOWER() on both sides for text comparisons.
- Incident statuses: open, in_progress.
- Incident severities: urgent, soon, whenever.
"#;

const SQL_FEW_SHOT_EXAMPLES: &str = r#"
## Examples

User: How many projects are underway?
SQL: SELECT COUNT(*) AS count FROM projects WHERE status = 'underway' AND deleted_at IS NULL

User: Show me total spending by project status
SQL: SELECT status, SUM(actual_cents) / 100.0 AS total_dollars FROM projects WHERE deleted_at IS NULL GROUP BY status

User: Which vendors have given me the most quotes?
SQL: SELECT v.name, COUNT(q.id) AS quote_count FROM vendors v JOIN quotes q ON v.id = q.vendor_id WHERE v.deleted_at IS NULL AND q.deleted_at IS NULL GROUP BY v.id, v.name ORDER BY quote_count DESC

User: What's the average quote amount for each project type?
SQL: SELECT pt.name AS project_type, AVG(q.total_cents) / 100.0 AS avg_quote_dollars FROM project_types pt JOIN projects p ON pt.id = p.project_type_id JOIN quotes q ON p.id = q.project_id WHERE p.deleted_at IS NULL AND q.deleted_at IS NULL GROUP BY pt.id, pt.name ORDER BY avg_quote_dollars DESC

User: What open incidents do I have?
SQL: SELECT title, severity, date_noticed, location FROM incidents WHERE status IN ('open', 'in_progress') AND deleted_at IS NULL

User: How much have I spent on incidents this year?
SQL: SELECT SUM(cost_cents) / 100.0 AS total_dollars FROM incidents WHERE deleted_at IS NULL AND date_noticed >= date('now', 'start of year')
"#;

const FALLBACK_SCHEMA_NOTES: &str = r#"
## Fallback notes

- Incident statuses: open, in_progress.
- Incident severities: urgent, soon, whenever.
"#;

const FALLBACK_GUIDELINES: &str = r#"
## How to answer

- Be concise. One short paragraph or a bullet list.
- If asked to change data, tell the user to use edit mode.
- Do not invent data that is not present in the provided summary.
"#;

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
        ColumnInfo, Message, Role, SqlTokenKind, TableInfo, build_fallback_prompt,
        build_sql_prompt, build_summary_prompt, extract_sql, format_results_table, format_sql,
        tokenize_sql,
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
    fn build_sql_prompt_includes_entity_relationships_and_case_insensitive_guidance() {
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
        assert!(prompt.contains("## Entity Relationships"));
        assert!(prompt.contains("projects.project_type_id"));
        assert!(prompt.contains("maintenance_items.appliance_id"));
        assert!(prompt.contains("incidents.appliance_id"));
        assert!(prompt.contains("incidents.vendor_id"));
        assert!(prompt.contains("NO direct FK between projects and appliances"));
        assert!(prompt.contains("case-insensitive matching"));
        assert!(prompt.contains("LOWER()"));
    }

    #[test]
    fn build_sql_prompt_includes_groupby_and_incident_examples() {
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
            None,
            None,
        );
        assert!(prompt.contains("GROUP BY"));
        assert!(prompt.contains("total spending by project status"));
        assert!(prompt.contains("vendors have given me the most quotes"));
        assert!(prompt.contains("average quote amount"));
        assert!(prompt.contains("What open incidents do I have?"));
        assert!(prompt.contains("status IN ('open', 'in_progress')"));
        assert!(prompt.contains("SUM(cost_cents) / 100.0"));
    }

    #[test]
    fn build_sql_prompt_includes_incident_schema_notes() {
        let prompt = build_sql_prompt(
            &[TableInfo {
                name: "incidents".to_owned(),
                columns: vec![ColumnInfo {
                    name: "status".to_owned(),
                    column_type: "TEXT".to_owned(),
                    not_null: false,
                    primary_key: false,
                }],
            }],
            OffsetDateTime::UNIX_EPOCH,
            None,
            None,
        );
        assert!(prompt.contains("Incident statuses: open, in_progress."));
        assert!(prompt.contains("Incident severities: urgent, soon, whenever."));
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
        assert!(prompt.contains("January"));
        assert!(prompt.contains("1970"));
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
    fn build_fallback_prompt_includes_relationships_and_incident_notes() {
        let prompt = build_fallback_prompt(
            &[TableInfo {
                name: "incidents".to_owned(),
                columns: vec![ColumnInfo {
                    name: "status".to_owned(),
                    column_type: "TEXT".to_owned(),
                    not_null: false,
                    primary_key: false,
                }],
            }],
            "",
            OffsetDateTime::UNIX_EPOCH,
            None,
        );
        assert!(prompt.contains("## Entity Relationships"));
        assert!(prompt.contains("projects.project_type_id"));
        assert!(prompt.contains("NO direct FK between projects and appliances"));
        assert!(prompt.contains("Incident statuses: open, in_progress."));
        assert!(prompt.contains("Incident severities: urgent, soon, whenever."));
    }

    #[test]
    fn build_fallback_prompt_omits_current_data_heading_when_summary_is_empty() {
        let prompt = build_fallback_prompt(
            &[TableInfo {
                name: "projects".to_owned(),
                columns: vec![ColumnInfo {
                    name: "id".to_owned(),
                    column_type: "INTEGER".to_owned(),
                    not_null: true,
                    primary_key: true,
                }],
            }],
            "",
            OffsetDateTime::UNIX_EPOCH,
            None,
        );
        assert!(prompt.contains("home management app"));
        assert!(prompt.contains("January"));
        assert!(prompt.contains("1970"));
        assert!(!prompt.contains("## Current data"));
    }

    #[test]
    fn build_fallback_prompt_includes_current_data_heading_when_summary_present() {
        let prompt = build_fallback_prompt(
            &[TableInfo {
                name: "projects".to_owned(),
                columns: vec![ColumnInfo {
                    name: "id".to_owned(),
                    column_type: "INTEGER".to_owned(),
                    not_null: true,
                    primary_key: true,
                }],
            }],
            "### projects (1 rows)\n- title: Deck",
            OffsetDateTime::UNIX_EPOCH,
            None,
        );
        assert!(prompt.contains("## Current data"));
        assert!(prompt.contains("title: Deck"));
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
    fn format_sql_simple_select() {
        let got = format_sql("SELECT name, age FROM users WHERE age > 21", 0);
        let expected = "SELECT name,\n  age\nFROM users\nWHERE age > 21";
        assert_eq!(got, expected);
    }

    #[test]
    fn format_sql_single_column() {
        let got = format_sql("SELECT COUNT(*) FROM projects WHERE deleted_at IS NULL", 0);
        let expected = "SELECT COUNT(*)\nFROM projects\nWHERE deleted_at IS NULL";
        assert_eq!(got, expected);
    }

    #[test]
    fn format_sql_multiple_clauses() {
        let got = format_sql(
            "SELECT name, budget_cents / 100.0 AS budget FROM projects WHERE status = 'underway' AND deleted_at IS NULL ORDER BY budget_cents DESC LIMIT 5",
            0,
        );
        let expected = "SELECT name,\n  budget_cents / 100.0 AS budget\nFROM projects\nWHERE status = 'underway'\n  AND deleted_at IS NULL\nORDER BY budget_cents DESC\nLIMIT 5";
        assert_eq!(got, expected);
    }

    #[test]
    fn format_sql_join() {
        let got = format_sql(
            "SELECT m.name, a.name FROM maintenance_items m LEFT JOIN appliances a ON m.appliance_id = a.id WHERE m.deleted_at IS NULL",
            0,
        );
        let expected = "SELECT m.name,\n  a.name\nFROM maintenance_items m\nLEFT JOIN appliances a\nON m.appliance_id = a.id\nWHERE m.deleted_at IS NULL";
        assert_eq!(got, expected);
    }

    #[test]
    fn format_sql_group_by() {
        let got = format_sql(
            "SELECT status, COUNT(*) AS cnt FROM projects WHERE deleted_at IS NULL GROUP BY status HAVING cnt > 1 ORDER BY cnt DESC",
            0,
        );
        let expected = "SELECT status,\n  COUNT(*) AS cnt\nFROM projects\nWHERE deleted_at IS NULL\nGROUP BY status\nHAVING cnt > 1\nORDER BY cnt DESC";
        assert_eq!(got, expected);
    }

    #[test]
    fn format_sql_keywords_uppercased() {
        let got = format_sql(
            "select name from projects where status = 'underway' and deleted_at is null limit 1",
            0,
        );
        assert!(got.contains("SELECT"));
        assert!(got.contains("FROM"));
        assert!(got.contains("WHERE"));
        assert!(got.contains("AND"));
        assert!(got.contains("IS NULL"));
        assert!(got.contains("LIMIT"));
    }

    #[test]
    fn format_sql_preserves_strings() {
        let got = format_sql("SELECT * FROM projects WHERE name = 'Kitchen Remodel'", 0);
        assert!(got.contains("'Kitchen Remodel'"));
    }

    #[test]
    fn format_sql_between_clause() {
        let got = format_sql(
            "SELECT name FROM appliances WHERE warranty_expiry BETWEEN date('now') AND date('now', '+90 days')",
            0,
        );
        assert!(got.contains("BETWEEN"));
        assert!(got.contains("date('now')"));
    }

    #[test]
    fn format_sql_empty() {
        assert_eq!(format_sql("", 0), "");
    }

    #[test]
    fn format_sql_wraps_long_lines() {
        let got = format_sql(
            "SELECT name, date(last_serviced_at, '+' || interval_months || ' months') AS next_due FROM maintenance_items WHERE deleted_at IS NULL ORDER BY next_due",
            40,
        );
        for line in got.lines() {
            assert!(
                line.len() <= 50,
                "line too long (allowing grace for unbreakable tokens): {line}"
            );
        }
        assert!(got.contains("SELECT name"));
        assert!(got.contains("next_due"));
        assert!(got.contains("FROM maintenance_items"));
    }

    #[test]
    fn format_sql_wrap_preserves_indent() {
        let got = format_sql(
            "SELECT a_very_long_column_name, another_really_long_column_name, yet_another_one FROM some_table",
            30,
        );
        for line in got.lines().skip(1) {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with("FROM") {
                assert!(line.starts_with("  "), "continuation line must be indented");
            }
        }
    }

    #[test]
    fn tokenize_sql_basic() {
        let tokens = tokenize_sql("SELECT name FROM users");
        let words = filter_kind(&tokens, SqlTokenKind::Word);
        assert_eq!(words, vec!["SELECT", "name", "FROM", "users"]);
    }

    #[test]
    fn tokenize_sql_string() {
        let tokens = tokenize_sql("WHERE name = 'O''Brien'");
        let strings = filter_kind(&tokens, SqlTokenKind::String);
        assert_eq!(strings, vec!["'O''Brien'"]);
    }

    #[test]
    fn tokenize_sql_numbers() {
        let tokens = tokenize_sql("LIMIT 10 OFFSET 3.5");
        let numbers = filter_kind(&tokens, SqlTokenKind::Number);
        assert_eq!(numbers, vec!["10", "3.5"]);
    }

    #[test]
    fn tokenize_sql_operators() {
        let tokens = tokenize_sql("a >= 1 AND b <> 2");
        let symbols = filter_kind(&tokens, SqlTokenKind::Symbol);
        assert!(symbols.iter().any(|entry| entry == ">="));
        assert!(symbols.iter().any(|entry| entry == "<>"));
    }

    fn filter_kind(tokens: &[super::SqlToken], kind: SqlTokenKind) -> Vec<String> {
        tokens
            .iter()
            .filter(|token| token.kind == kind)
            .map(|token| token.text.clone())
            .collect()
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
