mod auth;
mod github;
mod llm;
mod models;
mod output;
mod time_filter;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Markdown,
}

#[derive(Parser, Debug)]
#[command(name = "doner")]
#[command(about = "Summarize issues from a GitHub project board column")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Authenticate with GitHub
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },

    /// Fetch and summarize issues from a project board column
    #[command(name = "summarize", alias = "sum")]
    Summarize {
        /// GitHub Project identifier (owner/number or GraphQL node ID)
        project_id: String,

        /// Column name to fetch issues from
        #[arg(short = 'c', long = "col", default_value = "Done")]
        column: String,

        /// Filter issues by time (e.g., 7d, 24h, yesterday, this-week)
        #[arg(short = 's', long = "since")]
        since: Option<String>,

        /// Filter by iteration (e.g., @current, @previous, @current,@previous, or iteration name)
        #[arg(short = 'i', long = "iteration", default_value = "@current,@previous")]
        iteration: Option<String>,

        /// Output format
        #[arg(short = 'f', long = "format", value_enum, default_value = "text")]
        format: OutputFormat,

        /// Group issues by parent issue
        #[arg(short = 'w', long = "wrap")]
        wrap: bool,

        /// Use AI to generate a rich summary (requires OPENAI_API_KEY or ANTHROPIC_API_KEY)
        #[arg(long = "ai")]
        ai: bool,

        /// Show debug information about fetched items
        #[arg(long = "debug")]
        debug: bool,
    },
}

#[derive(Subcommand, Debug)]
enum AuthAction {
    /// Log in to GitHub (interactive)
    Login {
        /// Provide token directly instead of interactive prompt
        #[arg(long = "with-token")]
        with_token: Option<String>,

        /// Skip token validation (for testing)
        #[arg(long = "skip-validation", hide = true)]
        skip_validation: bool,
    },

    /// Log out and remove stored credentials
    Logout,

    /// Check authentication status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Auth { action } => handle_auth(action).await,
        Commands::Summarize {
            project_id,
            column,
            since,
            iteration,
            format,
            wrap,
            ai,
            debug,
        } => handle_summarize(project_id, column, since, iteration, format, wrap, ai, debug).await,
    }
}

async fn handle_auth(action: AuthAction) -> Result<()> {
    match action {
        AuthAction::Login {
            with_token,
            skip_validation,
        } => {
            let token = match with_token {
                Some(t) => t,
                None => auth::interactive_login()?,
            };

            let username = if skip_validation {
                println!("Skipping validation (test mode)");
                "test-user".to_string()
            } else {
                print!("Validating token... ");
                std::io::Write::flush(&mut std::io::stdout())?;

                let user = auth::validate_token(&token).await?;
                println!("OK");
                user
            };

            print!("Storing token... ");
            std::io::Write::flush(&mut std::io::stdout())?;

            auth::store_token(&token)?;
            println!("OK");

            println!("Logged in as {}", username);
        }

        AuthAction::Logout => {
            if auth::has_token() {
                auth::delete_token()?;
                println!("Logged out. Token removed from keychain.");
            } else {
                println!("Not logged in.");
            }
        }

        AuthAction::Status => {
            // Check environment variable first
            if std::env::var("GITHUB_TOKEN").is_ok() {
                println!("Using token from GITHUB_TOKEN environment variable");
            } else if auth::has_token() {
                let token = auth::get_token()?;
                match auth::validate_token(&token).await {
                    Ok(username) => {
                        println!("Logged in as {} (token stored in keychain)", username);
                    }
                    Err(_) => {
                        println!("Token found in keychain but appears invalid or expired.");
                        println!("Run 'doner auth login' to re-authenticate.");
                    }
                }
            } else {
                println!("Not logged in.");
                println!("Run 'doner auth login' to authenticate.");
            }
        }
    }

    Ok(())
}

async fn handle_summarize(
    project_id: String,
    column: String,
    since: Option<String>,
    iteration: Option<String>,
    format: OutputFormat,
    wrap: bool,
    ai: bool,
    debug: bool,
) -> Result<()> {
    let token = auth::resolve_token()?;

    let since_filter = since
        .as_ref()
        .map(|s| time_filter::parse_time_filter(s))
        .transpose()?;

    let client = github::GitHubClient::new(&token);

    // Resolve project ID (either direct node ID or owner/number format)
    let project_node_id = client.resolve_project_id(&project_id).await?;

    let (issues, stats) = client
        .fetch_project_issues(&project_node_id, &column, since_filter, iteration.as_deref(), debug)
        .await?;

    if debug {
        eprintln!("Debug: Project node ID: {}", project_node_id);
        eprintln!("Debug: Looking for column: \"{}\"", column);
        eprintln!("Debug: Status field: \"{}\"", std::env::var("DONER_STATUS_FIELD").unwrap_or_else(|_| "Status".to_string()));
        if let Some(ref iter) = iteration {
            eprintln!("Debug: Iteration filter: \"{}\"", iter);
        }
        eprintln!("Debug: Total items fetched: {}", stats.total_items);
        eprintln!("Debug: Archived items (skipped): {}", stats.archived);
        eprintln!("Debug: Wrong column (skipped): {}", stats.wrong_column);
        eprintln!("Debug: Not an issue (skipped): {}", stats.not_issue);
        eprintln!("Debug: Filtered by iteration (skipped): {}", stats.filtered_by_iteration);
        eprintln!("Debug: Filtered by time (skipped): {}", stats.filtered_by_time);
        eprintln!("Debug: Final count: {}", issues.len());
        if !stats.columns_seen.is_empty() {
            eprintln!("Debug: Columns seen: {:?}", stats.columns_seen);
        }
        if !stats.iterations_seen.is_empty() {
            eprintln!("Debug: Iterations seen: {:?}", stats.iterations_seen);
        }
        eprintln!();
    }

    if issues.is_empty() {
        println!("No issues found in column \"{}\"", column);
        return Ok(());
    }

    // Always compute the formatted output
    let output = if wrap {
        output::format_grouped(&issues, format)
    } else {
        output::format_list(&issues, format)
    };

    // If AI flag is set, pass the formatted output to the LLM
    if ai {
        let llm_client = llm::LlmClient::from_env()?;

        eprint!("Generating AI summary... ");
        std::io::Write::flush(&mut std::io::stderr())?;

        let summary = llm_client.summarize(&output).await?;
        eprintln!("done");
        eprintln!();

        println!("{}", summary);
    } else {
        println!("{}", output);
    }

    Ok(())
}
