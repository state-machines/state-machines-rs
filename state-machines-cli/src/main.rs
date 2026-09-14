//! CLI tool for state-machines visualization and introspection.
//!
//! # Usage
//!
//! Convert JSON schema to Mermaid:
//! ```bash
//! echo '{"name":"Door","initial":"Closed",...}' | state-machines render mermaid
//! ```
//!
//! Validate JSON schema:
//! ```bash
//! cat schema.json | state-machines validate
//! ```
//!
//! Format JSON schema:
//! ```bash
//! cat schema.json | state-machines render json
//! ```

use clap::{Parser, Subcommand};
use state_machines_core::schema::MachineSchema;
use std::io::{self, Read};

#[derive(Parser)]
#[command(name = "state-machines")]
#[command(about = "CLI tool for state-machines visualization and introspection")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Render a state machine schema to various formats
    Render {
        /// Output format: mermaid, json
        #[arg(value_name = "FORMAT")]
        format: String,

        /// Input JSON file (reads from stdin if not provided)
        #[arg(short, long)]
        input: Option<String>,
    },

    /// Validate a state machine schema
    Validate {
        /// Input JSON file (reads from stdin if not provided)
        #[arg(short, long)]
        input: Option<String>,
    },

    /// Show example schema
    Example,
}

fn read_input(input: Option<String>) -> io::Result<String> {
    match input {
        Some(path) => std::fs::read_to_string(path),
        None => {
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer)?;
            Ok(buffer)
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Render { format, input } => {
            let json = read_input(input)?;
            let schema: MachineSchema = serde_json::from_str(&json)?;

            match format.as_str() {
                "mermaid" => {
                    println!("{}", schema.to_mermaid());
                }
                "json" => {
                    println!("{}", schema.to_json_pretty());
                }
                other => {
                    eprintln!("Unknown format: {}. Supported: mermaid, json", other);
                    std::process::exit(1);
                }
            }
        }

        Commands::Validate { input } => {
            let json = read_input(input)?;
            match serde_json::from_str::<MachineSchema>(&json) {
                Ok(schema) => {
                    println!("✓ Valid schema: {}", schema.name);
                    println!("  States: {}", schema.states.len());
                    println!("  Events: {}", schema.events.len());
                    if !schema.superstates.is_empty() {
                        println!("  Superstates: {}", schema.superstates.len());
                    }
                }
                Err(e) => {
                    eprintln!("✗ Invalid schema: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Example => {
            use state_machines_core::schema::{EventSchema, TransitionSchema};

            let simple_event = |name: &str, from: &str, to: &str| EventSchema {
                name: name.into(),
                transitions: vec![TransitionSchema {
                    sources: vec![from.into()],
                    target: to.into(),
                    ..TransitionSchema::default()
                }],
                ..EventSchema::default()
            };

            let example = MachineSchema {
                name: "Door".into(),
                initial: "Closed".into(),
                states: vec!["Open".into(), "Closed".into(), "Locked".into()],
                superstates: vec![],
                events: vec![
                    simple_event("open", "Closed", "Open"),
                    simple_event("close", "Open", "Closed"),
                    simple_event("lock", "Closed", "Locked"),
                    simple_event("unlock", "Locked", "Closed"),
                ],
                async_mode: false,
            };

            println!("Example JSON Schema:");
            println!("{}", example.to_json_pretty());
            println!("\nExample Mermaid Output:");
            println!("{}", example.to_mermaid());
        }
    }

    Ok(())
}
