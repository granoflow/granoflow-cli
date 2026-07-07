use std::{fs, io};

use clap::Parser;
use serde_json::{json, Value};

use crate::{
    anki, backup,
    cli::*,
    client::{request_preview, ApiClient},
    config::RuntimeConfig,
    errors::{CliError, CliResult},
    help_catalog,
    output::{print_json, Envelope},
};

pub async fn run() -> anyhow::Result<i32> {
    let cli = Cli::parse();
    let json_mode = cli.json || matches!(&cli.command, Some(Command::Help(args)) if args.json);
    match run_inner(&cli).await {
        Ok(value) => {
            if json_mode {
                Ok(print_json(&Envelope::success(value)))
            } else {
                println!("{}", human_value(value));
                Ok(0)
            }
        }
        Err(error) => {
            if json_mode {
                Ok(print_json(&Envelope::error(&error)))
            } else {
                eprintln!("{error}");
                Ok(error.exit_code())
            }
        }
    }
}

async fn run_inner(cli: &Cli) -> CliResult<Value> {
    if let Some(Command::Backup(backup)) = &cli.command {
        return backup::run_backup(backup);
    }
    if let Some(Command::Deck(DeckCommand {
        command: DeckSubcommand::Anki(anki),
    })) = &cli.command
    {
        return anki::run_deck_anki(anki);
    }
    let config = RuntimeConfig::load(cli)?;
    let client = ApiClient::new(config.clone());
    match &cli.command {
        None => Ok(json!({"help": help_catalog::human_help(&cli.lang, &[])})),
        Some(Command::Help(args)) => {
            if args.json || cli.json {
                Ok(help_catalog::json_help(&cli.lang, &args.command))
            } else {
                Ok(json!({"help": help_catalog::human_help(&cli.lang, &args.command)}))
            }
        }
        Some(Command::Config) => Ok(config.redacted_json()),
        Some(Command::Health) => client.get("/v1/health").await,
        Some(Command::Api(api)) => run_api(&client, api).await,
        Some(Command::Task(task)) => run_task(&client, task).await,
        Some(Command::Project(project)) => run_project(&client, project).await,
        Some(Command::Milestone(milestone)) => run_milestone(&client, milestone).await,
        Some(Command::Review(review)) => run_review(&client, review).await,
        Some(Command::Deck(deck)) => run_deck(&client, deck).await,
        Some(Command::Card(card)) => run_card(&client, card).await,
        Some(Command::Backup(_)) => unreachable!("backup is handled before config loading"),
        Some(Command::AiAgent(ai_agent)) => run_ai_agent(&client, ai_agent).await,
    }
}

async fn run_api(client: &ApiClient, api: &ApiCommand) -> CliResult<Value> {
    match &api.command {
        ApiSubcommand::Version => client.get("/v1/version").await,
        ApiSubcommand::Capabilities => client.get("/v1/capabilities").await,
        ApiSubcommand::Sync(sync) => run_api_sync(client, sync).await,
        ApiSubcommand::Backup(backup) => run_api_backup(client, backup).await,
        ApiSubcommand::Test(test) => run_api_test(client, test).await,
    }
}

async fn run_api_sync(client: &ApiClient, sync: &ApiSyncCommand) -> CliResult<Value> {
    match &sync.command {
        ApiSyncSubcommand::Status => client.get("/v1/sync/status").await,
        ApiSyncSubcommand::Push(args) => {
            write_or_preview(client, "POST", "/v1/sync/push", json!({}), args.dry_run).await
        }
        ApiSyncSubcommand::Pull(args) => {
            write_or_preview(client, "POST", "/v1/sync/pull", json!({}), args.dry_run).await
        }
    }
}

async fn run_api_backup(client: &ApiClient, backup: &ApiBackupCommand) -> CliResult<Value> {
    match &backup.command {
        ApiBackupSubcommand::Export(args) => {
            write_or_preview(
                client,
                "POST",
                "/v1/backup/exports",
                json!({"outputPath": args.output}),
                args.dry_run,
            )
            .await
        }
        ApiBackupSubcommand::Preview(args) => {
            write_or_preview(
                client,
                "POST",
                "/v1/backup/imports/preview",
                json!({"inputPath": args.input}),
                args.dry_run,
            )
            .await
        }
        ApiBackupSubcommand::Restore(args) => {
            write_or_preview(
                client,
                "POST",
                "/v1/backup/imports",
                json!({
                    "inputPath": args.input,
                    "secretFile": args.secret_file,
                    "allowMissingAttachments": args.allow_missing_attachments,
                    "allowLargeAttachmentConversion": args.allow_large_attachment_conversion,
                    "confirm": args.confirm,
                }),
                args.dry_run,
            )
            .await
        }
    }
}

async fn run_api_test(client: &ApiClient, test: &ApiTestCommand) -> CliResult<Value> {
    match &test.command {
        ApiTestSubcommand::Login(args) => {
            write_or_preview(
                client,
                "POST",
                "/v1/commands",
                json!({
                    "command": "test-login",
                    "arguments": {"user": args.user},
                }),
                args.dry_run,
            )
            .await
        }
        ApiTestSubcommand::SeedSyncBackupCoverage(args) => {
            write_or_preview(
                client,
                "POST",
                "/v1/commands",
                json!({
                    "command": "test-seed-sync-backup-coverage",
                    "arguments": {"run_id": args.run_id},
                }),
                args.dry_run,
            )
            .await
        }
        ApiTestSubcommand::AssertSyncBackupCoverage(args) => {
            write_or_preview(
                client,
                "POST",
                "/v1/commands",
                json!({
                    "command": "test-assert-sync-backup-coverage",
                    "arguments": {"run_id": args.run_id},
                }),
                args.dry_run,
            )
            .await
        }
    }
}

async fn run_task(client: &ApiClient, task: &TaskCommand) -> CliResult<Value> {
    match &task.command {
        TaskSubcommand::List => client.get("/v1/tasks").await,
        TaskSubcommand::Create(args) => {
            let body = read_json_input(&args.input)?;
            if args.dry_run {
                Ok(request_preview("POST", "/v1/tasks", body))
            } else {
                client.post("/v1/tasks", body).await
            }
        }
        TaskSubcommand::Complete(args) => {
            let body = args
                .input
                .as_ref()
                .map(|path| read_json_input(path))
                .transpose()?
                .unwrap_or_else(|| json!({}));
            let path = format!("/v1/tasks/{}/complete", args.id);
            if args.dry_run {
                Ok(request_preview("POST", &path, body))
            } else {
                client.post(&path, body).await
            }
        }
        TaskSubcommand::Image(command) => run_attachment(client, "tasks", "images", command).await,
        TaskSubcommand::Pdf(command) => run_attachment(client, "tasks", "pdfs", command).await,
        TaskSubcommand::Attachment(command) => {
            run_attachment(client, "tasks", "attachments", command).await
        }
    }
}

async fn run_project(client: &ApiClient, project: &ProjectCommand) -> CliResult<Value> {
    match &project.command {
        ProjectSubcommand::List => client.get("/v1/projects").await,
        ProjectSubcommand::Create(args) => {
            let body = read_json_input(&args.input)?;
            if args.dry_run {
                Ok(request_preview("POST", "/v1/projects", body))
            } else {
                client.post("/v1/projects", body).await
            }
        }
        ProjectSubcommand::Image(command) => {
            run_attachment(client, "projects", "images", command).await
        }
        ProjectSubcommand::Pdf(command) => {
            run_attachment(client, "projects", "pdfs", command).await
        }
        ProjectSubcommand::Attachment(command) => {
            run_attachment(client, "projects", "attachments", command).await
        }
    }
}

async fn run_milestone(client: &ApiClient, milestone: &MilestoneCommand) -> CliResult<Value> {
    match &milestone.command {
        MilestoneSubcommand::List => client.get("/v1/milestones").await,
        MilestoneSubcommand::Create(args) => {
            let body = read_json_input(&args.input)?;
            if args.dry_run {
                Ok(request_preview("POST", "/v1/milestones", body))
            } else {
                client.post("/v1/milestones", body).await
            }
        }
        MilestoneSubcommand::Image(command) => {
            run_attachment(client, "milestones", "images", command).await
        }
        MilestoneSubcommand::Pdf(command) => {
            run_attachment(client, "milestones", "pdfs", command).await
        }
        MilestoneSubcommand::Attachment(command) => {
            run_attachment(client, "milestones", "attachments", command).await
        }
    }
}

async fn run_attachment(
    client: &ApiClient,
    resource: &str,
    attachment_resource: &str,
    command: &AttachmentCommand,
) -> CliResult<Value> {
    match &command.command {
        AttachmentSubcommand::List(args) => {
            client
                .get(&format!(
                    "/v1/{}/{}/{}",
                    resource, args.id, attachment_resource
                ))
                .await
        }
        AttachmentSubcommand::Add(args) => {
            let path = format!("/v1/{}/{}/{}", resource, args.id, attachment_resource);
            let body = json!({"file": args.file});
            if args.dry_run {
                Ok(request_preview("POST", &path, body))
            } else {
                client.post(&path, body).await
            }
        }
        AttachmentSubcommand::Delete(args) => {
            let path = format!(
                "/v1/{}/{}/{}/{}",
                resource, args.id, attachment_resource, args.attachment_id
            );
            if args.dry_run {
                Ok(request_preview("DELETE", &path, json!({})))
            } else {
                client.delete(&path).await
            }
        }
    }
}

async fn run_review(client: &ApiClient, review: &ReviewCommand) -> CliResult<Value> {
    match &review.period {
        ReviewPeriod::Day(day) => match &day.command {
            ReviewDaySubcommand::Show(args) => {
                client.get(&format!("/v1/reviews/days/{}", args.date)).await
            }
            ReviewDaySubcommand::Update(args) => {
                write_or_preview(
                    client,
                    "PATCH",
                    &format!("/v1/reviews/days/{}", args.date),
                    read_json_input(&args.input)?,
                    args.dry_run,
                )
                .await
            }
        },
        ReviewPeriod::Week(week) => match &week.command {
            ReviewWeekSubcommand::Show(args) => {
                client
                    .get(&format!("/v1/reviews/weeks/{}", args.week_start))
                    .await
            }
            ReviewWeekSubcommand::Update(args) => {
                write_or_preview(
                    client,
                    "PATCH",
                    &format!("/v1/reviews/weeks/{}", args.week_start),
                    read_json_input(&args.input)?,
                    args.dry_run,
                )
                .await
            }
            ReviewWeekSubcommand::Value(args) => {
                write_or_preview(
                    client,
                    "PUT",
                    &format!(
                        "/v1/reviews/weeks/{}/values/{}",
                        args.week_start, args.value_id
                    ),
                    read_json_input(&args.input)?,
                    args.dry_run,
                )
                .await
            }
        },
        ReviewPeriod::Month(month) => match &month.command {
            ReviewMonthSubcommand::Show(args) => {
                client
                    .get(&format!("/v1/reviews/months/{}", args.month))
                    .await
            }
            ReviewMonthSubcommand::Update(args) => {
                write_or_preview(
                    client,
                    "PATCH",
                    &format!("/v1/reviews/months/{}", args.month),
                    read_json_input(&args.input)?,
                    args.dry_run,
                )
                .await
            }
        },
    }
}

async fn run_deck(client: &ApiClient, deck: &DeckCommand) -> CliResult<Value> {
    match &deck.command {
        DeckSubcommand::List => client.get("/v1/review-card-decks").await,
        DeckSubcommand::Show(args) => {
            client
                .get(&format!("/v1/review-card-decks/{}", args.deck_id))
                .await
        }
        DeckSubcommand::Create(args) => {
            client
                .post(
                    "/v1/review-card-decks",
                    json!({
                        "name": args.name,
                        "parentDeckId": args.parent,
                    }),
                )
                .await
        }
        DeckSubcommand::Delete(args) => {
            client
                .delete(&format!("/v1/review-card-decks/{}", args.deck_id))
                .await
        }
        DeckSubcommand::Cards(args) => {
            client
                .post(
                    &format!("/v1/review-card-decks/{}/cards", args.deck_id),
                    json!({
                        "includeChildren": args.include_children,
                        "archived": deck_cards_archived_filter(args),
                    }),
                )
                .await
        }
        DeckSubcommand::Package(package) => run_deck_package(client, package).await,
        DeckSubcommand::Anki(_) => unreachable!("deck anki is handled before config loading"),
    }
}

async fn run_deck_package(client: &ApiClient, package: &DeckPackageCommand) -> CliResult<Value> {
    match &package.command {
        DeckPackageSubcommand::Preview(args) => {
            client
                .post(
                    "/v1/review-card-decks/import/package/preview",
                    json!({"path": args.path}),
                )
                .await
        }
        DeckPackageSubcommand::Import(args) => {
            client
                .post(
                    "/v1/review-card-decks/import/package/confirm",
                    json!({
                        "path": args.path,
                        "importStudyHistory": args.import_study_history,
                    }),
                )
                .await
        }
        DeckPackageSubcommand::Export(args) => {
            client
                .post(
                    "/v1/review-card-decks/export/package",
                    json!({
                        "deckId": args.deck_id,
                        "outPath": args.out_path,
                        "author": args.author,
                        "contact": args.contact,
                        "version": args.version,
                        "includeStudyHistory": args.include_study_history,
                    }),
                )
                .await
        }
    }
}

async fn run_card(client: &ApiClient, card: &CardCommand) -> CliResult<Value> {
    match &card.command {
        CardSubcommand::Archive(args) => {
            client
                .post(
                    &format!("/v1/review-cards/{}/archive", args.card_id),
                    json!({}),
                )
                .await
        }
        CardSubcommand::Unarchive(args) => {
            client
                .post(
                    &format!("/v1/review-cards/{}/unarchive", args.card_id),
                    json!({}),
                )
                .await
        }
        CardSubcommand::Trash(args) => {
            client
                .post(
                    &format!("/v1/review-cards/{}/trash", args.card_id),
                    json!({}),
                )
                .await
        }
        CardSubcommand::Unlink(args) => {
            client
                .post(
                    &format!(
                        "/v1/tasks/{}/review-cards/{}/unlink",
                        args.task_id, args.card_id
                    ),
                    json!({}),
                )
                .await
        }
        CardSubcommand::UnlinkNote(args) => {
            client
                .post(
                    &format!(
                        "/v1/tasks/{}/review-notes/{}/unlink",
                        args.task_id, args.note_id
                    ),
                    json!({}),
                )
                .await
        }
    }
}

async fn run_ai_agent(client: &ApiClient, ai_agent: &AiAgentCommand) -> CliResult<Value> {
    match &ai_agent.command {
        AiAgentSubcommand::Tools => client.get("/v1/ai-agent/tools").await,
        AiAgentSubcommand::Task(task) => match &task.command {
            AiAgentTaskSubcommand::Export(args) => {
                client
                    .get(&format!("/v1/ai-agent/tasks/{}/export", args.id))
                    .await
            }
            AiAgentTaskSubcommand::Validate(args) => {
                client
                    .post("/v1/ai-agent/tasks/validate", read_json_input(&args.input)?)
                    .await
            }
            AiAgentTaskSubcommand::Import(args) => {
                let path = if args.dry_run {
                    "/v1/ai-agent/tasks/import?dryRun=true"
                } else {
                    "/v1/ai-agent/tasks/import"
                };
                client.post(path, read_json_input(&args.input)?).await
            }
        },
        AiAgentSubcommand::ProjectContext(command) => match &command.command {
            AiAgentProjectContextSubcommand::Ensure(args) => {
                client
                    .post(
                        "/v1/ai-agent/project-context-attachments/ensure",
                        read_json_input(&args.input)?,
                    )
                    .await
            }
            AiAgentProjectContextSubcommand::Read(args) => {
                client
                    .post(
                        "/v1/ai-agent/project-context-attachments/read",
                        read_json_input(&args.input)?,
                    )
                    .await
            }
            AiAgentProjectContextSubcommand::Reconcile(args) => {
                client
                    .post(
                        "/v1/ai-agent/project-context-attachments/reconcile",
                        read_json_input(&args.input)?,
                    )
                    .await
            }
            AiAgentProjectContextSubcommand::Write(args) => {
                client
                    .post(
                        "/v1/ai-agent/project-context-attachments/write",
                        read_json_input(&args.input)?,
                    )
                    .await
            }
        },
    }
}

async fn write_or_preview(
    client: &ApiClient,
    method: &str,
    path: &str,
    body: Value,
    dry_run: bool,
) -> CliResult<Value> {
    if dry_run {
        return Ok(request_preview(method, path, body));
    }
    match method {
        "PATCH" => client.patch(path, body).await,
        "PUT" => client.put(path, body).await,
        "POST" => client.post(path, body).await,
        _ => Err(CliError::Internal(format!("unsupported method {method}"))),
    }
}

fn read_json_input(path: &str) -> CliResult<Value> {
    let raw = if path == "-" {
        let mut buffer = String::new();
        io::Read::read_to_string(&mut io::stdin(), &mut buffer)
            .map_err(|error| CliError::Usage(format!("failed to read stdin: {error}")))?;
        buffer
    } else {
        fs::read_to_string(path)
            .map_err(|error| CliError::Usage(format!("failed to read {path}: {error}")))?
    };
    serde_json::from_str(&raw)
        .map_err(|error| CliError::Usage(format!("input must be valid JSON: {error}")))
}

fn deck_cards_archived_filter(args: &DeckCardsArgs) -> Option<bool> {
    if args.archived {
        Some(true)
    } else if args.active {
        Some(false)
    } else {
        None
    }
}

fn human_value(value: Value) -> String {
    if let Some(help) = value.get("help").and_then(Value::as_str) {
        return help.to_string();
    }
    serde_json::to_string_pretty(&value).expect("JSON value serializes")
}
