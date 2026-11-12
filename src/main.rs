mod agent_ui;
mod commands;
mod handoff_ui;
mod rpc;
mod state;
mod ui;

use std::{
    ffi::OsString,
    io::{self, IsTerminal, Stdout},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use crossterm::{
    cursor::{Hide, Show},
    event::{DisableBracketedPaste, EnableBracketedPaste, Event, EventStream},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::StreamExt;
use melly::delegation::Harness;
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::time::{self, MissedTickBehavior};

use agent_ui::AgentWorkspace;
use rpc::Rpc;
use state::{SavedWorkspace, State};

const HELP: &str = "melly — a terminal workspace for Codex\n\nUsage: melly [--model MODEL] [--resume THREAD_ID | --continue]\n\nRun it from your project directory with Codex installed and signed in.";

struct Options {
    model: Option<String>,
    resume: Option<String>,
    latest: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let Some(options) = options()? else {
        return Ok(());
    };
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("Melly needs an interactive terminal. Run melly --help for options.");
    }

    let cwd = std::env::current_dir()?;
    let codex = std::env::var_os("MELLY_CODEX_BIN").unwrap_or_else(|| OsString::from("codex"));
    let mut state = State::open()?;
    let resume = if options.latest {
        Some(
            state
                .latest(&cwd)?
                .context("No saved Melly workspace in this directory.")?,
        )
    } else {
        options.resume
    };
    let saved = resume
        .as_deref()
        .map(|id| state.load(id))
        .transpose()?
        .flatten();
    let settings = state.settings()?;
    let mut session = Harness::new(
        cwd.to_str()
            .context("Melly requires a UTF-8 project path")?
            .into(),
        options.model,
        resume,
    );
    session.automatic_enabled = settings.auto_delegate;
    let mut workspace = AgentWorkspace::with_settings(settings);
    if let Some(saved) = saved {
        session.restore_team(saved.team);
        workspace.restore(&mut session, &saved.selected, saved.drafts);
    }
    let _restore = TerminalMode::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let rpc_cwd = PathBuf::from(&session.agents[0].session.cwd);
    let result = run(
        &mut terminal,
        rpc_cwd,
        codex,
        &mut session,
        &mut workspace,
        &mut state,
    )
    .await;
    save_workspace(&mut state, &session, &workspace)?;
    result
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    cwd: PathBuf,
    codex: OsString,
    session: &mut Harness,
    workspace: &mut AgentWorkspace,
    state: &mut State,
) -> Result<()> {
    workspace.synchronize(session);
    terminal.draw(|frame| workspace.draw(frame, session))?;

    let mut connection = match Rpc::connect(&codex, &cwd) {
        Ok(connection) => Some(connection),
        Err(error) => {
            session.disconnected(&format!(
                "{error:#}. Install Codex and sign in with codex login."
            ));
            None
        }
    };
    let mut keys = EventStream::new();
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    let mut tick = time::interval(Duration::from_millis(16));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut heartbeat = time::interval(Duration::from_millis(100));
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut save_tick = time::interval(Duration::from_millis(500));
    save_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut needs_save = true;
    let mut save_error = None;
    let mut dirty = true;

    loop {
        workspace.synchronize(session);
        let changed = session.tick();
        dirty |= changed;
        needs_save |= changed;
        if let Some(rpc) = connection.as_mut() {
            for request in session.drain_requests() {
                if let Err(error) = rpc.send(&request).await {
                    session.disconnected(&format!("{error:#}"));
                    connection = None;
                    dirty = true;
                    break;
                }
            }
        }

        tokio::select! {
            result = &mut shutdown => return result.context("Could not watch for terminal shutdown"),
            event = keys.next() => {
                match event {
                    Some(Ok(Event::Key(key))) => {
                        if workspace.key(key, session) {
                            return Ok(());
                        }
                        if workspace.settings_changed()
                            && let Err(error) = state.save_settings(workspace.settings())
                        {
                            workspace.settings_error(format!("{error:#}"));
                        }
                        needs_save = true;
                        dirty = true;
                    }
                    Some(Ok(Event::Paste(text))) => {
                        workspace.insert(&text);
                        needs_save = true;
                        dirty = true;
                    }
                    Some(Ok(Event::Resize(_, _))) => dirty = true,
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(error).context("Could not read terminal input"),
                    None => return Ok(()),
                }
            }
            message = async { connection.as_mut().expect("connection is present").receive().await }, if connection.is_some() => {
                match message {
                    Ok(Some(message)) => {
                        let changed = session.receive(message);
                        dirty |= changed;
                        needs_save |= changed;
                    }
                    Ok(None) => {
                        session.disconnected("Codex closed the connection. Reopen Melly to continue.");
                        connection = None;
                        dirty = true;
                    }
                    Err(error) => {
                        session.disconnected(&format!("{error:#}"));
                        connection = None;
                        dirty = true;
                    }
                }
            }
            _ = save_tick.tick(), if needs_save => {
                needs_save = false;
                if let Err(error) = save_workspace(state, session, workspace) {
                    let message = format!("{error:#}");
                    if save_error.as_ref() != Some(&message) {
                        session.agents[0].session.notice(&message);
                        save_error = Some(message);
                        dirty = true;
                    }
                } else {
                    save_error = None;
                }
            }
            _ = heartbeat.tick(), if session.busy() || session.agents.iter().any(|a|a.session.connecting()) => dirty = true,
            _ = tick.tick(), if dirty => {
                terminal.draw(|frame| workspace.draw(frame, session))?;
                dirty = false;
            }
        }
    }
}

async fn shutdown_signal() -> io::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut hangup = signal(SignalKind::hangup())?;
        let mut terminate = signal(SignalKind::terminate())?;
        tokio::select! {
            _ = hangup.recv() => Ok(()),
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    std::future::pending().await
}

fn save_workspace(state: &mut State, session: &Harness, workspace: &AgentWorkspace) -> Result<()> {
    if let Some(team) = session.snapshot() {
        let (selected, drafts) = workspace.snapshot(session);
        state.save(&SavedWorkspace {
            version: 1,
            team,
            selected,
            drafts,
        })?;
    }
    Ok(())
}

struct TerminalMode;

impl TerminalMode {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mode = Self;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            Hide
        )?;
        Ok(mode)
    }
}

impl Drop for TerminalMode {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            Show,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}

fn options() -> Result<Option<Options>> {
    let mut args = std::env::args().skip(1);
    let mut options = Options {
        model: None,
        resume: None,
        latest: false,
    };
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(None);
            }
            "--version" | "-V" => {
                println!("melly {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--model" | "-m" => {
                options.model = Some(args.next().context("--model requires a model ID")?)
            }
            "--continue" | "-c" => options.latest = true,
            "--resume" => {
                options.resume = Some(args.next().context("--resume requires a thread ID")?)
            }
            _ => bail!("Unknown argument {argument}. See melly --help."),
        }
    }
    if options.latest && options.resume.is_some() {
        bail!("Choose either --continue or --resume.");
    }
    Ok(Some(options))
}
