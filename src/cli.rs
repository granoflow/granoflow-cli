use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum Lang {
    En,
    #[value(name = "zh-CN")]
    ZhCn,
    #[value(name = "zh-TW")]
    ZhTw,
    #[value(name = "zh-HK")]
    ZhHk,
}

impl Lang {
    pub fn requested(&self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::ZhCn => "zh-CN",
            Lang::ZhTw => "zh-TW",
            Lang::ZhHk => "zh-HK",
        }
    }

    pub fn resolved(&self) -> &'static str {
        match self {
            Lang::ZhHk => "zh-TW",
            _ => self.requested(),
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "granoflow",
    version,
    about = "Granoflow Local HTTP API client",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[arg(long, global = true, env = "GRANOFLOW_API_BASE_URL")]
    pub api_base_url: Option<String>,
    #[arg(long, global = true, env = "GRANOFLOW_API_TOKEN")]
    pub token: Option<String>,
    #[arg(long, global = true, env = "GRANOFLOW_CONFIG")]
    pub config: Option<String>,
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true, value_enum, default_value_t = Lang::En)]
    pub lang: Lang,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Help(HelpArgs),
    Config,
    Health,
    Api(ApiCommand),
    Task(TaskCommand),
    Project(ProjectCommand),
    Review(ReviewCommand),
    #[command(name = "ai-agent")]
    AiAgent(AiAgentCommand),
}

#[derive(Debug, Args)]
pub struct HelpArgs {
    pub command: Vec<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ApiCommand {
    #[command(subcommand)]
    pub command: ApiSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ApiSubcommand {
    Version,
    Capabilities,
}

#[derive(Debug, Args)]
pub struct TaskCommand {
    #[command(subcommand)]
    pub command: TaskSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum TaskSubcommand {
    List,
    Create(InputArgs),
    Complete(TaskCompleteArgs),
}

#[derive(Debug, Args)]
pub struct ProjectCommand {
    #[command(subcommand)]
    pub command: ProjectSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ProjectSubcommand {
    List,
    Create(InputArgs),
}

#[derive(Debug, Args)]
pub struct TaskCompleteArgs {
    #[arg(long)]
    pub id: String,
    #[arg(long)]
    pub input: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args, Clone)]
pub struct InputArgs {
    #[arg(long)]
    pub input: String,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct ReviewCommand {
    #[command(subcommand)]
    pub period: ReviewPeriod,
}

#[derive(Debug, Subcommand)]
pub enum ReviewPeriod {
    Day(ReviewDayCommand),
    Week(ReviewWeekCommand),
}

#[derive(Debug, Args)]
pub struct ReviewDayCommand {
    #[command(subcommand)]
    pub command: ReviewDaySubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ReviewDaySubcommand {
    Show(DateArg),
    Update(DateInputArgs),
}

#[derive(Debug, Args)]
pub struct ReviewWeekCommand {
    #[command(subcommand)]
    pub command: ReviewWeekSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ReviewWeekSubcommand {
    Show(WeekArg),
    Update(WeekInputArgs),
    Value(WeekValueArgs),
}

#[derive(Debug, Args)]
pub struct DateArg {
    #[arg(long)]
    pub date: String,
}

#[derive(Debug, Args)]
pub struct WeekArg {
    #[arg(long = "week-start")]
    pub week_start: String,
}

#[derive(Debug, Args)]
pub struct DateInputArgs {
    #[arg(long)]
    pub date: String,
    #[arg(long)]
    pub input: String,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct WeekInputArgs {
    #[arg(long = "week-start")]
    pub week_start: String,
    #[arg(long)]
    pub input: String,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct WeekValueArgs {
    #[arg(long = "week-start")]
    pub week_start: String,
    #[arg(long = "value-id")]
    pub value_id: String,
    #[arg(long)]
    pub input: String,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct AiAgentCommand {
    #[command(subcommand)]
    pub command: AiAgentSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AiAgentSubcommand {
    Tools,
    Task(AiAgentTaskCommand),
}

#[derive(Debug, Args)]
pub struct AiAgentTaskCommand {
    #[command(subcommand)]
    pub command: AiAgentTaskSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AiAgentTaskSubcommand {
    Export(AiExportArgs),
    Validate(AiInputArgs),
    Import(AiInputArgs),
}

#[derive(Debug, Args)]
pub struct AiExportArgs {
    #[arg(long)]
    pub id: String,
}

#[derive(Debug, Args)]
pub struct AiInputArgs {
    #[arg(long)]
    pub input: String,
    #[arg(long)]
    pub dry_run: bool,
}
