// src/main.rs

use clap::Parser;
use env_logger::{Builder, Env};
use eyre::{eyre, Result};
use lazy_static::lazy_static;
use log::{debug, info};
use reqwest::blocking::Client;
use serde_json::json;
use shellexpand::tilde;
use std::env;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

lazy_static! {
    static ref CHATGPT_API_KEY: String = env::var("CHATGPT_API_KEY").expect("CHATGPT_API_KEY not set in environment");
    static ref RUST_LOG: String = env::var("RUST_LOG").unwrap_or_else(|_| "WARNING".to_string());
}

#[derive(Parser, Debug)]
#[command(name = "nerf", version = env!("GIT_DESCRIBE"), about = "AI-powered text processing tool")]
struct Cli {
    #[arg(required = true)]
    words: Vec<String>,

    #[arg(short, long, default_value = "~/.config/nerf/prompt")]
    prompt: String,
}

fn pretty_print_json(json_str: &str) -> Result<(), serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(json_str)?;
    // This creates a pretty-printed JSON string.
    let pretty = serde_json::to_string_pretty(&value)?;
    println!("{}", pretty);
    Ok(())
}

fn test_api_key(api_key: &str) -> eyre::Result<()> {
    let client = Client::new();
    let response = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()?;

    // Check if the request was successful
    if response.status().is_success() {
        let response_text = response.text()?;
        // Pretty print the JSON.
        pretty_print_json(&response_text).unwrap();
        Ok(())
    } else {
        Err(eyre::eyre!("API key test failed with status: {}", response.status()))
    }
}

fn main() -> Result<()> {
    init_logger();

    test_api_key(&CHATGPT_API_KEY)?;

    let cli = Cli::parse();
    let input = cli.words.join(" ");
    info!("Input sentence(s): {}", input);

    let prompt_template = load_prompt(&cli.prompt)?;
    debug!("Loaded prompt template: {}", prompt_template);

    let prompt = prompt_template.replace("{input}", &input);
    debug!("Final prompt to send: {}", prompt);

    println!("{}", "*".repeat(80));

    let reworded = send_to_chatgpt(&prompt)?;
    println!("{}", reworded);

    info!("Copying reworded sentence(s) to clipboard");
    copy_to_clipboard(&reworded)?;

    Ok(())
}

fn load_prompt(file_path: &str) -> Result<String> {
    let expanded_path = tilde(file_path);
    fs::read_to_string(expanded_path.as_ref())
        .map_err(|e| eyre!("Failed to read prompt file '{}': {}", expanded_path, e))
}

fn send_to_chatgpt(prompt: &str) -> Result<String> {
    let request_body = json!({
        //"model": "gpt-3.5-turbo",
        "model": "gpt-3.5-turbo-16k",
        "messages": [
            { "role": "system", "content": "You are a helpful assistant. When transforming statements, preserve all URLs, `@handles`, and `#channels` exactly as they are, without any modifications. Do not include these instructions in your output." },
            { "role": "user", "content": prompt }
        ]
    });

    debug!("Sending request body: {}", request_body);

    let client = Client::new();
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", CHATGPT_API_KEY.as_str()))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .map_err(|e| eyre!("Failed to send request: {}", e))?;

    if !response.status().is_success() {
        return Err(eyre!(
            "ChatGPT API call failed with status: {}",
            response.status()
        ));
    }

    let response_text = response.text().map_err(|e| eyre!("Failed to read response text: {}", e))?;
    debug!("ChatGPT API raw response: {}", response_text);

    let reworded = extract_reworded_text(&response_text)?;
    info!("Reworded sentence(s): {}", reworded);

    Ok(reworded)
}

fn extract_reworded_text(response: &str) -> Result<String> {
    let response_json: serde_json::Value = serde_json::from_str(response)
        .map_err(|e| eyre!("Failed to parse API response as JSON: {}", e))?;

    response_json["choices"]
        .get(0)
        .and_then(|choice| choice["message"]["content"].as_str())
        .map(|content| content.to_string())
        .ok_or_else(|| eyre!("Failed to extract reworded text from response"))
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut process = Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|_| eyre!("Failed to start xclip. Is it installed?"))?;

    if let Some(stdin) = process.stdin.as_mut() {
        stdin.write_all(text.as_bytes())?;
    } else {
        return Err(eyre!("Failed to access stdin for xclip"));
    }

    let status = process.wait()?;
    if !status.success() {
        return Err(eyre!("xclip process failed with status: {}", status));
    }

    Ok(())
}

fn init_logger() {
    Builder::from_env(Env::default().default_filter_or(RUST_LOG.as_str())).init();
}
