// The `intent` command: train, evaluate, and try the Stage 2 model. See intent/README.md.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use dataset::template::Template;
use intent::batch::encode;
use intent::data::{self, Example};
use intent::metrics::{Report, evaluate};
use intent::model::ModelConfig;
use intent::predict::{DEFAULT_THRESHOLD, Prediction, Predictor};
use intent::store::{self, Meta, TrainingSummary};
use intent::train::{TrainConfig, bow_accuracy, train_bow, train_joint};
use intent::vocab::{Labels, Vocab};

const USAGE: &str = "\
Train, evaluate and use the Stage 2 intent + slot model.

Usage:
  intent train   [--data DIR] [--out DIR] [--epochs N] [--seed N] [--show-test]
  intent eval    [--data DIR] [--model DIR] [--set test|dev]
  intent predict [--model DIR] [--threshold P] <request...>

Options:
  --data DIR       Command dataset directory        [default: data/commands]
  --out DIR        Where train saves the model      [default: data/models/intent]
  --model DIR      Where eval/predict load it from  [default: data/models/intent]
  --epochs N       Maximum training epochs          [default: 40]
  --seed N         Random seed                      [default: 42]
  --threshold P    Minimum confidence to suggest a command, 0..1  [default: 0.7]
  --set NAME       Which hand-written set eval scores: test or dev  [default: test]
  --show-test      Also report the test set after training (keep it for the final run:
                   decisions must be based on the development set only)
  --help           Show this message

Generate the dataset with `make commands-data` first.";

const VALIDATION_FRACTION: f64 = 0.1;
const MIN_WORD_COUNT: usize = 2;
const D_EMBED: usize = 64;
const D_HIDDEN: usize = 64;

/// Why the program stopped: a usage mistake (exit 2) or a runtime failure (exit 1).
enum Failure {
    Usage(String),
    Run(String),
}

struct Options {
    data: PathBuf,
    model: PathBuf,
    epochs: usize,
    seed: u64,
    threshold: f32,
    set: String,
    show_test: bool,
    words: Vec<String>,
}

fn parse_options(args: &[String], allowed: &[&str]) -> Result<Options, Failure> {
    let mut o = Options {
        data: PathBuf::from("data/commands"),
        model: PathBuf::from("data/models/intent"),
        epochs: TrainConfig::default().epochs,
        seed: TrainConfig::default().seed,
        threshold: DEFAULT_THRESHOLD,
        set: "test".to_string(),
        show_test: false,
        words: Vec::new(),
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if !arg.starts_with("--") {
            o.words.push(arg.clone());
            continue;
        }
        if !allowed.contains(&arg.as_str()) {
            return Err(Failure::Usage(format!("unknown option '{arg}'")));
        }
        if arg == "--show-test" {
            o.show_test = true;
            continue;
        }
        let value = iter
            .next()
            .ok_or_else(|| Failure::Usage(format!("{arg} needs a value")))?;
        let bad = || Failure::Usage(format!("{arg}: invalid value '{value}'"));
        match arg.as_str() {
            "--data" => o.data = PathBuf::from(value),
            "--out" | "--model" => o.model = PathBuf::from(value),
            "--epochs" => o.epochs = value.parse().ok().filter(|&n| n > 0).ok_or_else(bad)?,
            "--seed" => o.seed = value.parse().map_err(|_| bad())?,
            "--set" if value == "test" || value == "dev" => o.set = value.clone(),
            "--set" => return Err(bad()),
            _ => {
                o.threshold = value
                    .parse()
                    .ok()
                    .filter(|p: &f32| (0.0..=1.0).contains(p))
                    .ok_or_else(bad)?
            }
        }
    }
    Ok(o)
}

fn predict_all(predictor: &Predictor, examples: &[Example]) -> Vec<Prediction> {
    // Threshold 0: always fill in a command, so command accuracy measures the model itself.
    examples
        .iter()
        .map(|e| predictor.predict(&e.text, 0.0))
        .collect()
}

fn report(predictor: &Predictor, examples: &[Example]) -> Report {
    evaluate(examples, &predict_all(predictor, examples))
}

fn train(o: &Options) -> Result<(), Failure> {
    let start = Instant::now();
    let commands = data::load(&o.data).map_err(Failure::Run)?;
    let (fit, val) = data::split_validation(&commands.train, VALIDATION_FRACTION, o.seed);

    // Labels: intents in intents.json order; tags for every slot any template uses.
    let mut slot_names = BTreeSet::new();
    for info in &commands.intents {
        if let Some(command) = &info.command {
            let template = Template::parse(command).map_err(Failure::Run)?;
            slot_names.extend(template.slots().into_iter().map(|(n, _)| n.to_string()));
        }
    }
    let labels = Labels::new(
        commands.intents.iter().map(|i| i.name.clone()).collect(),
        slot_names,
    );
    let vocab = Vocab::build(&fit, MIN_WORD_COUNT);
    let fit_enc = encode(&fit, &vocab, &labels).map_err(Failure::Run)?;
    let val_enc = encode(&val, &vocab, &labels).map_err(Failure::Run)?;
    let dev_enc = encode(&commands.dev, &vocab, &labels).map_err(Failure::Run)?;
    let test_enc = encode(&commands.test, &vocab, &labels).map_err(Failure::Run)?;
    let config = ModelConfig {
        vocab_size: vocab.len(),
        n_intents: labels.intents.len(),
        n_tags: labels.tags.len(),
        d_embed: D_EMBED,
        d_hidden: D_HIDDEN,
    };
    let tc = TrainConfig {
        epochs: o.epochs,
        seed: o.seed,
        ..TrainConfig::default()
    };

    println!("=== Training the intent + slot model (Burn, CPU) ===\n");
    println!(
        "Data: {} training, {} validation, {} development, {} test sentences | vocabulary {} | {} intents | {} tags",
        fit.len(),
        val.len(),
        commands.dev.len(),
        commands.test.len(),
        vocab.len(),
        labels.intents.len(),
        labels.tags.len()
    );

    println!("\nBaseline (bag of words → dense), intent only:");
    let baseline = train_bow(
        vocab.len(),
        labels.intents.len(),
        &fit_enc,
        &val_enc,
        &dev_enc,
        &tc,
    );
    let baseline_val = baseline.best.val_intent_accuracy;
    let baseline_dev = baseline.best.dev_intent_accuracy;
    let baseline_test = bow_accuracy(&baseline.model, &test_enc, vocab.len());
    println!(
        "  kept epoch {} | validation intent {:.1}% | development intent {:.1}%",
        baseline.best_epoch,
        100.0 * baseline_val,
        100.0 * baseline_dev
    );

    println!(
        "\nJoint model (embedding {D_EMBED} → BiGRU {D_HIDDEN}×2 → intent + slot heads), seed {}:",
        o.seed
    );
    let joint = train_joint(&config, &fit_enc, &val_enc, &dev_enc, &tc, |log| {
        println!(
            "  epoch {:>3} | loss {:.4} | validation intent {:>5.1}% | development intent {:>5.1}%, exact {:>5.1}%",
            log.epoch,
            log.loss,
            100.0 * log.val_intent_accuracy,
            100.0 * log.dev_intent_accuracy,
            100.0 * log.dev_exact
        );
    });
    println!(
        "  kept epoch {} (best development exact match)",
        joint.best_epoch
    );

    let meta = Meta {
        config,
        vocab,
        labels,
        intents: commands.intents.clone(),
        training: TrainingSummary {
            seed: o.seed,
            best_epoch: joint.best_epoch,
            validation_intent_accuracy: joint.best.val_intent_accuracy,
            baseline_validation_intent_accuracy: baseline_val,
        },
    };
    store::save(&o.model, &joint.model, &meta).map_err(Failure::Run)?;
    let predictor = Predictor::new(joint.model, meta).map_err(Failure::Run)?;

    let val_report = report(&predictor, &val);
    let dev_report = report(&predictor, &commands.dev);
    println!("\n=== Validation (held out from the generated training data) ===\n{val_report}");
    println!("=== Development (hand-written; guides decisions) ===\n{dev_report}");
    let test_report = o.show_test.then(|| report(&predictor, &commands.test));
    if let Some(r) = &test_report {
        println!("=== Test (hand-written; report only) ===\n{r}");
    }
    let test_cell = |x: f64| match &test_report {
        Some(_) => format!("{:>5.1}%", 100.0 * x),
        None => "    –".to_string(),
    };
    println!("=== Baseline vs joint model (intent accuracy) ===");
    println!("                 validation  development    test");
    println!(
        "  bag of words      {:>5.1}%      {:>5.1}%     {}",
        100.0 * baseline_val,
        100.0 * baseline_dev,
        test_cell(baseline_test)
    );
    println!(
        "  joint model       {:>5.1}%      {:>5.1}%     {}",
        100.0 * val_report.intent_accuracy,
        100.0 * dev_report.intent_accuracy,
        test_cell(test_report.as_ref().map_or(0.0, |r| r.intent_accuracy))
    );
    if test_report.is_none() {
        println!(
            "  (test set not shown; use --show-test or `make intent-eval` for the final report)"
        );
    }
    println!(
        "\nSaved to {}/ in {:.1}s.",
        o.model.display(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn eval(o: &Options) -> Result<(), Failure> {
    let predictor = Predictor::load(&o.model).map_err(Failure::Run)?;
    let path = o.data.join(format!("{}.jsonl", o.set));
    let examples = data::read_examples(&path).map_err(Failure::Run)?;
    println!(
        "=== {} set ({}) with the model in {} ===",
        if o.set == "dev" {
            "Development"
        } else {
            "Test"
        },
        path.display(),
        o.model.display()
    );
    print!("{}", report(&predictor, &examples));
    Ok(())
}

fn predict(o: &Options) -> Result<(), Failure> {
    if o.words.is_empty() {
        return Err(Failure::Usage(
            "predict needs a request, e.g. intent predict show my files".into(),
        ));
    }
    let predictor = Predictor::load(Path::new(&o.model)).map_err(Failure::Run)?;
    let text = o.words.join(" ");
    let p = predictor.predict(&text, o.threshold);
    println!("request:     {text}");
    println!("intent:      {} (confidence {:.2})", p.intent, p.confidence);
    if !p.slots.is_empty() {
        let slots: Vec<String> = p.slots.iter().map(|(k, v)| format!("{k} = {v}")).collect();
        println!("slots:       {}", slots.join(", "));
    }
    match (&p.command, &p.reason) {
        (Some(command), _) => println!("command:     {command}"),
        (None, Some(reason)) => println!("no command:  {reason}"),
        (None, None) => println!("no command"),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return if args.is_empty() {
            ExitCode::from(2)
        } else {
            ExitCode::SUCCESS
        };
    }
    let rest = &args[1..];
    let result = match args[0].as_str() {
        "train" => parse_options(
            rest,
            &["--data", "--out", "--epochs", "--seed", "--show-test"],
        )
        .and_then(|o| {
            if o.words.is_empty() {
                train(&o)
            } else {
                Err(Failure::Usage(format!(
                    "unexpected argument '{}'",
                    o.words[0]
                )))
            }
        }),
        "eval" => parse_options(rest, &["--data", "--model", "--set"]).and_then(|o| eval(&o)),
        "predict" => parse_options(rest, &["--model", "--threshold"]).and_then(|o| predict(&o)),
        other => Err(Failure::Usage(format!("unknown command '{other}'"))),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("error: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Run(message)) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}
