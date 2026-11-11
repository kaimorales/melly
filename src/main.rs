mod commands;
mod rpc;
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
use melly::session::Session;
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::time::{self, MissedTickBehavior};

use rpc::Rpc;
use ui::Workspace;

const HELP: &str = "melly — a terminal workspace for Codex\n\nUsage: melly [--model MODEL] [--resume THREAD_ID]\n\nRun it from your project directory with Codex installed and signed in.";

struct Options {
    model: Option<String>,
    resume: Option<String>,
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
    let _restore = TerminalMode::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    run(&mut terminal, cwd, codex, options).await
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    cwd: PathBuf,
    codex: OsString,
    options: Options,
) -> Result<()> {
    let cwd_text = cwd
        .to_str()
        .context("Melly requires a UTF-8 project path")?
        .to_owned();
    let mut session = Session::new(cwd_text, options.model, options.resume);
    let mut workspace = Workspace::default();
    terminal.draw(|frame| workspace.draw(frame, &session))?;

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
    let mut tick = time::interval(Duration::from_millis(16));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut heartbeat = time::interval(Duration::from_millis(100));
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut dirty = true;

    loop {
        workspace.synchronize(&mut session);
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
            event = keys.next() => {
                match event {
                    Some(Ok(Event::Key(key))) => {
                        if workspace.key(key, &mut session) {
                            return Ok(());
                        }
                        dirty = true;
                    }
                    Some(Ok(Event::Paste(text))) => {
                        workspace.insert(&text);
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
                    Ok(Some(message)) => dirty |= session.receive(message),
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
            _ = heartbeat.tick(), if session.busy() || session.connecting() => dirty = true,
            _ = tick.tick(), if dirty => {
                terminal.draw(|frame| workspace.draw(frame, &session))?;
                dirty = false;
            }
        }
    }
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
            "--resume" => {
                options.resume = Some(args.next().context("--resume requires a thread ID")?)
            }
            _ => bail!("Unknown argument {argument}. See melly --help."),
        }
    }
    Ok(Some(options))
}
