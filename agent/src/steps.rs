// Perceive: split one request into steps (design.md, decision 6).
// "show disk space and then list the files" → ["show disk space", "list the files"]
// Splits at `;` and at the whole word "then"; an "and" or a comma right before "then" goes too.

pub fn split_steps(request: &str) -> Vec<String> {
    let mut steps = Vec::new();
    for part in request.split(';') {
        let mut current: Vec<&str> = Vec::new();
        for word in part.split_whitespace() {
            if word.eq_ignore_ascii_case("then") {
                // Drop a connecting "and" ("… and then …") and a trailing comma ("…, then …").
                if current
                    .last()
                    .is_some_and(|w| w.eq_ignore_ascii_case("and"))
                {
                    current.pop();
                }
                if let Some(last) = current.last_mut() {
                    *last = last.trim_end_matches(',');
                }
                push(&mut steps, &current);
                current.clear();
            } else {
                current.push(word);
            }
        }
        push(&mut steps, &current);
    }
    steps
}

fn push(steps: &mut Vec<String>, words: &[&str]) {
    let step = words.join(" ");
    let step = step.trim().trim_end_matches(',').trim();
    if !step.is_empty() {
        steps.push(step.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_step() {
        assert_eq!(split_steps("show my files"), ["show my files"]);
    }

    #[test]
    fn test_then_variants() {
        assert_eq!(
            split_steps("show disk space and then list the files"),
            ["show disk space", "list the files"]
        );
        assert_eq!(
            split_steps("show disk space, then list the files"),
            ["show disk space", "list the files"]
        );
        assert_eq!(split_steps("pwd THEN ls"), ["pwd", "ls"]);
        assert_eq!(split_steps("pwd then ls then date"), ["pwd", "ls", "date"]);
    }

    #[test]
    fn test_semicolon() {
        assert_eq!(split_steps("pwd; ls ;date"), ["pwd", "ls", "date"]);
    }

    #[test]
    fn test_whole_words_only() {
        assert_eq!(split_steps("strengthen athena"), ["strengthen athena"]);
    }

    #[test]
    fn test_empty_steps_dropped() {
        assert!(split_steps("").is_empty());
        assert!(split_steps(" ; then ;; ").is_empty());
        assert_eq!(split_steps("then pwd then"), ["pwd"]);
    }
}
