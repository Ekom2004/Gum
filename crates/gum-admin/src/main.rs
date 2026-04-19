use std::env;
use std::error::Error;
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Tabs, Wrap,
};
use ratatui::{Frame, Terminal};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum View {
    Runs,
    Runners,
    Leases,
    Concurrency,
}

impl View {
    fn next(self) -> Self {
        match self {
            View::Runs => View::Runners,
            View::Runners => View::Leases,
            View::Leases => View::Concurrency,
            View::Concurrency => View::Runs,
        }
    }

    fn previous(self) -> Self {
        match self {
            View::Runs => View::Concurrency,
            View::Runners => View::Runs,
            View::Leases => View::Runners,
            View::Concurrency => View::Leases,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct RunRecord {
    id: String,
    job_id: String,
    status: String,
    attempt: u32,
    trigger_type: Option<String>,
    failure_reason: Option<String>,
    waiting_reason: Option<String>,
    replay_of: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct RunsResponse {
    runs: Vec<RunRecord>,
}

#[derive(Clone, Debug, Deserialize)]
struct RunnerStatusRecord {
    id: String,
    compute_class: String,
    max_concurrent_leases: u32,
    last_heartbeat_at_epoch_ms: i64,
    active_lease_count: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct RunnersResponse {
    runners: Vec<RunnerStatusRecord>,
}

#[derive(Clone, Debug, Deserialize)]
struct LeaseStatusRecord {
    lease_id: String,
    run_id: String,
    attempt_id: String,
    runner_id: String,
    expires_at_epoch_ms: i64,
    cancel_requested: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct LeasesResponse {
    leases: Vec<LeaseStatusRecord>,
}

#[derive(Clone, Debug, Deserialize)]
struct ConcurrencyStatusRecord {
    job_id: String,
    job_name: String,
    concurrency_limit: u32,
    active_count: u32,
    queued_count: u32,
    active_run_ids: Vec<String>,
    queued_run_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ConcurrencyResponse {
    concurrency: Vec<ConcurrencyStatusRecord>,
}

#[derive(Clone, Debug, Deserialize)]
struct LogLineRecord {
    attempt_id: String,
    stream: String,
    message: String,
}

#[derive(Clone, Debug)]
struct Snapshot {
    runs: Vec<RunRecord>,
    runners: Vec<RunnerStatusRecord>,
    leases: Vec<LeaseStatusRecord>,
    concurrency: Vec<ConcurrencyStatusRecord>,
    logs: Vec<LogLineRecord>,
}

impl Snapshot {
    fn empty() -> Self {
        Self {
            runs: Vec::new(),
            runners: Vec::new(),
            leases: Vec::new(),
            concurrency: Vec::new(),
            logs: Vec::new(),
        }
    }
}

struct ApiClient {
    client: reqwest::Client,
    base_url: String,
    admin_key: String,
}

impl ApiClient {
    fn new(base_url: String, admin_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            admin_key,
        }
    }

    async fn list_runs(&self) -> Result<Vec<RunRecord>, String> {
        let response = self.get::<RunsResponse>("/internal/admin/runs").await?;
        Ok(response.runs)
    }

    async fn list_runners(&self) -> Result<Vec<RunnerStatusRecord>, String> {
        let response = self
            .get::<RunnersResponse>("/internal/admin/runners")
            .await?;
        Ok(response.runners)
    }

    async fn list_leases(&self) -> Result<Vec<LeaseStatusRecord>, String> {
        let response = self.get::<LeasesResponse>("/internal/admin/leases").await?;
        Ok(response.leases)
    }

    async fn list_concurrency(&self) -> Result<Vec<ConcurrencyStatusRecord>, String> {
        let response = self
            .get::<ConcurrencyResponse>("/internal/admin/concurrency")
            .await?;
        Ok(response.concurrency)
    }

    async fn run_logs(&self, run_id: &str) -> Result<Vec<LogLineRecord>, String> {
        self.get::<Vec<LogLineRecord>>(&format!("/v1/runs/{run_id}/logs"))
            .await
    }

    async fn cancel_run(&self, run_id: &str) -> Result<(), String> {
        self.post_empty(&format!("/v1/runs/{run_id}/cancel")).await
    }

    async fn replay_run(&self, run_id: &str) -> Result<(), String> {
        self.post_empty(&format!("/v1/runs/{run_id}/replay")).await
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, String> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let response = self
            .client
            .get(url)
            .header(AUTHORIZATION, format!("Bearer {}", self.admin_key))
            .send()
            .await
            .map_err(|error| format!("request failed: {error}"))?;
        parse_json_response(response).await
    }

    async fn post_empty(&self, path: &str) -> Result<(), String> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let response = self
            .client
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {}", self.admin_key))
            .header(CONTENT_TYPE, "application/json")
            .body("{\"reason\":null}")
            .send()
            .await
            .map_err(|error| format!("request failed: {error}"))?;
        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read error body".to_string());
            Err(format!("request failed: {status} {body}"))
        }
    }
}

async fn parse_json_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T, String> {
    if response.status().is_success() {
        response
            .json::<T>()
            .await
            .map_err(|error| format!("invalid response payload: {error}"))
    } else {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "failed to read error body".to_string());
        Err(format!("request failed: {status} {body}"))
    }
}

struct App {
    view: View,
    selected_index: usize,
    status_filter: RunFilter,
    filter_mode: bool,
    filter_selected_index: usize,
    message: String,
    message_is_error: bool,
    snapshot: Snapshot,
}

impl App {
    fn new() -> Self {
        Self {
            view: View::Runs,
            selected_index: 0,
            status_filter: RunFilter::All,
            filter_mode: false,
            filter_selected_index: 0,
            message: "Connected to Gum admin.".to_string(),
            message_is_error: false,
            snapshot: Snapshot::empty(),
        }
    }

    fn filtered_runs(&self) -> Vec<RunRecord> {
        filter_runs(&self.snapshot.runs, self.status_filter)
    }

    fn selected_run(&self) -> Option<RunRecord> {
        let runs = self.filtered_runs();
        select_item(&runs, self.selected_index).cloned()
    }

    fn selection_len(&self) -> usize {
        match self.view {
            View::Runs => self.filtered_runs().len(),
            View::Runners => self.snapshot.runners.len(),
            View::Leases => self.snapshot.leases.len(),
            View::Concurrency => self.snapshot.concurrency.len(),
        }
    }

    fn clamp_selection(&mut self) {
        let len = self.selection_len();
        if len == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= len {
            self.selected_index = len - 1;
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.selection_len();
        if len == 0 {
            self.selected_index = 0;
            return;
        }
        let next = (self.selected_index as isize + delta).rem_euclid(len as isize);
        self.selected_index = next as usize;
    }

    fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.message_is_error = false;
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.message_is_error = true;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunFilter {
    All,
    Running,
    Queued,
    Failed,
    TimedOut,
    Canceled,
    Succeeded,
}

impl RunFilter {
    const OPTIONS: [RunFilter; 7] = [
        RunFilter::All,
        RunFilter::Running,
        RunFilter::Queued,
        RunFilter::Failed,
        RunFilter::TimedOut,
        RunFilter::Canceled,
        RunFilter::Succeeded,
    ];

    fn label(self) -> &'static str {
        match self {
            RunFilter::All => "all",
            RunFilter::Running => "running",
            RunFilter::Queued => "queued",
            RunFilter::Failed => "failed",
            RunFilter::TimedOut => "timed_out",
            RunFilter::Canceled => "canceled",
            RunFilter::Succeeded => "succeeded",
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let base_url =
        env::var("GUM_API_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8000".to_string());
    let admin_key = env::var("GUM_ADMIN_KEY").map_err(|_| {
        "GUM_ADMIN_KEY is required; launch through `gum admin` or set it explicitly"
    })?;
    let client = ApiClient::new(base_url, admin_key);
    let mut app = App::new();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let guard = TerminalGuard::new();

    let result = run_app(&mut terminal, client, &mut app).await;

    drop(guard);
    if let Err(error) = result {
        eprintln!("{error}");
    }
    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    client: ApiClient,
    app: &mut App,
) -> Result<(), String> {
    let mut last_refresh = Instant::now() - Duration::from_secs(2);
    loop {
        if last_refresh.elapsed() >= Duration::from_secs(1) {
            refresh_snapshot(app, &client).await;
            last_refresh = Instant::now();
        }

        terminal
            .draw(|frame| render(frame, app))
            .map_err(|error| format!("draw failed: {error}"))?;

        if event::poll(Duration::from_millis(100)).map_err(|error| error.to_string())? {
            let event = event::read().map_err(|error| error.to_string())?;
            if let Event::Key(key) = event {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if handle_key(app, &client, key.code).await? {
                    return Ok(());
                }
            }
        }
    }
}

async fn refresh_snapshot(app: &mut App, client: &ApiClient) {
    let runs = client.list_runs().await;
    let runners = client.list_runners().await;
    let leases = client.list_leases().await;
    let concurrency = client.list_concurrency().await;
    match (runs, runners, leases, concurrency) {
        (Ok(runs), Ok(runners), Ok(leases), Ok(concurrency)) => {
            app.snapshot.runs = runs;
            app.snapshot.runners = runners;
            app.snapshot.leases = leases;
            app.snapshot.concurrency = concurrency;
            let selected_run_id = if matches!(app.view, View::Runs) {
                app.selected_run().map(|run| run.id)
            } else {
                None
            };
            app.snapshot.logs = match selected_run_id {
                Some(run_id) => match client.run_logs(&run_id).await {
                    Ok(logs) => logs,
                    Err(error) => {
                        app.set_error(error);
                        Vec::new()
                    }
                },
                None => Vec::new(),
            };
            app.clamp_selection();
        }
        (Err(error), _, _, _)
        | (_, Err(error), _, _)
        | (_, _, Err(error), _)
        | (_, _, _, Err(error)) => {
            app.set_error(error);
        }
    }
}

async fn handle_key(app: &mut App, client: &ApiClient, code: KeyCode) -> Result<bool, String> {
    if app.filter_mode {
        match code {
            KeyCode::Esc => {
                app.filter_mode = false;
                app.set_message("Filter canceled.");
            }
            KeyCode::Enter => {
                app.filter_mode = false;
                app.status_filter = RunFilter::OPTIONS[app.filter_selected_index];
                app.set_message(format!("Run filter: {}", app.status_filter.label()));
                app.clamp_selection();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.filter_selected_index =
                    (app.filter_selected_index + 1) % RunFilter::OPTIONS.len();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.filter_selected_index = (app.filter_selected_index + RunFilter::OPTIONS.len()
                    - 1)
                    % RunFilter::OPTIONS.len();
            }
            _ => {}
        }
        return Ok(false);
    }

    match code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('1') => {
            app.view = View::Runs;
            app.selected_index = 0;
        }
        KeyCode::Char('2') => {
            app.view = View::Runners;
            app.selected_index = 0;
        }
        KeyCode::Char('3') => {
            app.view = View::Leases;
            app.selected_index = 0;
        }
        KeyCode::Char('4') => {
            app.view = View::Concurrency;
            app.selected_index = 0;
        }
        KeyCode::Left => {
            app.view = app.view.previous();
            app.selected_index = 0;
        }
        KeyCode::Right => {
            app.view = app.view.next();
            app.selected_index = 0;
        }
        KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
        KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
        KeyCode::Char('/') => {
            app.filter_mode = true;
            app.filter_selected_index = RunFilter::OPTIONS
                .iter()
                .position(|filter| *filter == app.status_filter)
                .unwrap_or(0);
            app.set_message("Select a status filter. Enter applies. Esc cancels.");
        }
        KeyCode::Char('c') => {
            if let Some(run) = app.selected_run() {
                client.cancel_run(&run.id).await?;
                app.set_message(format!("Canceled {}.", run.id));
                refresh_snapshot(app, client).await;
            }
        }
        KeyCode::Char('r') => {
            if let Some(run) = app.selected_run() {
                client.replay_run(&run.id).await?;
                app.set_message(format!("Replayed {}.", run.id));
                refresh_snapshot(app, client).await;
            }
        }
        _ => {}
    }
    Ok(false)
}

fn render(frame: &mut Frame<'_>, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(14),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(10)])
        .split(layout[3]);
    let upper = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(body[0]);

    render_header(frame, layout[0], app);
    render_tabs(frame, layout[1], app);
    render_message(frame, layout[2], app);
    render_primary(frame, upper[0], app);
    render_detail(frame, upper[1], app);
    render_logs(frame, body[1], app);
    render_footer(frame, layout[4]);
    if app.filter_mode {
        render_filter_popup(frame, app);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let runs = app.filtered_runs();
    let queued = runs.iter().filter(|run| run.status == "queued").count();
    let running = runs.iter().filter(|run| run.status == "running").count();
    let failed = runs
        .iter()
        .filter(|run| matches!(run.status.as_str(), "failed" | "timed_out"))
        .count();
    let text = Line::from(vec![
        Span::styled(
            "GUM ADMIN",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "   queued: {}   running: {}   failed: {}   runners: {}   active leases: {}",
            queued,
            running,
            failed,
            app.snapshot.runners.len(),
            app.snapshot.leases.len()
        )),
    ]);
    frame.render_widget(Paragraph::new(text), area);
}

fn render_tabs(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let titles = ["1:runs", "2:runners", "3:leases", "4:concurrency"]
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    let selected = match app.view {
        View::Runs => 0,
        View::Runners => 1,
        View::Leases => 2,
        View::Concurrency => 3,
    };
    let tabs = Tabs::new(titles)
        .block(panel_block(" VIEWS ").borders(Borders::BOTTOM))
        .select(selected)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider("  ");
    frame.render_widget(tabs, area);
}

fn render_message(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let color = if app.message_is_error {
        Color::Red
    } else {
        Color::Green
    };
    let line = Line::from(vec![
        Span::styled(
            format!("filter: {}  ", app.status_filter.label()),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(app.message.clone(), Style::default().fg(color)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_primary(frame: &mut Frame<'_>, area: Rect, app: &App) {
    match app.view {
        View::Runs => render_runs_table(frame, area, app),
        View::Runners => render_runners_table(frame, area, app),
        View::Leases => render_leases_table(frame, area, app),
        View::Concurrency => render_concurrency_table(frame, area, app),
    }
}

fn render_runs_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let runs = app.filtered_runs();
    let rows = runs.iter().map(|run| {
        Row::new(vec![
            Cell::from(status_cell(&run.status)),
            Cell::from(truncate_text(&run.job_id, 18)),
            Cell::from(truncate_text(&run.id, 18)),
            Cell::from(run.attempt.to_string()),
            Cell::from(truncate_text(
                run.trigger_type.as_deref().unwrap_or("--"),
                10,
            )),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(11),
            Constraint::Length(20),
            Constraint::Length(20),
            Constraint::Length(7),
            Constraint::Length(10),
        ],
    )
    .block(panel_block(" RUNS "))
    .header(
        Row::new(vec!["status", "job", "run id", "try", "trigger"])
            .style(Style::default().fg(Color::DarkGray)),
    )
    .highlight_style(Style::default().bg(Color::DarkGray))
    .highlight_symbol("▶ ");
    let mut state = TableState::default();
    if !runs.is_empty() {
        state.select(Some(app.selected_index.min(runs.len() - 1)));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_runners_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = app.snapshot.runners.iter().map(|runner| {
        Row::new(vec![
            Cell::from(truncate_text(&runner.id, 18)),
            Cell::from(truncate_text(&runner.compute_class, 10)),
            Cell::from(format!(
                "{}/{}",
                runner.active_lease_count, runner.max_concurrent_leases
            )),
            Cell::from(truncate_text(
                &runner.last_heartbeat_at_epoch_ms.to_string(),
                12,
            )),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(14),
        ],
    )
    .block(panel_block(" RUNNERS "))
    .header(
        Row::new(vec!["id", "class", "active/max", "heartbeat"])
            .style(Style::default().fg(Color::DarkGray)),
    )
    .highlight_style(Style::default().bg(Color::DarkGray))
    .highlight_symbol("▶ ");
    let mut state = TableState::default();
    if !app.snapshot.runners.is_empty() {
        state.select(Some(app.selected_index.min(app.snapshot.runners.len() - 1)));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_leases_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = app.snapshot.leases.iter().map(|lease| {
        Row::new(vec![
            Cell::from(truncate_text(&lease.lease_id, 18)),
            Cell::from(truncate_text(&lease.run_id, 18)),
            Cell::from(truncate_text(&lease.runner_id, 18)),
            Cell::from(if lease.cancel_requested { "yes" } else { "no" }),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(20),
            Constraint::Length(20),
            Constraint::Length(8),
        ],
    )
    .block(panel_block(" LEASES "))
    .header(
        Row::new(vec!["lease id", "run id", "runner", "cancel"])
            .style(Style::default().fg(Color::DarkGray)),
    )
    .highlight_style(Style::default().bg(Color::DarkGray))
    .highlight_symbol("▶ ");
    let mut state = TableState::default();
    if !app.snapshot.leases.is_empty() {
        state.select(Some(app.selected_index.min(app.snapshot.leases.len() - 1)));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_concurrency_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = app.snapshot.concurrency.iter().map(|status| {
        Row::new(vec![
            Cell::from(truncate_text(&status.job_name, 18)),
            Cell::from(truncate_text(&status.job_id, 18)),
            Cell::from(format!(
                "{}/{}",
                status.active_count, status.concurrency_limit
            )),
            Cell::from(status.queued_count.to_string()),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(20),
            Constraint::Length(12),
            Constraint::Length(8),
        ],
    )
    .block(panel_block(" CONCURRENCY "))
    .header(
        Row::new(vec!["job", "job id", "active/max", "queued"])
            .style(Style::default().fg(Color::DarkGray)),
    )
    .highlight_style(Style::default().bg(Color::DarkGray))
    .highlight_symbol("▶ ");
    let mut state = TableState::default();
    if !app.snapshot.concurrency.is_empty() {
        state.select(Some(
            app.selected_index.min(app.snapshot.concurrency.len() - 1),
        ));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = match app.view {
        View::Runs => match app.selected_run() {
            Some(run) => vec![
                detail_line("run", &run.id),
                detail_line("job", &run.job_id),
                Line::from(vec![
                    detail_key("status"),
                    Span::raw(" "),
                    status_span(&run.status),
                    Span::raw(" "),
                    Span::styled(run.status.clone(), Style::default().fg(Color::White)),
                ]),
                detail_line("attempt", &run.attempt.to_string()),
                detail_line("trigger", run.trigger_type.as_deref().unwrap_or("--")),
                detail_line("replay", run.replay_of.as_deref().unwrap_or("--")),
                detail_line("waiting", run.waiting_reason.as_deref().unwrap_or("--")),
                detail_line("failure", run.failure_reason.as_deref().unwrap_or("--")),
            ],
            None => vec![Line::from("No run selected.")],
        },
        View::Runners => match select_item(&app.snapshot.runners, app.selected_index) {
            Some(runner) => vec![
                detail_line("runner", &runner.id),
                detail_line("class", &runner.compute_class),
                detail_line(
                    "active",
                    &format!(
                        "{}/{}",
                        runner.active_lease_count, runner.max_concurrent_leases
                    ),
                ),
                detail_line("seen", &runner.last_heartbeat_at_epoch_ms.to_string()),
            ],
            None => vec![Line::from("No runner selected.")],
        },
        View::Leases => match select_item(&app.snapshot.leases, app.selected_index) {
            Some(lease) => vec![
                detail_line("lease", &lease.lease_id),
                detail_line("run", &lease.run_id),
                detail_line("attempt", &lease.attempt_id),
                detail_line("runner", &lease.runner_id),
                detail_line("expires", &lease.expires_at_epoch_ms.to_string()),
                detail_line("cancel", if lease.cancel_requested { "yes" } else { "no" }),
            ],
            None => vec![Line::from("No lease selected.")],
        },
        View::Concurrency => match select_item(&app.snapshot.concurrency, app.selected_index) {
            Some(status) => {
                let slots = format!("{}/{}", status.active_count, status.concurrency_limit);
                let queued = status.queued_count.to_string();
                let active_runs = if status.active_run_ids.is_empty() {
                    "--".to_string()
                } else {
                    status.active_run_ids.join(", ")
                };
                let queued_runs = if status.queued_run_ids.is_empty() {
                    "--".to_string()
                } else {
                    status.queued_run_ids.join(", ")
                };
                vec![
                    detail_line("job", &status.job_name),
                    detail_line("job id", &status.job_id),
                    detail_line("slots", &slots),
                    detail_line("queued", &queued),
                    detail_line("active runs", &active_runs),
                    detail_line("queued runs", &queued_runs),
                ]
            }
            None => vec![Line::from("No concurrency-limited job selected.")],
        },
    };
    let widget = Paragraph::new(lines)
        .block(panel_block(" DETAIL "))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, area);
}

fn render_logs(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = if app.snapshot.logs.is_empty() {
        vec![Line::from("No logs for selected run.")]
    } else {
        app.snapshot
            .logs
            .iter()
            .rev()
            .take(12)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(render_log_line)
            .collect()
    };
    let widget = Paragraph::new(lines)
        .block(panel_block(" LOGS "))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect) {
    let footer =
        "j/k move  / filter  c cancel  r replay  1 runs  2 runners  3 leases  4 concurrency  q quit";
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn render_filter_popup(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(40, 11, frame.area());
    let lines = RunFilter::OPTIONS
        .iter()
        .enumerate()
        .map(|(index, filter)| {
            let marker = if index == app.filter_selected_index {
                "▶"
            } else {
                " "
            };
            Line::from(format!("{marker} {}", filter.label()))
        })
        .collect::<Vec<_>>();
    let widget = Paragraph::new(lines)
        .block(panel_block(" FILTER BY STATUS "))
        .wrap(Wrap { trim: true });
    frame.render_widget(Clear, area);
    frame.render_widget(widget, area);
}

fn centered_rect(width_percent: u16, height: u16, area: Rect) -> Rect {
    let popup = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(popup[1])[1]
}

fn status_span(status: &str) -> Span<'static> {
    let (symbol, color) = match status {
        "running" => ("●", Color::Green),
        "queued" => ("○", Color::Yellow),
        "failed" => ("×", Color::Red),
        "timed_out" => ("!", Color::Red),
        "canceled" => ("■", Color::DarkGray),
        "succeeded" => ("✓", Color::Cyan),
        _ => ("?", Color::White),
    };
    Span::styled(
        symbol,
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

fn status_cell(status: &str) -> Line<'static> {
    Line::from(vec![
        status_span(status),
        Span::raw(" "),
        Span::styled(truncate_text(status, 8), Style::default().fg(Color::White)),
    ])
}

fn render_log_line(log: &LogLineRecord) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("[{}]", truncate_text(&log.stream, 6)),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{} ", truncate_text(&log.attempt_id, 10)),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(truncate_text(&log.message, 120)),
    ])
}

fn detail_key(label: &str) -> Span<'static> {
    Span::styled(format!("{label:8}:"), Style::default().fg(Color::DarkGray))
}

fn detail_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        detail_key(label),
        Span::raw(" "),
        Span::raw(value.to_string()),
    ])
}

fn panel_block(title: &'static str) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return value.to_string();
    }
    if max_chars <= 1 {
        return "…".to_string();
    }
    chars[..max_chars - 1].iter().collect::<String>() + "…"
}

fn filter_runs(runs: &[RunRecord], filter: RunFilter) -> Vec<RunRecord> {
    runs.iter()
        .filter(|run| match filter {
            RunFilter::All => true,
            RunFilter::Running => run.status == "running",
            RunFilter::Queued => run.status == "queued",
            RunFilter::Failed => run.status == "failed",
            RunFilter::TimedOut => run.status == "timed_out",
            RunFilter::Canceled => run.status == "canceled",
            RunFilter::Succeeded => run.status == "succeeded",
        })
        .cloned()
        .collect()
}

fn select_item<T>(items: &[T], selected_index: usize) -> Option<&T> {
    if items.is_empty() {
        None
    } else {
        items.get(selected_index.min(items.len() - 1))
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> Self {
        Self
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen);
    }
}

#[cfg(test)]
mod tests {
    use super::{filter_runs, select_item, RunFilter, RunRecord};

    fn run(id: &str, job_id: &str, status: &str) -> RunRecord {
        RunRecord {
            id: id.to_string(),
            job_id: job_id.to_string(),
            status: status.to_string(),
            attempt: 1,
            trigger_type: Some("enqueue".to_string()),
            failure_reason: None,
            waiting_reason: None,
            replay_of: None,
        }
    }

    #[test]
    fn filter_runs_matches_selected_status() {
        let runs = vec![
            run("run_1", "job_export", "running"),
            run("run_2", "job_sync", "failed"),
        ];
        assert_eq!(filter_runs(&runs, RunFilter::All).len(), 2);
        assert_eq!(filter_runs(&runs, RunFilter::Running).len(), 1);
        assert_eq!(filter_runs(&runs, RunFilter::Failed).len(), 1);
        assert_eq!(filter_runs(&runs, RunFilter::Queued).len(), 0);
    }

    #[test]
    fn select_item_clamps_index() {
        let runs = vec![run("run_1", "job_export", "running")];
        let selected = select_item(&runs, 42).expect("selection should clamp");
        assert_eq!(selected.id, "run_1");
    }
}
