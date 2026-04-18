use clap::{CommandFactory, Parser, Subcommand};

mod commands;
#[allow(dead_code)]
mod output;

#[derive(Parser)]
#[command(
    name = "librarian",
    version,
    about = "Organise files using rules and AI"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose (DEBUG-level) logging
    #[arg(long, global = true)]
    verbose: bool,

    /// Output as JSON lines for scripting
    #[arg(long, global = true)]
    json: bool,

    /// Suppress all output except errors
    #[arg(long, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold configuration and folder structure
    Init,

    /// Scan inbox folders, classify files, produce a plan
    Process {
        /// Inbox folders to scan (repeatable)
        #[arg(long, value_name = "PATH")]
        source: Vec<std::path::PathBuf>,

        /// Target root directory
        #[arg(long, value_name = "PATH")]
        destination: Option<std::path::PathBuf>,

        /// AI provider: lmstudio or openai
        #[arg(long, value_name = "PROVIDER")]
        provider: Option<String>,

        /// Model for chat completions
        #[arg(long, value_name = "MODEL")]
        llm_model: Option<String>,

        /// Model for embeddings
        #[arg(long, value_name = "MODEL")]
        embed_model: Option<String>,

        /// Rules file path
        #[arg(long, value_name = "PATH")]
        rules: Option<std::path::PathBuf>,

        /// Override all confidence thresholds
        #[arg(long, value_name = "FLOAT")]
        threshold: Option<f64>,

        /// Generate plan without applying (default: true)
        #[arg(long, default_value_t = true)]
        dry_run: bool,

        /// Name for the saved plan
        #[arg(long, value_name = "NAME")]
        plan_name: Option<String>,

        /// Also propose renames
        #[arg(long)]
        rename: bool,
    },

    /// Execute a previously generated plan
    Apply {
        /// Plan name or path
        #[arg(long, value_name = "NAME")]
        plan: Option<String>,

        /// Copy originals to backup before moves
        #[arg(long)]
        backup: bool,

        /// Allow moves without keeping source copy (requires prior --backup)
        #[arg(long)]
        aggressive: bool,

        /// Show what would happen without executing
        #[arg(long)]
        dry_run: bool,
    },

    /// Reverse an applied plan
    Rollback {
        /// Plan to rollback (defaults to most recent applied)
        #[arg(long, value_name = "NAME")]
        plan: Option<String>,
    },

    /// List plans, recent runs, pending reviews
    Status,

    /// Manage named plans
    Plans {
        #[command(subcommand)]
        action: Option<PlansAction>,
    },

    /// Validate or suggest rules
    Rules {
        #[command(subcommand)]
        action: RulesAction,
    },

    /// Record an explicit correction
    Correct {
        /// Path to the file to correct
        file: std::path::PathBuf,

        /// Correct destination path
        #[arg(long, value_name = "PATH")]
        to: Option<std::path::PathBuf>,

        /// Correct tags (comma-separated)
        #[arg(long, value_name = "TAGS")]
        retag: Option<String>,
    },

    /// Suggest a folder structure and rules using AI
    SuggestStructure {
        /// Inbox folders to scan (repeatable)
        #[arg(long, value_name = "PATH")]
        source: Vec<std::path::PathBuf>,

        /// Target root directory
        #[arg(long, value_name = "PATH")]
        destination: Option<std::path::PathBuf>,

        /// AI provider: lmstudio or openai
        #[arg(long, value_name = "PROVIDER")]
        provider: Option<String>,

        /// Model for chat completions
        #[arg(long, value_name = "MODEL")]
        llm_model: Option<String>,

        /// Model for embeddings
        #[arg(long, value_name = "MODEL")]
        embed_model: Option<String>,

        /// Maximum number of files to analyse
        #[arg(long, value_name = "COUNT", default_value = "200")]
        max_files: Option<usize>,

        /// Create the suggested folder structure
        #[arg(long)]
        apply_folders: bool,

        /// Write suggested rules to rules.yaml
        #[arg(long)]
        apply_rules: bool,
    },

    /// Watch destination for manual corrections (runs until interrupted)
    Watch,

    /// Interactive review of needs-review folder
    Review,

    /// Show or edit configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
enum PlansAction {
    /// Show plan details
    Show {
        /// Plan name
        name: String,
    },
    /// Delete a plan
    Delete {
        /// Plan name
        name: String,
    },
    /// Remove plans older than N days (default: 30)
    Clean {
        /// Max age in days
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
}

#[derive(Subcommand)]
enum RulesAction {
    /// Validate a rules file
    Validate {
        /// Rules file path
        #[arg(long, value_name = "PATH")]
        rules: Option<std::path::PathBuf>,
    },
    /// Emit proposed rules from correction history
    Suggest,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Print current configuration
    Show,
    /// Open config in $EDITOR
    Edit,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Validate mutual exclusion of output flags
    let output_flags = [cli.verbose, cli.json, cli.quiet];
    if output_flags.iter().filter(|&&f| f).count() > 1 {
        anyhow::bail!("--verbose, --json, and --quiet are mutually exclusive");
    }

    output::init_tracing(cli.verbose, cli.json, cli.quiet)?;

    match cli.command {
        Commands::Init => commands::init::run().await,
        Commands::Process {
            source,
            destination,
            provider,
            llm_model,
            embed_model,
            rules,
            threshold,
            dry_run,
            plan_name,
            rename,
        } => {
            commands::process::run(
                source,
                destination,
                provider,
                llm_model,
                embed_model,
                rules,
                threshold,
                dry_run,
                plan_name,
                rename,
            )
            .await
        }
        Commands::Apply {
            plan,
            backup,
            aggressive,
            dry_run,
        } => commands::apply::run(plan, backup, aggressive, dry_run).await,
        Commands::Rollback { plan } => commands::rollback::run(plan).await,
        Commands::Status => commands::status::run().await,
        Commands::Plans { action } => match action {
            Some(PlansAction::Show { name }) => commands::plans::show(&name).await,
            Some(PlansAction::Delete { name }) => commands::plans::delete(&name).await,
            Some(PlansAction::Clean { days }) => commands::plans::clean(days).await,
            None => commands::plans::list().await,
        },
        Commands::Rules { action } => match action {
            RulesAction::Validate { rules } => commands::rules::validate(rules).await,
            RulesAction::Suggest => commands::rules::suggest().await,
        },
        Commands::Correct { file, to, retag } => commands::correct::run(file, to, retag).await,
        Commands::SuggestStructure {
            source,
            destination,
            provider,
            llm_model,
            embed_model,
            max_files,
            apply_folders,
            apply_rules,
        } => {
            commands::suggest::run(
                source,
                destination,
                provider,
                llm_model,
                embed_model,
                max_files,
                apply_folders,
                apply_rules,
            )
            .await
        }
        Commands::Watch => commands::watch::run().await,
        Commands::Review => commands::review::run().await,
        Commands::Config { action } => match action {
            ConfigAction::Show => commands::config::show().await,
            ConfigAction::Edit => commands::config::edit().await,
        },
        Commands::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "librarian",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    }
}
