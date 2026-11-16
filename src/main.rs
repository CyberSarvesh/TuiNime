mod backend;
mod frontend;

use std::io;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tracing_subscriber::{fmt, EnvFilter};

use frontend::app::App;
use frontend::event::{Event, EventHandler};

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    // --- failsafe panic hook: always restore terminal even if we jam/throttle then panic ---
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), DisableMouseCapture, LeaveAlternateScreen, Clear(ClearType::All));
        orig_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Ask the terminal which image protocol it supports (Kitty / Sixel / iTerm2) once, after
    // entering the alternate screen and before reading events; fall back to unicode halfblocks.
    let picker = ratatui_image::picker::Picker::from_query_stdio()
        .unwrap_or_else(|_| ratatui_image::picker::Picker::from_fontsize((8, 16)));
    let mut app = App::new(frontend::widgets::poster::PosterRenderer::new(picker));
    if let Some(path) = frontend::persist::default_path() {
        app.restore(path);
    }
    let mut events = EventHandler::new(120);
    app.spawn_initial_fetch();

    // race the app loop vs OS SIGINT (Ctrl+C) so a jammed/throttled task can't block exit
    let res = tokio::select! {
        // OS signal path — true instant, even if EventStream is stuck in a blocking decode
        _ = tokio::signal::ctrl_c() => {
            force_restore(&mut terminal);
            std::process::exit(130);
        }
        r = run_app(&mut terminal, &mut app, &mut events) => r,
    };

    // normal exit — also clear entire screen (no ghost frames)
    force_restore(&mut terminal);

    if let Err(err) = res {
        eprintln!("tuinime error: {err:?}");
    }
    Ok(())
}

/// Instant, idempotent terminal restore — safe to call from signal path or normal exit.
/// Uses `std::io::stdout()` for the `execute!` so it works for any `Backend` (no `B: Write` bound),
/// while `terminal.show_cursor()` still restores the cursor via the `Terminal`.
fn force_restore<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>) {
    let _ = disable_raw_mode();
    // clear entire screen before leaving alternate buffer — prevents ghost / jam artifacts.
    // We write to the real stdout handle; the `Terminal`'s backend shares the same fd.
    let _ = execute!(std::io::stdout(), Clear(ClearType::All), DisableMouseCapture, LeaveAlternateScreen);
    let _ = terminal.show_cursor();
}

fn is_ctrl_c(key: &crossterm::event::KeyEvent) -> bool {
    matches!(
        (key.code, key.modifiers),
        (crossterm::event::KeyCode::Char('c'), crossterm::event::KeyModifiers::CONTROL)
    )
}

/// Log to a file, never to the terminal: stderr shares the screen with the TUI, so any
/// warning printed there would scribble over the interface.
/// Location: `<cache dir>/tuinime/tuinime.log` (set `RUST_LOG=debug` for more detail).
fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("tuinime");
    let _ = std::fs::create_dir_all(&dir);
    let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("tuinime.log")) else {
        return; // no log file available — stay silent rather than corrupt the screen
    };
    let _ = fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(file))
        .try_init();
}

async fn run_app<B>(terminal: &mut Terminal<B>, app: &mut App, events: &mut EventHandler) -> Result<()>
where
    B: ratatui::backend::Backend,
{
    let mut last_screen = None;
    loop {
        // Sixel images are pixels on the terminal, not cells, so ratatui's diffing can't erase
        // them; wipe the screen whenever the screen or help popup changes so none are left behind.
        let screen = (app.view, app.show_help);
        if last_screen != Some(screen) {
            terminal.clear()?;
            last_screen = Some(screen);
        }
        // draw is the only blocking call — if it jams, the outer `tokio::select!` on SIGINT still wins,
        // and the Ctrl+C key path below bypasses `app.on_key` throttling via instant `force_restore`.
        terminal.draw(|f| frontend::ui::render(f, app))?;

        // race input vs background results (search / streams / trending) vs SIGINT, so results
        // show up immediately instead of waiting for the next tick, and Ctrl+C is never delayed
        let ev = tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                force_restore(terminal);
                std::process::exit(130);
            }
            Some(bg) = app.next_background() => {
                app.on_background(bg).await;
                continue;
            }
            ev = events.next() => ev,
        };

        match ev {
            Some(Ok(Event::Key(k))) if is_ctrl_c(&k) => {
                // failsafe: Ctrl+C must never wait on anything — exit before any await
                force_restore(terminal);
                std::process::exit(130);
            }
            Some(Ok(Event::Key(k))) if k.kind == crossterm::event::KeyEventKind::Press => app.on_key(k).await,
            Some(Ok(Event::Mouse(m))) => app.on_mouse(m).await,
            Some(Ok(Event::Resize)) => {}
            Some(Ok(Event::Tick)) => app.on_tick().await,
            Some(Err(e)) => tracing::warn!("event error: {e}"),
            None => break,
            _ => {}
        }
        if !app.running { break; }
    }
    Ok(())
}
