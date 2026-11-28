// Generate the Stage 2 command dataset: validate the catalog, expand it into training and
// development and test examples, and write them to JSON Lines files. See dataset/README.md.

mod generate;
mod grammar;
mod random;
mod validate;

// Shared with other crates through the library (src/lib.rs).
use dataset::{catalog, template, tokenize};

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use generate::{Example, generate_dev, generate_test, generate_train, intent_infos};
use random::Rng;
use validate::{check_dev_test_disjoint, check_disjoint, counts_per_intent, prepare};

const USAGE: &str = "\
Generate the command dataset (intents, slots, phrasings) for the Stage 2 models.

Usage: dataset [OPTIONS]

Options:
  --out <DIR>         Output directory                    [default: data/commands]
  --seed <N>          Random seed (same seed = same files) [default: 42]
  --per-intent <N>    Training examples per intent        [default: 150]
  --help              Show this message";

#[derive(Debug, PartialEq)]
struct Config {
    out: PathBuf,
    seed: u64,
    per_intent: usize,
}

/// None means "--help was given".
fn parse_args(args: &[String]) -> Result<Option<Config>, String> {
    let mut config = Config {
        out: PathBuf::from("data/commands"),
        seed: 42,
        per_intent: 150,
    };
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        if flag == "--help" || flag == "-h" {
            return Ok(None);
        }
        if !matches!(flag.as_str(), "--out" | "--seed" | "--per-intent") {
            return Err(format!("unknown option '{flag}'"));
        }
        let value = iter.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--out" => config.out = PathBuf::from(value),
            "--seed" => {
                config.seed = value
                    .parse()
                    .map_err(|_| format!("--seed expects a whole number, got '{value}'"))?
            }
            _ => {
                config.per_intent = match value.parse::<usize>() {
                    Ok(0) => return Err("--per-intent must be greater than 0".to_string()),
                    Ok(n) => n,
                    Err(_) => {
                        return Err(format!(
                            "--per-intent expects a whole number, got '{value}'"
                        ));
                    }
                }
            }
        }
    }
    Ok(Some(config))
}

/// Write to `<name>.tmp` first, then rename, so a crash never leaves a half-written file.
fn write_atomic(dir: &Path, name: &str, contents: &str) -> Result<(), String> {
    let path = dir.join(name);
    let tmp = dir.join(format!("{name}.tmp"));
    let result = fs::File::create(&tmp)
        .and_then(|mut f| f.write_all(contents.as_bytes()))
        .and_then(|_| fs::rename(&tmp, &path));
    result.map_err(|e| format!("could not write {}: {e}", path.display()))
}

fn to_jsonl(examples: &[Example]) -> Result<String, String> {
    let mut out = String::new();
    for e in examples {
        out.push_str(&serde_json::to_string(e).map_err(|e| e.to_string())?);
        out.push('\n');
    }
    Ok(out)
}

fn run(config: &Config) -> Result<(), Vec<String>> {
    let catalog = catalog::catalog();
    let intents = prepare(&catalog)?;

    // Separate generators so adding held-out sentences never changes the training data
    // (and adding development sentences never changes the test data).
    let mut train_rng = Rng::new(config.seed);
    let mut test_rng = Rng::new(config.seed.wrapping_add(1));
    let mut dev_rng = Rng::new(config.seed.wrapping_add(2));
    let test = generate_test(&intents, catalog.pools, &mut test_rng).map_err(|e| vec![e])?;
    let dev = generate_dev(&intents, catalog.pools, &mut dev_rng).map_err(|e| vec![e])?;
    let held_out: Vec<Example> = dev.iter().chain(&test).cloned().collect();
    let train = generate_train(
        &intents,
        catalog.pools,
        config.per_intent,
        &held_out,
        &mut train_rng,
    )
    .map_err(|e| vec![e])?;
    let mut errors = Vec::new();
    for check in [
        check_disjoint(&train, &test, "test"),
        check_disjoint(&train, &dev, "development"),
        check_dev_test_disjoint(&dev, &test),
    ] {
        errors.extend(check.err().unwrap_or_default());
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // Everything is valid; only now touch the disk.
    let infos =
        serde_json::to_string_pretty(&intent_infos(&intents)).map_err(|e| vec![e.to_string()])?;
    let train_text = to_jsonl(&train).map_err(|e| vec![e])?;
    let test_text = to_jsonl(&test).map_err(|e| vec![e])?;
    let dev_text = to_jsonl(&dev).map_err(|e| vec![e])?;
    fs::create_dir_all(&config.out)
        .map_err(|e| vec![format!("could not create {}: {e}", config.out.display())])?;
    for (name, contents) in [
        ("intents.json", infos + "\n"),
        ("train.jsonl", train_text),
        ("dev.jsonl", dev_text),
        ("test.jsonl", test_text),
    ] {
        write_atomic(&config.out, name, &contents).map_err(|e| vec![e])?;
    }

    let counts = counts_per_intent(&train);
    let min = counts.values().min().copied().unwrap_or(0);
    let max = counts.values().max().copied().unwrap_or(0);
    let phrasings: usize = intents.iter().map(|i| i.phrasings.len()).sum();
    println!("Command dataset written to {}/", config.out.display());
    println!(
        "  intents:            {} ({} commands + unknown)",
        intents.len(),
        intents.len() - 1
    );
    println!("  distinct phrasings: {phrasings}");
    println!(
        "  training examples:  {} ({min}–{max} per intent, seed {})",
        train.len(),
        config.seed
    );
    println!("  development:        {}", dev.len());
    println!("  test examples:      {}", test.len());
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match parse_args(&args) {
        Ok(Some(config)) => config,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(errors) => {
            eprintln!("error: the dataset was not written:");
            for e in errors {
                eprintln!("  - {e}");
            }
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(list: &[&str]) -> Result<Option<Config>, String> {
        parse_args(&list.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn test_defaults_and_overrides() {
        let c = parse(&[]).unwrap().unwrap();
        assert_eq!(
            (c.out, c.seed, c.per_intent),
            (PathBuf::from("data/commands"), 42, 150)
        );
        let c = parse(&["--out", "x", "--seed", "1", "--per-intent", "5"])
            .unwrap()
            .unwrap();
        assert_eq!((c.out, c.seed, c.per_intent), (PathBuf::from("x"), 1, 5));
    }

    #[test]
    fn test_bad_args() {
        assert!(parse(&["--per-intent", "0"]).is_err());
        assert!(parse(&["--per-intent", "many"]).is_err());
        assert!(parse(&["--seed"]).is_err());
        assert!(parse(&["--verbose"]).is_err());
        assert_eq!(parse(&["--help"]), Ok(None));
    }

    #[test]
    fn test_overlapping_test_set_rejected() {
        let example = |text: &str, value: &str| Example {
            text: text.to_string(),
            tokens: Vec::new(),
            tags: Vec::new(),
            intent: "list_files".to_string(),
            slots: [("dir".to_string(), value.to_string())].into(),
            command: None,
        };
        let train = [example("show the files in src", "src")];
        let test = [example("Show the files in SRC", "src")];
        let errors = check_disjoint(&train, &test, "test").unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("also appears in training"))
        );
        assert!(errors.iter().any(|e| e.contains("test slot value \"src\"")));
        assert!(
            check_disjoint(
                &train,
                &[example("list everything in build", "build")],
                "test"
            )
            .is_ok()
        );
    }
}
