mod backend;
mod frontend;

use std::io;

use anyhow::Result;
use crossterm::{
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
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen, Clear(ClearType::All));
        orig_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new().await;
    let mut events = EventHandler::new(500);
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
    let _ = execute!(std::io::stdout(), Clear(ClearType::All), LeaveAlternateScreen);
    let _ = terminal.show_cursor();
}

fn is_ctrl_c(key: &crossterm::event::KeyEvent) -> bool {
    matches!(
        (key.code, key.modifiers),
        (crossterm::event::KeyCode::Char('c'), crossterm::event::KeyModifiers::CONTROL)
    )
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = fmt().with_env_filter(filter).with_writer(std::io::stderr).try_init();
}

async fn run_app<B>(terminal: &mut Terminal<B>, app: &mut App, events: &mut EventHandler) -> Result<()>
where
    B: ratatui::backend::Backend,
{
    loop {
        // draw is the only blocking call — if it jams, the outer `tokio::select!` on SIGINT still wins,
        // and the Ctrl+C key path below bypasses `app.on_key` throttling via instant `force_restore`.
        terminal.draw(|f| frontend::ui::render(f, app))?;

        // also race each event poll vs SIGINT so a throttled/flooded channel can't delay Ctrl+C
        let ev = tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                // instant close: clear entire screen, no throttling
                force_restore(terminal);
                std::process::exit(130);
            }
            ev = events.next() => ev,
        };

        match ev {
            Some(Ok(Event::Key(k))) if k.kind == crossterm::event::KeyEventKind::Press => {
                // failsafe: Ctrl+C must never be throttled or await a slow resolver — exit before any await
                if is_ctrl_c(&k) {
                    force_restore(terminal);
                    std::process::exit(130);
                }
                // also handle Ctrl+C with release/repeat variants (raw mode may emit them)
                app.on_key(k).await;
                if is_ctrl_c(&k) {
                    force_restore(terminal);
                    std::process::exit(130);
                }
            }
            Some(Ok(Event::Key(k))) if is_ctrl_c(&k) => {
                force_restore(terminal);
                std::process::exit(130);
            }
            Some(Ok(Event::Mouse(m))) => app.on_mouse(m).await,
            Some(Ok(Event::Resize(_, _))) => {}
            Some(Ok(Event::Tick)) => app.on_tick().await,
            Some(Err(e)) => tracing::warn!("event error: {e}"),
            None => break,
            _ => {}
        }
        if !app.running { break; }
    }
    Ok(())
}
