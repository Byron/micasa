// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Result, anyhow};
use micasa_llm::{Client, Message, Role};
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Response, Server};

#[test]
fn ping_error_contains_actionable_remediation() {
    let client = Client::new("http://127.0.0.1:1/v1", "qwen3", Duration::from_millis(50))
        .expect("client should initialize");

    let error = client
        .ping()
        .expect_err("ping should fail for unreachable endpoint");
    let message = error.to_string();
    assert!(message.contains("ollama serve"));
}

#[test]
fn list_models_and_ping_work_against_mock_server() -> Result<()> {
    let server =
        Server::http("127.0.0.1:0").map_err(|error| anyhow!("start mock server: {error}"))?;
    let addr = format!("http://{}/v1", server.server_addr());

    let handle = thread::spawn(move || {
        for _ in 0..2 {
            let request = server.recv().expect("request expected");
            assert_eq!(request.url(), "/v1/models");
            let response = Response::from_string(r#"{"data":[{"id":"qwen3"}]}"#)
                .with_status_code(200)
                .with_header(
                    Header::from_bytes("Content-Type", "application/json")
                        .expect("valid content type header"),
                );
            request.respond(response).expect("response should succeed");
        }
    });

    let client = Client::new(&addr, "qwen3", Duration::from_secs(1))?;
    let models = client.list_models()?;
    assert_eq!(models, vec!["qwen3".to_owned()]);
    client.ping()?;

    handle.join().expect("server thread should join");
    Ok(())
}

#[test]
fn chat_stream_parses_server_sent_events() -> Result<()> {
    let server =
        Server::http("127.0.0.1:0").map_err(|error| anyhow!("start mock server: {error}"))?;
    let addr = format!("http://{}/v1", server.server_addr());

    let handle = thread::spawn(move || {
        let request = server.recv().expect("request expected");
        assert_eq!(request.url(), "/v1/chat/completions");

        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":\"stop\"}]}\n",
            "data: [DONE]\n",
        );
        let response = Response::from_string(body)
            .with_status_code(200)
            .with_header(
                Header::from_bytes("Content-Type", "text/event-stream")
                    .expect("valid content type header"),
            );
        request.respond(response).expect("response should succeed");
    });

    let client = Client::new(&addr, "qwen3", Duration::from_secs(1))?;
    let mut stream = client.chat_stream(&[Message {
        role: Role::User,
        content: "Say hi".to_owned(),
    }])?;

    let first = stream.next().expect("first chunk should exist")?;
    assert_eq!(first.content, "Hello");
    assert!(!first.done);

    let second = stream.next().expect("second chunk should exist")?;
    assert_eq!(second.content, " world");
    assert!(second.done);

    assert!(stream.next().is_none());

    handle.join().expect("server thread should join");
    Ok(())
}

#[test]
fn chat_stream_handles_partial_tokens_and_done_chunks() -> Result<()> {
    let server =
        Server::http("127.0.0.1:0").map_err(|error| anyhow!("start mock server: {error}"))?;
    let addr = format!("http://{}/v1", server.server_addr());

    let handle = thread::spawn(move || {
        let request = server.recv().expect("request expected");
        assert_eq!(request.url(), "/v1/chat/completions");

        let body = concat!(
            "event: message\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"\"},\"finish_reason\":null}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Par\"},\"finish_reason\":null}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"tial\"},\"finish_reason\":null}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"\"},\"finish_reason\":\"stop\"}]}\n",
            "data: [DONE]\n",
        );
        let response = Response::from_string(body)
            .with_status_code(200)
            .with_header(
                Header::from_bytes("Content-Type", "text/event-stream")
                    .expect("valid content type header"),
            );
        request.respond(response).expect("response should succeed");
    });

    let client = Client::new(&addr, "qwen3", Duration::from_secs(1))?;
    let mut stream = client.chat_stream(&[Message {
        role: Role::User,
        content: "Say partial".to_owned(),
    }])?;

    let first = stream.next().expect("first chunk should exist")?;
    assert_eq!(first.content, "Par");
    assert!(!first.done);

    let second = stream.next().expect("second chunk should exist")?;
    assert_eq!(second.content, "tial");
    assert!(!second.done);

    let done = stream.next().expect("done chunk should exist")?;
    assert!(done.content.is_empty());
    assert!(done.done);

    assert!(stream.next().is_none());

    handle.join().expect("server thread should join");
    Ok(())
}

#[test]
fn ping_fails_actionably_when_model_is_missing() -> Result<()> {
    let server =
        Server::http("127.0.0.1:0").map_err(|error| anyhow!("start mock server: {error}"))?;
    let addr = format!("http://{}/v1", server.server_addr());

    let handle = thread::spawn(move || {
        let request = server.recv().expect("request expected");
        assert_eq!(request.url(), "/v1/models");
        let response = Response::from_string(r#"{"data":[{"id":"llama3"}]}"#)
            .with_status_code(200)
            .with_header(
                Header::from_bytes("Content-Type", "application/json")
                    .expect("valid content type header"),
            );
        request.respond(response).expect("response should succeed");
    });

    let client = Client::new(&addr, "qwen3", Duration::from_secs(1))?;
    let error = client
        .ping()
        .expect_err("ping should fail when model is missing");
    let message = error.to_string();
    assert!(message.contains("not found"));
    assert!(message.contains("ollama pull qwen3"));

    handle.join().expect("server thread should join");
    Ok(())
}

#[test]
fn list_models_allows_empty_server_response() -> Result<()> {
    let server =
        Server::http("127.0.0.1:0").map_err(|error| anyhow!("start mock server: {error}"))?;
    let addr = format!("http://{}/v1", server.server_addr());

    let handle = thread::spawn(move || {
        let request = server.recv().expect("request expected");
        assert_eq!(request.url(), "/v1/models");
        let response = Response::from_string(r#"{"data":[]}"#)
            .with_status_code(200)
            .with_header(
                Header::from_bytes("Content-Type", "application/json")
                    .expect("valid content type header"),
            );
        request.respond(response).expect("response should succeed");
    });

    let client = Client::new(&addr, "qwen3", Duration::from_secs(1))?;
    let models = client.list_models()?;
    assert!(models.is_empty());

    handle.join().expect("server thread should join");
    Ok(())
}

#[test]
fn chat_complete_returns_single_choice_content() -> Result<()> {
    let server =
        Server::http("127.0.0.1:0").map_err(|error| anyhow!("start mock server: {error}"))?;
    let addr = format!("http://{}/v1", server.server_addr());

    let handle = thread::spawn(move || {
        let request = server.recv().expect("request expected");
        assert_eq!(request.url(), "/v1/chat/completions");
        let response =
            Response::from_string(r#"{"choices":[{"message":{"content":"Two active projects"}}]}"#)
                .with_status_code(200)
                .with_header(
                    Header::from_bytes("Content-Type", "application/json")
                        .expect("valid content type header"),
                );
        request.respond(response).expect("response should succeed");
    });

    let client = Client::new(&addr, "qwen3", Duration::from_secs(1))?;
    let answer = client.chat_complete(&[Message {
        role: Role::User,
        content: "How many active projects?".to_owned(),
    }])?;
    assert_eq!(answer, "Two active projects");

    handle.join().expect("server thread should join");
    Ok(())
}

#[test]
fn chat_complete_server_errors_are_cleaned() -> Result<()> {
    let server =
        Server::http("127.0.0.1:0").map_err(|error| anyhow!("start mock server: {error}"))?;
    let addr = format!("http://{}/v1", server.server_addr());

    let handle = thread::spawn(move || {
        let request = server.recv().expect("request expected");
        assert_eq!(request.url(), "/v1/chat/completions");
        let response = Response::from_string(r#"{"error":{"message":"bad request"}}"#)
            .with_status_code(400)
            .with_header(
                Header::from_bytes("Content-Type", "application/json")
                    .expect("valid content type header"),
            );
        request.respond(response).expect("response should succeed");
    });

    let client = Client::new(&addr, "qwen3", Duration::from_secs(1))?;
    let error = client
        .chat_complete(&[Message {
            role: Role::User,
            content: "bad prompt".to_owned(),
        }])
        .expect_err("chat_complete should surface server error");
    assert!(
        error
            .to_string()
            .contains("server error (400): bad request")
    );

    handle.join().expect("server thread should join");
    Ok(())
}

#[test]
fn chat_complete_rejects_empty_choices() -> Result<()> {
    let server =
        Server::http("127.0.0.1:0").map_err(|error| anyhow!("start mock server: {error}"))?;
    let addr = format!("http://{}/v1", server.server_addr());

    let handle = thread::spawn(move || {
        let request = server.recv().expect("request expected");
        assert_eq!(request.url(), "/v1/chat/completions");
        let response = Response::from_string(r#"{"choices":[]}"#)
            .with_status_code(200)
            .with_header(
                Header::from_bytes("Content-Type", "application/json")
                    .expect("valid content type header"),
            );
        request.respond(response).expect("response should succeed");
    });

    let client = Client::new(&addr, "qwen3", Duration::from_secs(1))?;
    let error = client
        .chat_complete(&[Message {
            role: Role::User,
            content: "hello".to_owned(),
        }])
        .expect_err("empty choices should fail");
    assert!(error.to_string().contains("no choices in chat response"));

    handle.join().expect("server thread should join");
    Ok(())
}

#[test]
fn list_models_clean_error_response_handles_ollama_and_plain_text() -> Result<()> {
    let server =
        Server::http("127.0.0.1:0").map_err(|error| anyhow!("start mock server: {error}"))?;
    let addr = format!("http://{}/v1", server.server_addr());

    let handle = thread::spawn(move || {
        let request_one = server.recv().expect("request expected");
        assert_eq!(request_one.url(), "/v1/models");
        let response_one = Response::from_string(r#"{"error":"model index unavailable"}"#)
            .with_status_code(500)
            .with_header(
                Header::from_bytes("Content-Type", "application/json")
                    .expect("valid content type header"),
            );
        request_one
            .respond(response_one)
            .expect("response should succeed");

        let request_two = server.recv().expect("request expected");
        assert_eq!(request_two.url(), "/v1/models");
        let response_two = Response::from_string("internal meltdown")
            .with_status_code(502)
            .with_header(
                Header::from_bytes("Content-Type", "text/plain")
                    .expect("valid content type header"),
            );
        request_two
            .respond(response_two)
            .expect("response should succeed");
    });

    let client = Client::new(&addr, "qwen3", Duration::from_secs(1))?;
    let first = client
        .list_models()
        .expect_err("ollama-style error payload should fail");
    assert!(
        first
            .to_string()
            .contains("server error (500): model index unavailable")
    );

    let second = client
        .list_models()
        .expect_err("plain text error payload should fail");
    assert!(
        second
            .to_string()
            .contains("server error (502): internal meltdown")
    );

    handle.join().expect("server thread should join");
    Ok(())
}

#[test]
fn model_and_base_url_accessors_and_setter_work() -> Result<()> {
    let mut client = Client::new(
        "http://localhost:11434/v1/",
        "qwen3",
        Duration::from_secs(1),
    )?;
    assert_eq!(client.base_url(), "http://localhost:11434/v1");
    assert_eq!(client.model(), "qwen3");
    client.set_model("qwen3:32b");
    assert_eq!(client.model(), "qwen3:32b");
    Ok(())
}
