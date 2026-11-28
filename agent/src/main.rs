// `cyore`: the Stage 3 agent. Type what you want in plain English; Cyore proposes a command,
// shows it, and runs it only if you answer "y". See agent/README.md.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use agent::agent::{Agent, Settings};
use agent::embedded::{self, ModelSource};
use agent::runner::ProcessRunner;
use agent::tools::ModelInterpreter;
use intent::predict::{DEFAULT_THRESHOLD, Predictor};

const DEFAULT_DIR: &str = "data/models/intent";

const USAGE: &str = "\
Cyore: plain English in, terminal commands out. Nothing runs until you answer \"y\".

Usage:
  cyore [OPTIONS]              interactive: type requests, `help` for tools, `quit` to leave
  cyore [OPTIONS] <request...> one request, then exit

Options:
  --model DIR        Load the model from this directory instead of the one built in
                     (without a built-in model: data/models/intent)
  --threshold P      Minimum model confidence, 0..1        [default: 0.7]
  --timeout SECS     Kill a command after this many seconds [default: 10]
  --dry-run          Show commands but never run them
  --help             Show this message";

struct Options {
    model: Option<PathBuf>,
    threshold: f32,
    timeout: u64,
    dry_run: bool,
    request: Vec<String>,
}

fn parse(args: &[String]) -> Result<Option<Options>, String> {
    let mut o = Options {
        model: None,
        threshold: DEFAULT_THRESHOLD,
        timeout: 10,
        dry_run: false,
        request: Vec::new(),
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--dry-run" => o.dry_run = true,
            "--model" | "--threshold" | "--timeout" => {
                let value = iter.next().ok_or_else(|| format!("{arg} needs a value"))?;
                let bad = || format!("{arg}: invalid value '{value}'");
                match arg.as_str() {
                    "--model" => o.model = Some(PathBuf::from(value)),
                    "--threshold" => {
                        o.threshold = value
                            .parse()
                            .ok()
                            .filter(|p: &f32| (0.0..=1.0).contains(p))
                            .ok_or_else(bad)?
                    }
                    _ => o.timeout = value.parse().ok().filter(|&s| s > 0).ok_or_else(bad)?,
                }
            }
            flag if flag.starts_with("--") => return Err(format!("unknown option '{flag}'")),
            word => o.request.push(word.to_string()),
        }
    }
    Ok(Some(o))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(Some(o)) => o,
        Ok(None) => {
            let source =
                embedded::choose(None, embedded::embedded().is_some(), Path::new(DEFAULT_DIR));
            println!(
                "{USAGE}\n\nThis build uses: {}",
                embedded::describe(&source)
            );
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    // Stage 4: --model DIR, else the int8 model built into this binary, else the default dir.
    let source = embedded::choose(
        options.model.as_deref(),
        embedded::embedded().is_some(),
        Path::new(DEFAULT_DIR),
    );
    let loaded = match (&source, embedded::embedded()) {
        (ModelSource::Embedded, Some((weights, meta))) => Predictor::from_static(weights, meta),
        (ModelSource::Dir(dir), _) => Predictor::load(dir),
        (ModelSource::Embedded, None) => Err("no embedded model".to_string()),
    };
    let predictor = match loaded {
        Ok(p) => p,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };
    let interpreter = ModelInterpreter {
        predictor,
        threshold: options.threshold,
    };
    let runner = ProcessRunner::default();
    let settings = Settings {
        timeout: Duration::from_secs(options.timeout),
        dry_run: options.dry_run,
        home: std::env::var("HOME").ok(),
    };
    let mut agent = Agent::new(Box::new(interpreter), &runner, settings);

    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut out = io::stdout().lock();
    let result = if options.request.is_empty() {
        let interactive = io::stdin().is_terminal();
        if interactive {
            let _ = writeln!(
                out,
                "Cyore: tell me what you want to do (`help` for tools, `quit` to leave). Using the {}.{}",
                embedded::describe(&source),
                if options.dry_run {
                    " Dry run: nothing will be executed."
                } else {
                    ""
                }
            );
        }
        agent
            .run_interactive(&mut input, &mut out, interactive)
            .map(|_| true)
    } else {
        agent
            .handle(&options.request.join(" "), &mut input, &mut out)
            .map(|report| !report.failed)
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
