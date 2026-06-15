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
    Deck(DeckCommand),
    Card(CardCommand),
    Backup(BackupCommand),
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

#[derive(Debug, Args)]
pub struct DeckCommand {
    #[command(subcommand)]
    pub command: DeckSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum DeckSubcommand {
    List,
    Show(DeckIdArg),
    Create(DeckCreateArgs),
    Delete(DeckIdArg),
    Cards(DeckCardsArgs),
    Import(DeckImportCommand),
}

#[derive(Debug, Args)]
pub struct DeckIdArg {
    pub deck_id: String,
}

#[derive(Debug, Args)]
pub struct DeckCreateArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub parent: Option<String>,
}

#[derive(Debug, Args)]
pub struct DeckCardsArgs {
    pub deck_id: String,
    #[arg(long)]
    pub include_children: bool,
    #[arg(long, conflicts_with = "active")]
    pub archived: bool,
    #[arg(long, conflicts_with = "archived")]
    pub active: bool,
}

#[derive(Debug, Args)]
pub struct DeckImportCommand {
    #[command(subcommand)]
    pub command: DeckImportSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum DeckImportSubcommand {
    Anki(DeckImportAnkiArgs),
}

#[derive(Debug, Args)]
pub struct DeckImportAnkiArgs {
    pub path: String,
    #[arg(long, conflicts_with = "confirm")]
    pub dry_run: bool,
    #[arg(long, conflicts_with = "dry_run")]
    pub confirm: Option<String>,
    #[arg(long)]
    pub skip_cards_with_missing_media: bool,
    #[arg(long)]
    pub strip_remote_media: bool,
}

#[derive(Debug, Args)]
pub struct CardCommand {
    #[command(subcommand)]
    pub command: CardSubcommand,
}

#[derive(Debug, Args)]
pub struct BackupCommand {
    #[command(subcommand)]
    pub command: BackupSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum BackupSubcommand {
    Decrypt(BackupConvertArgs),
    Encrypt(BackupConvertArgs),
}

#[derive(Debug, Args)]
pub struct BackupConvertArgs {
    #[arg(long)]
    pub input: String,
    #[arg(long)]
    pub output: String,
    #[arg(long, conflicts_with = "secret_file")]
    pub secret_env: Option<String>,
    #[arg(long, conflicts_with = "secret_env")]
    pub secret_file: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum CardSubcommand {
    Archive(CardIdArg),
    Unarchive(CardIdArg),
    Trash(CardIdArg),
    Unlink(TaskCardArg),
    #[command(name = "unlink-note")]
    UnlinkNote(TaskNoteArg),
}

#[derive(Debug, Args)]
pub struct CardIdArg {
    pub card_id: String,
}

#[derive(Debug, Args)]
pub struct TaskCardArg {
    #[arg(long)]
    pub task_id: String,
    #[arg(long)]
    pub card_id: String,
}

#[derive(Debug, Args)]
pub struct TaskNoteArg {
    #[arg(long)]
    pub task_id: String,
    #[arg(long)]
    pub note_id: String,
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

#[derive(Debug, Args)]
pub struct AiTaskInputArgs {
    #[arg(long)]
    pub task_id: String,
    #[arg(long)]
    pub input: String,
    #[arg(long)]
    pub dry_run: bool,
}
