use std::process::ExitCode;
use std::sync::LazyLock;

use clap::Parser;
use regex::Regex;
use serde_json::{Map, Value, json};

use tohru_desktop::assistant::chat::{self, CommitArgs, resolve_backend_config};
use tohru_desktop::assistant::cli::{BackendKind, Cli, Command, NeedsYou, Offline, OrcaCommand};
use tohru_desktop::assistant::store::{check_id, connect, load, search};
use tohru_desktop::assistant::{
    AssistantError, Result, emit, home, models, needs_you, orca, quick, transfer,
};

static VOICE_REQUEST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_:%.-]{1,512}$").expect("fixed pattern is valid"));

fn models_of(raw: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(models)) => Some(models),
        _ => None,
    }
}

/// A local conversation as `show` prints it: without provider sessions or the memory mark.
fn shown(conversation: &tohru_desktop::assistant::store::Conversation) -> Value {
    let mut value = conversation.json();
    if let Value::Object(fields) = &mut value {
        fields.shift_remove("sessions");
        fields.shift_remove("remembered");
    }
    value
}

#[expect(clippy::too_many_lines, reason = "one arm per subcommand")]
fn run(command: Command) -> Result<u8> {
    match command {
        Command::VoiceReady {
            conversation,
            request,
        } => {
            let cid = check_id(&conversation)?;
            if !VOICE_REQUEST.is_match(&request) {
                emit(json!({"event": "error", "message": "Invalid voice request identity"}));
                return Ok(2);
            }
            let result = orca::orca_call(
                &[
                    "bot",
                    "voice-ready",
                    "--chat",
                    &cid,
                    "--request",
                    &request,
                    "--json",
                ],
                std::time::Duration::from_secs(10),
            );
            let registered = result
                .as_ref()
                .is_ok_and(|result| result.get("registered") == Some(&Value::Bool(true)));
            let message = result.as_ref().err().cloned().unwrap_or_default();
            emit(json!({"event": "voice_ready", "registered": registered, "message": message}));
            Ok(u8::from(result.is_err()))
        }
        Command::Orca {
            command: OrcaCommand::Init(args),
        } => home::orca_init_command(&args),
        Command::Providers {
            models: raw,
            backend,
        } => {
            let (kind, bot, _, offline) = resolve_backend_config(&backend);
            let models = models_of(&raw);
            let rows = if kind == BackendKind::Orca {
                orca::orca_providers(&bot, models.as_ref(), offline_name(offline))
            } else {
                models::discover(models.as_ref())
            };
            emit(Value::Array(rows));
            Ok(0)
        }
        Command::Signin { provider } => models::signin(&provider).map(|()| 0),
        Command::Chat(mut args) => {
            if args.voice && !args.draft && !args.semantic {
                quick::voice_failures(&args);
            }
            chat::chat(&mut args)
        }
        Command::Commit {
            provider,
            conversation,
            voice,
            session,
        } => chat::commit_draft(&CommitArgs {
            provider: &provider,
            conversation: &conversation,
            voice,
            session: &session,
        }),
        Command::Rollback { conversation } => chat::rollback(&conversation),
        Command::Remember(args) => {
            let (kind, ..) = resolve_backend_config(&args.backend);
            if kind == BackendKind::Orca {
                return Ok(0);
            }
            chat::remember(&args)
        }
        Command::History {
            backend,
            query,
            provider,
            since,
            limit,
        } => {
            let (kind, bot, _, offline) = resolve_backend_config(&backend);
            let limit = limit.clamp(1, 500);
            let provider = Some(provider.as_str()).filter(|provider| !provider.is_empty());
            let local = || search(&connect(None)?, &query, limit, provider, since);
            let rows = if kind == BackendKind::Orca {
                let rows = home::orca_history(&bot, &query, usize::try_from(limit).unwrap_or(500));
                if rows.is_empty()
                    && offline == Offline::Local
                    && orca::orca_find_bot(&bot).is_err()
                {
                    local()?
                } else {
                    rows
                }
            } else {
                local()?
            };
            emit(Value::Array(rows));
            Ok(0)
        }
        Command::Show {
            backend,
            conversation,
        } => {
            let (kind, bot, _, offline) = resolve_backend_config(&backend);
            let local = || -> Result<Option<Value>> {
                Ok(load(&connect(None)?, &conversation)?.as_ref().map(shown))
            };
            let found = if kind == BackendKind::Orca {
                match home::orca_show(&bot, &conversation)? {
                    None if offline == Offline::Local => local()?,
                    other => other,
                }
            } else {
                local()?
            };
            let Some(found) = found else {
                eprintln!("tohru-assistant: no such conversation");
                return Ok(1);
            };
            emit(found);
            Ok(0)
        }
        Command::Delete {
            backend,
            conversation,
        } => {
            let (kind, ..) = resolve_backend_config(&backend);
            if kind == BackendKind::Orca {
                eprintln!("tohru-assistant: close Orca bot chats in Orca's Bots panel");
                return Ok(1);
            }
            Ok(u8::from(!chat::delete(&conversation)?))
        }
        Command::Memory {
            backend,
            action,
            id,
            json,
        } => {
            let (kind, bot, ..) = resolve_backend_config(&backend);
            if kind == BackendKind::Orca {
                return home::orca_memory_command(action, id.as_deref(), json, &bot);
            }
            chat::memory_command(action, id.as_deref(), json)
        }
        Command::Sync { backend } => {
            let (kind, bot, ..) = resolve_backend_config(&backend);
            emit(if kind == BackendKind::Orca {
                home::orca_sync(&bot)
            } else {
                json!({"backend": "local", "online": true, "home_chat": "", "chats": []})
            });
            Ok(0)
        }
        Command::NeedsYou { command } => {
            let (backend, items) = match &command {
                NeedsYou::Poll { backend } => (backend, None),
                NeedsYou::Wake { backend, items } => (backend, Some(items)),
            };
            let (kind, bot, ..) = resolve_backend_config(backend);
            if kind != BackendKind::Orca {
                emit(
                    json!({"ok": false, "error": "Needs you watching needs assistant.backend = orca"}),
                );
                return Ok(0);
            }
            emit(match items {
                None => needs_you::poll(&bot),
                Some(items) => {
                    let pairs: Vec<(String, String)> = items
                        .iter()
                        .filter_map(|item| {
                            item.split_once(':')
                                .map(|(uid, event)| (uid.to_owned(), event.to_owned()))
                        })
                        .collect();
                    needs_you::wake(&bot, &pairs)
                }
            });
            Ok(0)
        }
        Command::Export { file } => transfer::export_command(file.as_deref()),
        Command::Import {
            file,
            merge: _,
            replace,
            yes,
            only,
        } => transfer::import_command(&file, replace, yes, &only),
    }
}

fn offline_name(offline: Offline) -> &'static str {
    match offline {
        Offline::Error => "error",
        Offline::Local => "local",
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(code) => ExitCode::from(code),
        Err(AssistantError::Exit(message)) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("tohru-assistant: {:#}", anyhow::Error::from(error));
            ExitCode::FAILURE
        }
    }
}
