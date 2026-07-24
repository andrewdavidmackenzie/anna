//! Tab-completion for anna CLI commands.

use crate::COMPONENT_NAMES;
use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper};

/// Commands available in the anna interactive CLI.
pub const ANNA_COMMANDS: &[&str] = &[
    "GET",
    "PUT",
    "GET_SET",
    "PUT_SET",
    "GET_ORDERED_SET",
    "PUT_ORDERED_SET",
    "GET_CAUSAL",
    "PUT_CAUSAL",
    "GET_SINGLE_CAUSAL",
    "PUT_SINGLE_CAUSAL",
    "GET_PRIORITY",
    "PUT_PRIORITY",
    "START",
    "STOP",
    "STATUS",
    "HELP",
    "DELETE",
    "EXIT",
];

/// Commands that accept an optional component name argument.
const COMPONENT_COMMANDS: &[&str] = &["START", "STOP", "STATUS"];

/// Provides tab-completion for anna CLI commands.
pub struct AnnaCompleter;

impl Completer for AnnaCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let text = &line[..pos];
        let upper = text.to_ascii_uppercase();

        // If there is a space, we may be completing a component argument
        if let Some(space_pos) = upper.find(' ') {
            let cmd = upper[..space_pos].trim();
            if COMPONENT_COMMANDS.contains(&cmd) {
                let arg_start = space_pos + 1;
                let arg_prefix = text[arg_start..].to_ascii_lowercase();
                let matches: Vec<Pair> = COMPONENT_NAMES
                    .iter()
                    .filter(|name| name.starts_with(arg_prefix.as_str()))
                    .map(|name| Pair {
                        display: name.to_string(),
                        replacement: name.to_string(),
                    })
                    .collect();
                return Ok((arg_start, matches));
            }
            return Ok((pos, vec![]));
        }

        // Complete the command itself
        let matches: Vec<Pair> = ANNA_COMMANDS
            .iter()
            .filter(|cmd| cmd.starts_with(upper.as_str()))
            .map(|cmd| Pair {
                display: cmd.to_string(),
                replacement: cmd.to_string(),
            })
            .collect();
        Ok((0, matches))
    }
}

impl Hinter for AnnaCompleter {
    type Hint = String;
}
impl Highlighter for AnnaCompleter {}
impl Validator for AnnaCompleter {}
impl Helper for AnnaCompleter {}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete(input: &str) -> Vec<String> {
        let completer = AnnaCompleter;
        let (_, pairs) = completer
            .complete(
                input,
                input.len(),
                &Context::new(&rustyline::history::DefaultHistory::new()),
            )
            .expect("completion failed");
        pairs.into_iter().map(|p| p.display).collect()
    }

    #[test]
    fn completes_get() {
        let results = complete("GE");
        assert!(results.contains(&"GET".to_string()));
        assert!(results.contains(&"GET_SET".to_string()));
        assert!(results.contains(&"GET_CAUSAL".to_string()));
    }

    #[test]
    fn completes_put() {
        let results = complete("PU");
        assert!(results.contains(&"PUT".to_string()));
        assert!(results.contains(&"PUT_SET".to_string()));
    }

    #[test]
    fn completes_case_insensitive() {
        let results = complete("ge");
        assert!(results.contains(&"GET".to_string()));
    }

    #[test]
    fn no_completion_after_space_for_non_component_command() {
        let results = complete("GET ");
        assert!(results.is_empty());
    }

    #[test]
    fn empty_prefix_returns_all() {
        let results = complete("");
        assert_eq!(results.len(), ANNA_COMMANDS.len());
    }

    #[test]
    fn exact_match() {
        let results = complete("EXIT");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "EXIT");
    }

    #[test]
    fn start_completes_components() {
        let results = complete("START ");
        assert_eq!(results.len(), 3);
        assert!(results.contains(&"monitor".to_string()));
        assert!(results.contains(&"route".to_string()));
        assert!(results.contains(&"kvs".to_string()));
    }

    #[test]
    fn stop_completes_components() {
        let results = complete("STOP ");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn status_completes_components() {
        let results = complete("STATUS ");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn start_filters_component_prefix() {
        let results = complete("START k");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "kvs");
    }

    #[test]
    fn component_completion_case_insensitive() {
        let results = complete("START M");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], "monitor");
    }
}
