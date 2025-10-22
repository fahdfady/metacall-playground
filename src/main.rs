use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use metacall::{initialize, load, metacall};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Cell, List, ListItem, Padding, Paragraph, Row, Table, Tabs, Wrap,
    },
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
struct Script {
    path: PathBuf,
    name: String,
    language: String,
    runtime: String,
    functions: Vec<String>,
    loaded: bool,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutionResult {
    function: String,
    args: Vec<String>,
    output: String,
    duration_ms: u64,
    success: bool,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PipelineStep {
    id: String,
    script: String,
    function: String,
    args: Vec<String>,
    description: String,
}

#[derive(Debug, Clone, PartialEq)]
enum View {
    ScriptBrowser,
    FunctionTester,
    PipelineBuilder,
    ResultsExplorer,
    Export,
}

struct App {
    root_dir: PathBuf,
    scripts: Vec<Script>,
    selected_script: usize,
    results: Vec<ExecutionResult>,
    pipeline: Vec<PipelineStep>,
    selected_pipeline_step: usize,
    current_view: View,
    logs: Vec<LogEntry>,
    input_mode: InputMode,
    input_buffer: String,
    function_input: FunctionInput,
    selected_result: usize,
    show_help: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum InputMode {
    Normal,
    EditingArgs,
    AddingStep,
    ExportName,
}

#[derive(Debug, Clone)]
struct FunctionInput {
    selected_function: usize,
    args: Vec<String>,
    current_arg: String,
}

#[derive(Debug, Clone)]
struct LogEntry {
    timestamp: String,
    level: LogLevel,
    message: String,
}

#[derive(Debug, Clone)]
enum LogLevel {
    Info,
    Success,
    Error,
    Warning,
}

impl App {
    fn new(root_dir: PathBuf) -> Self {
        let mut app = Self {
            root_dir: root_dir.clone(),
            scripts: Vec::new(),
            selected_script: 0,
            results: Vec::new(),
            pipeline: Vec::new(),
            selected_pipeline_step: 0,
            current_view: View::ScriptBrowser,
            logs: Vec::new(),
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            function_input: FunctionInput {
                selected_function: 0,
                args: Vec::new(),
                current_arg: String::new(),
            },
            selected_result: 0,
            show_help: false,
        };

        app.add_log(LogLevel::Info, "MetaCall Playground started".to_string());
        app.add_log(
            LogLevel::Info,
            format!("Scanning directory: {}", root_dir.display()),
        );
        app.scan_scripts();
        app
    }

    fn timestamp() -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        let secs = now.as_secs() % 86400;
        format!(
            "{:02}:{:02}:{:02}",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )
    }

    fn add_log(&mut self, level: LogLevel, message: String) {
        self.logs.push(LogEntry {
            timestamp: Self::timestamp(),
            level,
            message,
        });
        if self.logs.len() > 100 {
            self.logs.remove(0);
        }
    }

    fn scan_scripts(&mut self) {
        self.scripts.clear();
        let mut found = 0;

        for entry in WalkDir::new(&self.root_dir)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                let (lang, runtime) = match ext.as_str() {
                    "py" => (Some("Python"), "py"),
                    "js" => (Some("JavaScript"), "node"),
                    "rb" => (Some("Ruby"), "rb"),
                    "ts" => (Some("TypeScript"), "ts"),
                    _ => (None, ""),
                };

                if let Some(language) = lang {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    
                    self.scripts.push(Script {
                        path: path.to_path_buf(),
                        name: name.clone(),
                        language: language.to_string(),
                        runtime: runtime.to_string(),
                        functions: Vec::new(),
                        loaded: false,
                        error: None,
                    });
                    found += 1;
                }
            }
        }

        self.add_log(
            LogLevel::Success,
            format!("Found {} scripts", found),
        );
    }

    fn load_script(&mut self, index: usize) -> Result<(), String> {
        if index >= self.scripts.len() {
            return Err("Invalid script index".into());
        }

        // Clone necessary data before borrowing mutably
        let script_path = self.scripts[index].path.clone();
        let script_name = self.scripts[index].name.clone();
        let script_runtime = self.scripts[index].runtime.clone();
        let script_language = self.scripts[index].language.clone();

        self.add_log(
            LogLevel::Info,
            format!("Loading {}...", script_name),
        );

        match load::from_single_file(&script_runtime, script_path.to_str().unwrap()) {
            Ok(_) => {
                self.scripts[index].loaded = true;
                self.scripts[index].error = None;
                
                if let Ok(content) = fs::read_to_string(&script_path) {
                    let functions = self.extract_functions(&content, &script_language);
                    let func_count = functions.len();
                    self.scripts[index].functions = functions;
                    
                    self.add_log(
                        LogLevel::Success,
                        format!(
                            "Loaded {} - found {} functions",
                            script_name,
                            func_count
                        ),
                    );
                }
                Ok(())
            }
            Err(e) => {
                let error = format!("{:?}", e);
                self.scripts[index].error = Some(error.clone());
                self.add_log(LogLevel::Error, format!("Failed to load {}: {}", script_name, error));
                Err(error)
            }
        }
    }

    fn extract_functions(&self, content: &str, language: &str) -> Vec<String> {
        let mut functions = Vec::new();
        
        match language {
            "Python" => {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("def ") && !trimmed.starts_with("def _") {
                        if let Some(name) = trimmed
                            .strip_prefix("def ")
                            .and_then(|s| s.split('(').next())
                        {
                            functions.push(name.to_string());
                        }
                    }
                }
            }
            "JavaScript" | "TypeScript" => {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("function ") {
                        if let Some(name) = trimmed
                            .strip_prefix("function ")
                            .and_then(|s| s.split('(').next())
                        {
                            functions.push(name.to_string());
                        }
                    } else if trimmed.starts_with("const ") || trimmed.starts_with("let ") {
                        if trimmed.contains(" = ") && (trimmed.contains("=>") || trimmed.contains("function")) {
                            if let Some(name) = trimmed
                                .split_whitespace()
                                .nth(1)
                                .and_then(|s| s.split('=').next())
                                .map(|s| s.trim())
                            {
                                functions.push(name.to_string());
                            }
                        }
                    }
                }
            }
            "Ruby" => {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("def ") {
                        if let Some(name) = trimmed
                            .strip_prefix("def ")
                            .and_then(|s| s.split('(').next())
                            .and_then(|s| s.split_whitespace().next())
                        {
                            functions.push(name.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
        
        functions
    }

    fn execute_function(&mut self) -> Result<(), String> {
        if self.scripts.is_empty() {
            return Err("No scripts available".into());
        }

        let script = &self.scripts[self.selected_script];
        if !script.loaded {
            return Err("Script not loaded. Press 'l' to load first".into());
        }

        if script.functions.is_empty() {
            return Err("No functions found in script".into());
        }

        let func_name = script.functions[self.function_input.selected_function].clone();
        let script_name = script.name.clone();
        let args = self.function_input.args.clone();
        
        self.add_log(
            LogLevel::Info,
            format!("Executing {}({:?})", func_name, args),
        );

        let start = Instant::now();
        
        // Fixed: Use single generic parameter for metacall
        let result: Result<String, String> = if args.is_empty() {
            metacall::<String>(&func_name, Vec::<i32>::new())
                .map_err(|e| format!("{:?}", e))
                .or_else(|_| {
                    metacall::<i64>(&func_name, Vec::<i32>::new())
                        .map(|v| v.to_string())
                        .map_err(|e| format!("{:?}", e))
                })
        } else if args.len() == 1 {
            let arg = &args[0];
            
            if let Ok(num) = arg.parse::<i64>() {
                metacall::<String>(&func_name, vec![num])
                    .map_err(|e| format!("{:?}", e))
                    .or_else(|_| {
                        metacall::<i64>(&func_name, vec![num])
                            .map(|v| v.to_string())
                            .map_err(|e| format!("{:?}", e))
                    })
            } else {
                metacall::<String>(&func_name, vec![arg.clone()])
                    .map_err(|e| format!("{:?}", e))
                    .or_else(|_| {
                        metacall::<i64>(&func_name, vec![arg.clone()])
                            .map(|v| v.to_string())
                            .map_err(|e| format!("{:?}", e))
                    })
            }
        } else {
            let nums: Result<Vec<i64>, _> = args
                .iter()
                .map(|s| s.parse::<i64>())
                .collect();
                
            if let Ok(nums) = nums {
                metacall::<String>(&func_name, nums.clone())
                    .map_err(|e| format!("{:?}", e))
                    .or_else(|_| {
                        metacall::<i64>(&func_name, nums)
                            .map(|v| v.to_string())
                            .map_err(|e| format!("{:?}", e))
                    })
            } else {
                metacall::<String>(&func_name, args.clone())
                    .map_err(|e| format!("{:?}", e))
            }
        };

        let duration = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                self.results.push(ExecutionResult {
                    function: format!("{}::{}", script_name, func_name),
                    args: self.function_input.args.clone(),
                    output: output.clone(),
                    duration_ms: duration,
                    success: true,
                    timestamp: Self::timestamp(),
                });
                
                self.add_log(
                    LogLevel::Success,
                    format!("✓ {}ms → {}", duration, output),
                );
                Ok(())
            }
            Err(e) => {
                self.results.push(ExecutionResult {
                    function: format!("{}::{}", script_name, func_name),
                    args: self.function_input.args.clone(),
                    output: e.clone(),
                    duration_ms: duration,
                    success: false,
                    timestamp: Self::timestamp(),
                });
                
                self.add_log(LogLevel::Error, format!("✗ Error: {}", e));
                Err(e)
            }
        }
    }

    fn add_to_pipeline(&mut self) {
        if self.scripts.is_empty() || !self.scripts[self.selected_script].loaded {
            return;
        }

        let script = &self.scripts[self.selected_script];
        if script.functions.is_empty() {
            return;
        }

        let func = &script.functions[self.function_input.selected_function];
        
        let id = format!("step_{}", self.pipeline.len() + 1);
        let description = format!("{}({})", func, self.function_input.args.join(", "));
        
        self.pipeline.push(PipelineStep {
            id: id.clone(),
            script: script.name.clone(),
            function: func.clone(),
            args: self.function_input.args.clone(),
            description,
        });

        self.add_log(
            LogLevel::Success,
            format!("Added {} to pipeline", id),
        );
    }

    fn execute_pipeline(&mut self) -> Result<(), String> {
        if self.pipeline.is_empty() {
            return Err("Pipeline is empty".into());
        }

        self.add_log(LogLevel::Info, "🚀 Executing pipeline...".into());
        let start = Instant::now();
        let mut success_count = 0;

        // Clone pipeline to avoid borrow issues
        let pipeline = self.pipeline.clone();

        for (i, step) in pipeline.iter().enumerate() {
            self.selected_pipeline_step = i;
            
            let script_idx = self.scripts.iter().position(|s| s.name == step.script);
            if script_idx.is_none() {
                self.add_log(LogLevel::Error, format!("Script {} not found", step.script));
                continue;
            }
            
            let script_idx = script_idx.unwrap();
            if !self.scripts[script_idx].loaded {
                let _ = self.load_script(script_idx);
            }

            let old_input = self.function_input.clone();
            self.function_input.args = step.args.clone();
            
            if let Some(func_idx) = self.scripts[script_idx]
                .functions
                .iter()
                .position(|f| f == &step.function)
            {
                self.function_input.selected_function = func_idx;
                let _ = self.execute_function();
                success_count += 1;
            }
            
            self.function_input = old_input;
        }

        let duration = start.elapsed().as_millis() as u64;
        self.add_log(
            LogLevel::Success,
            format!("Pipeline completed: {}/{} steps in {}ms", success_count, self.pipeline.len(), duration),
        );

        Ok(())
    }

    fn export_pipeline(&self) -> String {
        let mut output = String::new();
        
        output.push_str("// Generated MetaCall Pipeline\n");
        output.push_str("// Export Date: ");
        output.push_str(&Self::timestamp());
        output.push_str("\n\n");
        
        output.push_str("// === Rust Implementation ===\n");
        output.push_str("use metacall::{initialize, load, metacall};\n\n");
        output.push_str("fn execute_pipeline() -> Result<(), String> {\n");
        output.push_str("    let _metacall = initialize()?;\n\n");
        
        let mut loaded_scripts = std::collections::HashSet::new();
        for step in &self.pipeline {
            if !loaded_scripts.contains(&step.script) {
                if let Some(script) = self.scripts.iter().find(|s| s.name == step.script) {
                    output.push_str(&format!(
                        "    load::from_single_file(\"{}\", \"{}\")?;\n",
                        script.runtime,
                        script.path.display()
                    ));
                    loaded_scripts.insert(step.script.clone());
                }
            }
        }
        
        output.push_str("\n");
        
        for (i, step) in self.pipeline.iter().enumerate() {
            output.push_str(&format!("    // Step {}: {}\n", i + 1, step.description));
            output.push_str(&format!(
                "    let result_{}: String = metacall(\"{}\", vec![",
                i + 1,
                step.function
            ));
            
            for (j, arg) in step.args.iter().enumerate() {
                if j > 0 {
                    output.push_str(", ");
                }
                if arg.parse::<i64>().is_ok() {
                    output.push_str(arg);
                } else {
                    output.push_str(&format!("\"{}\".to_string()", arg));
                }
            }
            
            output.push_str("])?;\n");
            output.push_str(&format!("    println!(\"Step {}: {{}}\", result_{});\n\n", i + 1, i + 1));
        }
        
        output.push_str("    Ok(())\n");
        output.push_str("}\n\n");
        
        output.push_str("// === JSON Configuration ===\n");
        output.push_str("/*\n");
        output.push_str(&serde_json::to_string_pretty(&self.pipeline).unwrap_or_default());
        output.push_str("\n*/\n");
        
        output
    }

    fn next_view(&mut self) {
        self.current_view = match self.current_view {
            View::ScriptBrowser => View::FunctionTester,
            View::FunctionTester => View::PipelineBuilder,
            View::PipelineBuilder => View::ResultsExplorer,
            View::ResultsExplorer => View::Export,
            View::Export => View::ScriptBrowser,
        };
    }

    fn prev_view(&mut self) {
        self.current_view = match self.current_view {
            View::ScriptBrowser => View::Export,
            View::FunctionTester => View::ScriptBrowser,
            View::PipelineBuilder => View::FunctionTester,
            View::ResultsExplorer => View::PipelineBuilder,
            View::Export => View::ResultsExplorer,
        };
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(6)])
        .split(f.area());

    render_header(f, app, chunks[0]);
    render_main_view(f, app, chunks[1]);
    render_footer(f, app, chunks[2]);

    if app.show_help {
        render_help_popup(f);
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let titles = vec!["Scripts", "Tester", "Pipeline", "Results", "Export"];
    let selected = match app.current_view {
        View::ScriptBrowser => 0,
        View::FunctionTester => 1,
        View::PipelineBuilder => 2,
        View::ResultsExplorer => 3,
        View::Export => 4,
    };

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .title("🔧 MetaCall Playground")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .select(selected)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, area);
}

fn render_main_view(f: &mut Frame, app: &App, area: Rect) {
    match app.current_view {
        View::ScriptBrowser => render_script_browser(f, app, area),
        View::FunctionTester => render_function_tester(f, app, area),
        View::PipelineBuilder => render_pipeline_builder(f, app, area),
        View::ResultsExplorer => render_results(f, app, area),
        View::Export => render_export(f, app, area),
    }
}

fn render_script_browser(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let items: Vec<ListItem> = app
        .scripts
        .iter()
        .enumerate()
        .map(|(i, script)| {
            let icon = match script.language.as_str() {
                "Python" => "🐍",
                "JavaScript" => "📜",
                "TypeScript" => "📘",
                "Ruby" => "💎",
                _ => "📄",
            };

            let status = if script.loaded {
                "✓"
            } else if script.error.is_some() {
                "✗"
            } else {
                "○"
            };

            let style = if i == app.selected_script {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else if script.loaded {
                Style::default().fg(Color::Green)
            } else if script.error.is_some() {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Gray)
            };

            ListItem::new(format!("{} {} {} [{}]", status, icon, script.name, script.language))
                .style(style)
        })
        .collect();

    f.render_widget(
        List::new(items)
            .block(
                Block::default()
                    .title("📂 Scripts")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(Style::default().bg(Color::DarkGray)),
        chunks[0],
    );

    let details = if !app.scripts.is_empty() {
        let script = &app.scripts[app.selected_script];
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Gray)),
                Span::styled(&script.name, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Language: ", Style::default().fg(Color::Gray)),
                Span::styled(&script.language, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Path: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    script.path.display().to_string(),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            Line::from(""),
        ];

        if let Some(error) = &script.error {
            lines.push(Line::from(vec![
                Span::styled("Error: ", Style::default().fg(Color::Red)),
                Span::styled(error, Style::default().fg(Color::Red)),
            ]));
        } else if script.loaded {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("Functions ({}): ", script.functions.len()),
                    Style::default().fg(Color::Gray),
                ),
            ]));
            
            for func in &script.functions {
                lines.push(Line::from(format!("  • {}", func)));
            }
        } else {
            lines.push(Line::from(Span::styled(
                "Press 'l' to load this script",
                Style::default().fg(Color::Yellow),
            )));
        }

        Paragraph::new(lines)
            .block(
                Block::default()
                    .title("📋 Details")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .padding(Padding::uniform(1)),
            )
            .wrap(Wrap { trim: true })
    } else {
        Paragraph::new("No scripts found")
            .block(
                Block::default()
                    .title("📋 Details")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .alignment(Alignment::Center)
    };

    f.render_widget(details, chunks[1]);
}

fn render_function_tester(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Min(0),
        ])
        .split(area);

    if !app.scripts.is_empty() && app.selected_script < app.scripts.len() {
        let script = &app.scripts[app.selected_script];
        
        let func_items: Vec<ListItem> = script
            .functions
            .iter()
            .enumerate()
            .map(|(i, func)| {
                let style = if i == app.function_input.selected_function {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(format!("{}()", func)).style(style)
            })
            .collect();

        f.render_widget(
            List::new(func_items).block(
                Block::default()
                    .title(format!("🔧 Functions - {}", script.name))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .padding(Padding::horizontal(1)),
            ),
            chunks[0],
        );
    }

    let mut arg_lines = vec![
        Line::from(vec![
            Span::styled("Arguments: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{:?}", app.function_input.args),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
    ];

    if app.input_mode == InputMode::EditingArgs {
        arg_lines.push(Line::from(vec![
            Span::styled(">> ", Style::default().fg(Color::Yellow)),
            Span::styled(&app.input_buffer, Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK)),
        ]));
    } else {
        arg_lines.push(Line::from(Span::styled(
            "Press 'a' to add arguments, 'Enter' to execute",
            Style::default().fg(Color::DarkGray),
        )));
    }

    f.render_widget(
        Paragraph::new(arg_lines).block(
            Block::default()
                .title("⚙️  Input")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .padding(Padding::uniform(1)),
        ),
        chunks[1],
    );

    let result_items: Vec<ListItem> = app
        .results
        .iter()
        .rev()
        .take(10)
        .map(|r| {
            let color = if r.success { Color::Green } else { Color::Red };
            let icon = if r.success { "✓" } else { "✗" };
            
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(format!("{} ", icon), Style::default().fg(color)),
                    Span::styled(&r.function, Style::default().fg(Color::White)),
                    Span::styled(
                        format!(" ({}ms)", r.duration_ms),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  Args: ", Style::default().fg(Color::Gray)),
                    Span::styled(format!("{:?}", r.args), Style::default().fg(Color::DarkGray)),
                ]),
                Line::from(vec![
                    Span::styled("  Result: ", Style::default().fg(Color::Gray)),
                    Span::styled(&r.output, Style::default().fg(color)),
                ]),
            ])
        })
        .collect();

    f.render_widget(
        List::new(result_items).block(
            Block::default()
                .title("📊 Recent Results")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .padding(Padding::horizontal(1)),
        ),
        chunks[2],
    );
}

fn render_pipeline_builder(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    let rows: Vec<Row> = app
        .pipeline
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let style = if i == app.selected_pipeline_step {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            Row::new(vec![
                Cell::from(step.id.clone()),
                Cell::from(step.script.clone()),
                Cell::from(step.function.clone()),
                Cell::from(format!("{:?}", step.args)),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(20),
            Constraint::Length(20),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new(vec!["Step", "Script", "Function", "Arguments"])
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .title("🔗 Pipeline Steps")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray));

    f.render_widget(table, chunks[0]);

    let info_text = if app.pipeline.is_empty() {
        vec![
            Line::from(Span::styled(
                "No steps in pipeline",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Go to Function Tester (Tab) and press 'p' to add steps",
                Style::default().fg(Color::Yellow),
            )),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled("Total Steps: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    app.pipeline.len().to_string(),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Press 'x' to execute pipeline",
                Style::default().fg(Color::Green),
            )),
            Line::from(Span::styled(
                "Press 'd' to delete selected step",
                Style::default().fg(Color::Red),
            )),
            Line::from(Span::styled(
                "Press 'c' to clear all steps",
                Style::default().fg(Color::Yellow),
            )),
        ]
    };

    f.render_widget(
        Paragraph::new(info_text)
            .block(
                Block::default()
                    .title("ℹ️  Info")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .padding(Padding::uniform(1)),
            )
            .alignment(Alignment::Left),
        chunks[1],
    );
}

fn render_results(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let items: Vec<ListItem> = app
        .results
        .iter()
        .rev()
        .enumerate()
        .map(|(i, r)| {
            let color = if r.success { Color::Green } else { Color::Red };
            let icon = if r.success { "✓" } else { "✗" };
            let style = if i == app.selected_result {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };

            ListItem::new(format!(
                "{} [{}] {} - {}ms",
                icon, r.timestamp, r.function, r.duration_ms
            ))
            .style(style)
        })
        .collect();

    f.render_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(format!("📊 All Results ({})", app.results.len()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(Style::default().bg(Color::DarkGray)),
        chunks[0],
    );

    let detail_text = if !app.results.is_empty() && app.selected_result < app.results.len() {
        let r = &app.results[app.results.len() - 1 - app.selected_result];
        let color = if r.success { Color::Green } else { Color::Red };

        vec![
            Line::from(vec![
                Span::styled("Function: ", Style::default().fg(Color::Gray)),
                Span::styled(&r.function, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("Arguments: ", Style::default().fg(Color::Gray)),
                Span::styled(format!("{:?}", r.args), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("Duration: ", Style::default().fg(Color::Gray)),
                Span::styled(format!("{}ms", r.duration_ms), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::styled("Timestamp: ", Style::default().fg(Color::Gray)),
                Span::styled(&r.timestamp, Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    if r.success { "Success" } else { "Failed" },
                    Style::default().fg(color),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled("Output:", Style::default().fg(Color::Gray))),
            Line::from(Span::styled(&r.output, Style::default().fg(color))),
        ]
    } else {
        vec![Line::from(Span::styled(
            "No results yet",
            Style::default().fg(Color::Gray),
        ))]
    };

    f.render_widget(
        Paragraph::new(detail_text)
            .block(
                Block::default()
                    .title("🔍 Details")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .padding(Padding::uniform(1)),
            )
            .wrap(Wrap { trim: true }),
        chunks[1],
    );
}

fn render_export(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(0)])
        .split(area);

    let stats = vec![
        Line::from(vec![
            Span::styled("Scripts Loaded: ", Style::default().fg(Color::Gray)),
            Span::styled(
                app.scripts.iter().filter(|s| s.loaded).count().to_string(),
                Style::default().fg(Color::Green),
            ),
            Span::styled(" / ", Style::default().fg(Color::Gray)),
            Span::styled(
                app.scripts.len().to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("Pipeline Steps: ", Style::default().fg(Color::Gray)),
            Span::styled(
                app.pipeline.len().to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::styled("Total Executions: ", Style::default().fg(Color::Gray)),
            Span::styled(
                app.results.len().to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ];

    f.render_widget(
        Paragraph::new(stats)
            .block(
                Block::default()
                    .title("📈 Statistics")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .padding(Padding::uniform(1)),
            ),
        chunks[0],
    );

    let export_text: Vec<Line> = if app.input_mode == InputMode::ExportName {
        vec![
            Line::from(Span::styled(
                "Enter filename (without extension):",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(">> ", Style::default().fg(Color::Yellow)),
                Span::styled(&app.input_buffer, Style::default().fg(Color::White)),
                Span::styled("_", Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK)),
            ]),
        ]
    } else {
        let code = app.export_pipeline();
        code.lines()
            .map(|line| Line::from(Span::styled(line.to_owned(), Style::default().fg(Color::White))))
            .collect()
    };

    f.render_widget(
        Paragraph::new(export_text)
            .block(
                Block::default()
                    .title("📤 Export Preview (Press 's' to save)")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .padding(Padding::uniform(1)),
            )
            .wrap(Wrap { trim: false })
            .scroll((0, 0)),
        chunks[1],
    );
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3)])
        .split(area);

    let keybinds = match app.current_view {
        View::ScriptBrowser => "↑↓: Select | l: Load | r: Reload | Tab: Next View | ?: Help | q: Quit",
        View::FunctionTester => "↑↓: Select Function | a: Add Args | Enter: Execute | p: Add to Pipeline | Tab: Next View",
        View::PipelineBuilder => "↑↓: Select Step | x: Execute | d: Delete | c: Clear | Tab: Next View",
        View::ResultsExplorer => "↑↓: Navigate | Tab: Next View",
        View::Export => "s: Save to File | Tab: Next View",
    };

    f.render_widget(
        Paragraph::new(keybinds)
            .style(Style::default().fg(Color::Cyan))
            .block(
                Block::default()
                    .title("⌨️  Keybindings")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .alignment(Alignment::Center),
        chunks[0],
    );

    let log_items: Vec<Line> = app
        .logs
        .iter()
        .rev()
        .take(1)
        .map(|log| {
            let (icon, color) = match log.level {
                LogLevel::Info => ("ℹ️", Color::Cyan),
                LogLevel::Success => ("✓", Color::Green),
                LogLevel::Error => ("✗", Color::Red),
                LogLevel::Warning => ("⚠", Color::Yellow),
            };

            Line::from(vec![
                Span::styled(format!("[{}] ", log.timestamp), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ", icon), Style::default().fg(color)),
                Span::styled(&log.message, Style::default().fg(color)),
            ])
        })
        .collect();

    f.render_widget(
        Paragraph::new(log_items)
            .block(
                Block::default()
                    .title("📝 Log")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .alignment(Alignment::Left),
        chunks[1],
    );
}

fn render_help_popup(f: &mut Frame) {
    let area = centered_rect(60, 70, f.area());

    let help_text = vec![
        Line::from(Span::styled(
            "MetaCall Playground - Help",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("Global Keys:", Style::default().fg(Color::Yellow))),
        Line::from("  Tab / Shift+Tab  - Navigate between views"),
        Line::from("  ?                - Toggle this help"),
        Line::from("  q                - Quit application"),
        Line::from(""),
        Line::from(Span::styled("Script Browser:", Style::default().fg(Color::Yellow))),
        Line::from("  ↑ / ↓            - Navigate scripts"),
        Line::from("  l                - Load selected script"),
        Line::from("  r                - Rescan directory"),
        Line::from(""),
        Line::from(Span::styled("Function Tester:", Style::default().fg(Color::Yellow))),
        Line::from("  ↑ / ↓            - Select function"),
        Line::from("  a                - Add argument"),
        Line::from("  c                - Clear arguments"),
        Line::from("  Enter            - Execute function"),
        Line::from("  p                - Add to pipeline"),
        Line::from(""),
        Line::from(Span::styled("Pipeline Builder:", Style::default().fg(Color::Yellow))),
        Line::from("  ↑ / ↓            - Select step"),
        Line::from("  x                - Execute pipeline"),
        Line::from("  d                - Delete selected step"),
        Line::from("  c                - Clear all steps"),
        Line::from(""),
        Line::from(Span::styled("Export View:", Style::default().fg(Color::Yellow))),
        Line::from("  s                - Save pipeline to file"),
        Line::from(""),
        Line::from(Span::styled("Press any key to close", Style::default().fg(Color::DarkGray))),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .title("❓ Help")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .padding(Padding::uniform(2)),
        )
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn handle_input(app: &mut App, key: KeyCode, modifiers: KeyModifiers) -> io::Result<bool> {
    match app.input_mode {
        InputMode::EditingArgs => {
            match key {
                KeyCode::Enter => {
                    if !app.input_buffer.is_empty() {
                        app.function_input.args.push(app.input_buffer.clone());
                        app.input_buffer.clear();
                    }
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Esc => {
                    app.input_buffer.clear();
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Backspace => {
                    app.input_buffer.pop();
                }
                KeyCode::Char(c) => {
                    app.input_buffer.push(c);
                }
                _ => {}
            }
            Ok(false)
        }
        InputMode::ExportName => {
            match key {
                KeyCode::Enter => {
                    if !app.input_buffer.is_empty() {
                        let filename = format!("{}.rs", app.input_buffer);
                        let content = app.export_pipeline();
                        match fs::write(&filename, content) {
                            Ok(_) => {
                                app.add_log(
                                    LogLevel::Success,
                                    format!("Exported to {}", filename),
                                );
                            }
                            Err(e) => {
                                app.add_log(
                                    LogLevel::Error,
                                    format!("Failed to export: {}", e),
                                );
                            }
                        }
                        app.input_buffer.clear();
                    }
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Esc => {
                    app.input_buffer.clear();
                    app.input_mode = InputMode::Normal;
                }
                KeyCode::Backspace => {
                    app.input_buffer.pop();
                }
                KeyCode::Char(c) => {
                    app.input_buffer.push(c);
                }
                _ => {}
            }
            Ok(false)
        }
        InputMode::Normal => {
            if app.show_help {
                app.show_help = false;
                return Ok(false);
            }

            match key {
                KeyCode::Char('q') if modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(true);
                }
                KeyCode::Char('q') => return Ok(true),
                KeyCode::Char('?') => {
                    app.show_help = true;
                }
                KeyCode::Tab => {
                    app.next_view();
                }
                KeyCode::BackTab => {
                    app.prev_view();
                }
                _ => {
                    match app.current_view {
                        View::ScriptBrowser => handle_script_browser_input(app, key),
                        View::FunctionTester => handle_function_tester_input(app, key),
                        View::PipelineBuilder => handle_pipeline_builder_input(app, key),
                        View::ResultsExplorer => handle_results_input(app, key),
                        View::Export => handle_export_input(app, key),
                    }
                }
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn handle_script_browser_input(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Up => {
            if app.selected_script > 0 {
                app.selected_script -= 1;
            }
        }
        KeyCode::Down => {
            if app.selected_script < app.scripts.len().saturating_sub(1) {
                app.selected_script += 1;
            }
        }
        KeyCode::Char('l') => {
            if !app.scripts.is_empty() {
                let _ = app.load_script(app.selected_script);
            }
        }
        KeyCode::Char('r') => {
            app.scan_scripts();
        }
        _ => {}
    }
}

fn handle_function_tester_input(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Up => {
            if !app.scripts.is_empty() && app.selected_script < app.scripts.len() {
                if app.function_input.selected_function > 0 {
                    app.function_input.selected_function -= 1;
                }
            }
        }
        KeyCode::Down => {
            if !app.scripts.is_empty() && app.selected_script < app.scripts.len() {
                let script = &app.scripts[app.selected_script];
                if app.function_input.selected_function < script.functions.len().saturating_sub(1) {
                    app.function_input.selected_function += 1;
                }
            }
        }
        KeyCode::Char('a') => {
            app.input_mode = InputMode::EditingArgs;
            app.input_buffer.clear();
        }
        KeyCode::Char('c') => {
            app.function_input.args.clear();
            app.add_log(LogLevel::Info, "Arguments cleared".to_string());
        }
        KeyCode::Enter => {
            let _ = app.execute_function();
        }
        KeyCode::Char('p') => {
            app.add_to_pipeline();
        }
        _ => {}
    }
}

fn handle_pipeline_builder_input(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Up => {
            if app.selected_pipeline_step > 0 {
                app.selected_pipeline_step -= 1;
            }
        }
        KeyCode::Down => {
            if app.selected_pipeline_step < app.pipeline.len().saturating_sub(1) {
                app.selected_pipeline_step += 1;
            }
        }
        KeyCode::Char('x') => {
            let _ = app.execute_pipeline();
        }
        KeyCode::Char('d') => {
            if !app.pipeline.is_empty() && app.selected_pipeline_step < app.pipeline.len() {
                app.pipeline.remove(app.selected_pipeline_step);
                app.add_log(LogLevel::Success, "Step deleted".to_string());
                if app.selected_pipeline_step >= app.pipeline.len() && app.selected_pipeline_step > 0 {
                    app.selected_pipeline_step -= 1;
                }
            }
        }
        KeyCode::Char('c') => {
            app.pipeline.clear();
            app.selected_pipeline_step = 0;
            app.add_log(LogLevel::Success, "Pipeline cleared".to_string());
        }
        _ => {}
    }
}

fn handle_results_input(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Up => {
            if app.selected_result > 0 {
                app.selected_result -= 1;
            }
        }
        KeyCode::Down => {
            if app.selected_result < app.results.len().saturating_sub(1) {
                app.selected_result += 1;
            }
        }
        _ => {}
    }
}

fn handle_export_input(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('s') => {
            app.input_mode = InputMode::ExportName;
            app.input_buffer = "pipeline".to_string();
        }
        _ => {}
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let root_dir = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        std::env::current_dir()?
    };

    if !root_dir.exists() {
        eprintln!("Error: Directory '{}' does not exist", root_dir.display());
        std::process::exit(1);
    }

    let _metacall = initialize().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("Failed to initialize MetaCall: {:?}", e))
    })?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(root_dir);
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(250);

    loop {
        terminal.draw(|f| ui(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if handle_input(&mut app, key.code, key.modifiers)? {
                        break;
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}