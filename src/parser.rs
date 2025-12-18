//! Parse tmux pane content into command blocks

use regex::Regex;
use serde_json::Value;

/// Default regex pattern for detecting shell prompts.
/// Matches zsh-style prompts: path starting with / or ~, followed by ` % `
pub const DEFAULT_PROMPT_REGEX: &str = r#"^[/~].* % "#;

/// A block representing a command and its output
#[derive(Debug, Clone)]
pub struct CommandBlock {
    /// The command line (including prompt, with ANSI codes)
    pub command: String,
    /// The command line with ANSI codes stripped (for display in list)
    pub clean_command: String,
    /// The command text only (prompt removed, ANSI stripped) for copying
    pub command_text: String,
    /// The output following the command
    pub output: String,
}

/// A block representing a detected JSON object
#[derive(Debug, Clone)]
pub struct JsonBlock {
    /// Short description (e.g. "{ key1, key2, ... }")
    pub name: String,
    /// Formatted JSON for display
    pub pretty: String,
    /// Raw JSON string for copying
    pub raw: String,
    /// Parsed JSON value for syntax highlighting
    pub value: Value,
}

/// Tracks shell syntax state for detecting incomplete commands
#[derive(Default)]
struct CommandState {
    in_single_quote: bool,
    in_double_quote: bool,
    trailing_backslash: bool,
    trailing_operator: bool,
}

impl CommandState {
    fn reset(&mut self) {
        self.in_single_quote = false;
        self.in_double_quote = false;
        self.trailing_backslash = false;
        self.trailing_operator = false;
    }

    /// Process a line and update state. Returns true if command is incomplete
    /// (needs continuation on next line).
    fn process_line(&mut self, line: &str) -> bool {
        let line = line.trim_end();
        let mut escaped = false;

        for c in line.chars() {
            if escaped {
                escaped = false;
                continue;
            }

            match c {
                '\\' => escaped = true,
                // Single quotes toggle only outside double quotes
                '\'' if !self.in_double_quote => self.in_single_quote = !self.in_single_quote,
                // Double quotes toggle only outside single quotes
                '"' if !self.in_single_quote => self.in_double_quote = !self.in_double_quote,
                _ => {}
            }
        }

        // Trailing backslash means line continuation
        self.trailing_backslash = escaped;

        // Check for trailing operators that expect continuation (only outside quotes)
        self.trailing_operator = false;
        if !self.in_single_quote && !self.in_double_quote && !self.trailing_backslash {
            let trimmed = line.trim_end();
            self.trailing_operator =
                trimmed.ends_with('|') || trimmed.ends_with("&&") || trimmed.ends_with("||");
        }

        self.needs_continuation()
    }

    /// Returns true if command needs continuation
    fn needs_continuation(&self) -> bool {
        self.in_single_quote
            || self.in_double_quote
            || self.trailing_backslash
            || self.trailing_operator
    }
}

/// Check if a line is likely right-aligned git status (not real output)
fn is_git_status_line(line: &str) -> bool {
    let clean_bytes = strip_ansi_escapes::strip(line);
    let clean = String::from_utf8_lossy(&clean_bytes);
    let trimmed = clean.trim();

    // Must start with significant whitespace (10+ spaces)
    let leading_spaces = clean.len() - clean.trim_start().len();
    if leading_spaces < 10 {
        return false;
    }

    // Check for common git status indicators
    trimmed.contains("main")
        || trimmed.contains("master")
        || trimmed.contains('✱')
        || trimmed.contains('✚')
        || trimmed.contains('✖')
}

fn build_block(
    command_lines: &[&str],
    output_lines: &[&str],
    prompt_regex: &Regex,
) -> CommandBlock {
    let full_command = command_lines.join("\n");
    let clean_bytes = strip_ansi_escapes::strip(&full_command);

    // Filter out git status lines from output
    let filtered_output: Vec<&str> = output_lines
        .iter()
        .filter(|line| !is_git_status_line(line))
        .copied()
        .collect();

    // Build command_text: strip prompt from first line, keep continuation lines as-is
    let mut command_text = String::new();

    if let Some(first) = command_lines.first() {
        let clean_first_bytes = strip_ansi_escapes::strip(first);
        let clean_first = String::from_utf8_lossy(&clean_first_bytes);

        if let Some(cmd) = extract_command(&clean_first, prompt_regex) {
            command_text.push_str(&cmd);
        } else {
            // Fallback (shouldn't happen due to parse_history logic)
            command_text.push_str(clean_first.trim());
        }
    }

    // Continuation lines (no prompt, just strip ANSI)
    for line in command_lines.iter().skip(1) {
        let clean_bytes = strip_ansi_escapes::strip(line);
        let clean_line = String::from_utf8_lossy(&clean_bytes);
        command_text.push('\n');
        command_text.push_str(&clean_line);
    }

    CommandBlock {
        command: full_command,
        clean_command: String::from_utf8_lossy(&clean_bytes).into_owned(),
        command_text,
        output: filtered_output.join("\n"),
    }
}

/// Extract the command portion after the prompt (trimmed, right-prompt stripped)
fn extract_command(clean_line: &str, prompt_regex: &Regex) -> Option<String> {
    if let Some(m) = prompt_regex.find(clean_line) {
        let after_prompt = &clean_line[m.end()..];

        // If empty or whitespace-only, no command
        if after_prompt.trim().is_empty() {
            return None;
        }

        // Detect right-aligned git status: if text starts with many spaces (10+),
        // it's likely right-aligned status info (e.g., "main ✚"), not a command
        let leading_spaces = after_prompt.len() - after_prompt.trim_start().len();
        if leading_spaces >= 10 {
            return None;
        }

        // Strip right-side prompt: find sequence of 10+ spaces and take only what's before
        let trimmed = after_prompt.trim_start();
        let command = if let Some(pos) = find_right_prompt_start(trimmed) {
            trimmed[..pos].trim_end()
        } else {
            trimmed.trim_end()
        };

        if command.is_empty() {
            return None;
        }

        Some(command.to_string())
    } else {
        None
    }
}

/// Find the start of right-aligned prompt (10+ consecutive spaces)
fn find_right_prompt_start(s: &str) -> Option<usize> {
    let mut space_count = 0;
    let mut space_start = 0;

    for (i, c) in s.char_indices() {
        if c == ' ' {
            if space_count == 0 {
                space_start = i;
            }
            space_count += 1;
            if space_count >= 10 {
                return Some(space_start);
            }
        } else {
            space_count = 0;
        }
    }
    None
}

/// Parse raw terminal content into command blocks
///
/// Handles multiline commands by tracking shell syntax state (quotes, backslash
/// continuations). ANSI escape codes are stripped for regex matching but
/// preserved in the output for display.
pub fn parse_history(raw_content: &str, prompt_regex: &Regex) -> Vec<CommandBlock> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = raw_content.lines().collect();

    let mut current_command_lines: Vec<&str> = Vec::new();
    let mut current_output: Vec<&str> = Vec::new();
    let mut state = CommandState::default();
    let mut last_command: Option<String> = None; // Track last command for deduplication

    for line in lines {
        // Strip ANSI codes for regex matching and syntax analysis
        let clean_bytes = strip_ansi_escapes::strip(line);
        let clean_line = String::from_utf8_lossy(&clean_bytes);

        // Check if line matches prompt pattern structurally
        let is_prompt_match = prompt_regex.is_match(&clean_line);

        // Extract command if prompt has actual command text
        let extracted_cmd = extract_command(&clean_line, prompt_regex);
        let is_valid_prompt = extracted_cmd.is_some();

        // Skip duplicate consecutive commands (tmux captures both typing and after Enter)
        let is_duplicate = if let (Some(cmd), Some(last)) = (&extracted_cmd, &last_command) {
            cmd == last
        } else {
            false
        };

        if is_valid_prompt && !state.needs_continuation() && !is_duplicate {
            // New prompt detected and previous command is complete
            // Push previous block if exists
            if !current_command_lines.is_empty() {
                blocks.push(build_block(
                    &current_command_lines,
                    &current_output,
                    prompt_regex,
                ));
            }

            // Start new block
            current_command_lines.clear();
            current_output.clear();
            state.reset();

            current_command_lines.push(line);
            state.process_line(&clean_line);
            last_command = extracted_cmd;
        } else if is_duplicate {
            // Skip duplicate prompt lines entirely (don't add to output)
            continue;
        } else if state.needs_continuation() {
            // Previous line was incomplete, this is a continuation
            current_command_lines.push(line);
            state.process_line(&clean_line);
        } else if !current_command_lines.is_empty() {
            // Skip empty prompts (match regex but no command) - don't add to output
            if is_prompt_match {
                continue;
            }
            // Command is complete, this is output
            current_output.push(line);
        }
        // Lines before first prompt are ignored
    }

    // Push final block
    if !current_command_lines.is_empty() {
        blocks.push(build_block(
            &current_command_lines,
            &current_output,
            prompt_regex,
        ));
    }

    // Reverse so newest commands appear first
    blocks.reverse();

    blocks
}

/// Scan command blocks for valid JSON substrings
pub fn find_json_candidates(blocks: &[CommandBlock]) -> Vec<JsonBlock> {
    let mut json_blocks = Vec::new();

    for block in blocks {
        // Strip ANSI codes from output before JSON parsing
        let clean_bytes = strip_ansi_escapes::strip(&block.output);
        let text = String::from_utf8_lossy(&clean_bytes);
        if text.trim().is_empty() {
            continue;
        }

        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];
            // Check for start of object or array
            if c == '{' || c == '[' {
                let start_index = i;
                let mut stack = vec![c];
                let mut j = i + 1;
                let mut in_string = false;
                let mut escaped = false;

                while j < chars.len() && !stack.is_empty() {
                    let curr = chars[j];

                    if escaped {
                        escaped = false;
                    } else if curr == '\\' && in_string {
                        escaped = true;
                    } else if curr == '"' {
                        in_string = !in_string;
                    } else if !in_string {
                        match curr {
                            '{' | '[' => stack.push(curr),
                            '}' => {
                                if stack.last() == Some(&'{') {
                                    stack.pop();
                                }
                            }
                            ']' => {
                                if stack.last() == Some(&'[') {
                                    stack.pop();
                                }
                            }
                            _ => {}
                        }
                    }
                    j += 1;
                }

                if stack.is_empty() {
                    // Potential JSON found
                    let candidate: String = chars[start_index..j].iter().collect();
                    if let Ok(value) = serde_json::from_str::<Value>(&candidate) {
                        let pretty = serde_json::to_string_pretty(&value)
                            .unwrap_or_else(|_| candidate.clone());

                        // Generate a short name/preview
                        let name = match &value {
                            Value::Object(map) => {
                                let keys: Vec<&str> =
                                    map.keys().take(3).map(|k| k.as_str()).collect();
                                let suffix = if map.len() > 3 { ", ..." } else { "" };
                                format!("{{ {} }}{}", keys.join(", "), suffix)
                            }
                            Value::Array(arr) => format!("[Array({})]", arr.len()),
                            _ => "JSON Value".to_string(),
                        };

                        json_blocks.push(JsonBlock {
                            name,
                            pretty,
                            raw: candidate,
                            value,
                        });

                        i = j; // Skip past this JSON
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    json_blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_history() {
        let content = "$ echo hello\nhello\n$ ls\nfile1\nfile2\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 2);
        // Newest (last) command first
        assert_eq!(blocks[0].command, "$ ls");
        assert_eq!(blocks[0].output, "file1\nfile2");
        assert_eq!(blocks[1].command, "$ echo hello");
        assert_eq!(blocks[1].clean_command, "$ echo hello");
        assert_eq!(blocks[1].output, "hello");
    }

    #[test]
    fn test_parse_with_ansi_codes() {
        // Simulated ANSI-colored prompt
        let content = "\x1b[32m$\x1b[0m echo test\ntest output\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        // Original ANSI codes preserved in command
        assert!(blocks[0].command.contains("\x1b[32m"));
        // Clean command has ANSI stripped
        assert_eq!(blocks[0].clean_command, "$ echo test");
        assert_eq!(blocks[0].output, "test output");
    }

    #[test]
    fn test_empty_content() {
        let content = "";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert!(blocks.is_empty());
    }

    #[test]
    fn test_multiline_double_quotes() {
        let content = "$ echo \"hello\nworld\"\nhello\nworld\n$ ls\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 2);
        // Newest first
        assert_eq!(blocks[0].clean_command, "$ ls");
        assert_eq!(blocks[1].clean_command, "$ echo \"hello\nworld\"");
        assert_eq!(blocks[1].output, "hello\nworld");
    }

    #[test]
    fn test_multiline_single_quotes() {
        let content = "$ echo 'hello\nworld'\nhello\nworld\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].clean_command, "$ echo 'hello\nworld'");
        assert_eq!(blocks[0].output, "hello\nworld");
    }

    #[test]
    fn test_trailing_backslash_continuation() {
        let content = "$ echo \\\ncontinued\nthe output\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].clean_command, "$ echo \\\ncontinued");
        assert_eq!(blocks[0].output, "the output");
    }

    #[test]
    fn test_multiline_json_in_single_quotes() {
        let content = "$ curl -d '{\n  \"key\": \"value\"\n}' https://api.com\n{\"ok\":true}\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].clean_command.contains("curl"));
        assert!(blocks[0].clean_command.contains("}'")); // Closing brace and quote
        assert_eq!(blocks[0].output, "{\"ok\":true}");
    }

    #[test]
    fn test_nested_quotes() {
        // Double quotes inside single quotes should not affect state
        let content = "$ echo '\"nested\"'\n\"nested\"\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].clean_command, "$ echo '\"nested\"'");
        assert_eq!(blocks[0].output, "\"nested\"");
    }

    #[test]
    fn test_escaped_quote_in_double_quotes() {
        // Escaped quote inside double quotes
        let content = "$ echo \"he said \\\"hi\\\"\"\nhe said \"hi\"\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].output, "he said \"hi\"");
    }

    #[test]
    fn test_prompt_inside_quotes_not_matched() {
        // A $ inside quotes should not be treated as a new prompt
        let content = "$ echo \"$HOME\n/home/user\"\n/home/user\n$ ls\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 2);
        // Newest first, older command spans multiple lines
        assert_eq!(blocks[0].clean_command, "$ ls");
        assert!(blocks[1].clean_command.contains("$HOME"));
        assert!(blocks[1].clean_command.contains("/home/user\""));
    }

    // === Zsh-style prompt tests ===

    #[test]
    fn test_zsh_percent_prompt() {
        let content = "~/code % ls\nfile1\nfile2\n~/code % pwd\n/home/user/code\n";
        let re = Regex::new(r"^.+% ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 2);
        // Newest first
        assert_eq!(blocks[0].clean_command, "~/code % pwd");
        assert_eq!(blocks[0].output, "/home/user/code");
        assert_eq!(blocks[1].clean_command, "~/code % ls");
        assert_eq!(blocks[1].output, "file1\nfile2");
    }

    #[test]
    fn test_zsh_colored_prompt() {
        // Simulates: \x1b[36m~/c/project \x1b[32m%\x1b[39m command
        let content = "\x1b[36m~/c/project \x1b[32m%\x1b[39m gst\nM  file.rs\n";
        let re = Regex::new(r"^.+% ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].clean_command, "~/c/project % gst");
        assert_eq!(blocks[0].output, "M  file.rs");
        // ANSI codes preserved in raw command
        assert!(blocks[0].command.contains("\x1b[36m"));
    }

    #[test]
    fn test_zsh_prompt_with_git_status_suffix() {
        // Real zsh prompt has git info after command on same line
        // But this appears visually, the actual input is just the command
        let content = "~/c/workmux % gst\nM  tests/conftest.py\n~/c/workmux % ls\n";
        let re = Regex::new(r"^.+% ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 2);
        // Newest first
        assert!(blocks[0].clean_command.contains("ls"));
        assert!(blocks[1].clean_command.contains("gst"));
    }

    // === Trailing operator tests ===

    #[test]
    fn test_trailing_pipe_continuation() {
        let content = "$ cat file.txt |\ngrep error\nerror: something failed\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].clean_command, "$ cat file.txt |\ngrep error");
        assert_eq!(blocks[0].output, "error: something failed");
    }

    #[test]
    fn test_trailing_and_continuation() {
        let content = "$ mkdir foo &&\ncd foo\n$ pwd\n/tmp/foo\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 2);
        // Newest first
        assert_eq!(blocks[0].clean_command, "$ pwd");
        assert_eq!(blocks[0].output, "/tmp/foo");
        assert_eq!(blocks[1].clean_command, "$ mkdir foo &&\ncd foo");
        assert_eq!(blocks[1].output, "");
    }

    #[test]
    fn test_trailing_or_continuation() {
        let content = "$ false ||\necho fallback\nfallback\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].clean_command, "$ false ||\necho fallback");
        assert_eq!(blocks[0].output, "fallback");
    }

    #[test]
    fn test_pipe_inside_quotes_not_continuation() {
        // Pipe inside quotes should not trigger continuation
        let content = "$ echo \"hello | world\"\nhello | world\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].clean_command, "$ echo \"hello | world\"");
        assert_eq!(blocks[0].output, "hello | world");
    }

    // === Edge cases ===

    #[test]
    fn test_command_with_no_output() {
        let content = "$ true\n$ false\n$ echo hi\nhi\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 3);
        // Newest first
        assert_eq!(blocks[0].clean_command, "$ echo hi");
        assert_eq!(blocks[0].output, "hi");
        assert_eq!(blocks[1].clean_command, "$ false");
        assert_eq!(blocks[1].output, "");
        assert_eq!(blocks[2].clean_command, "$ true");
        assert_eq!(blocks[2].output, "");
    }

    #[test]
    fn test_output_looks_like_prompt() {
        // Output contains text that looks like a prompt - parser WILL treat it
        // as a new command. This is a fundamental limitation of regex-based parsing.
        // The parser cannot distinguish prompt-like output from real prompts.
        let content = "$ cat script.sh\n$ echo hello\necho goodbye\n$ ls\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        // Each line matching prompt pattern becomes a new block (newest first)
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].clean_command, "$ ls");
        assert_eq!(blocks[0].output, "");
        assert_eq!(blocks[1].clean_command, "$ echo hello");
        assert_eq!(blocks[1].output, "echo goodbye");
        assert_eq!(blocks[2].clean_command, "$ cat script.sh");
        assert_eq!(blocks[2].output, ""); // Empty because next line looks like prompt
    }

    #[test]
    fn test_multiple_backslash_continuations() {
        let content = "$ echo \\\none \\\ntwo \\\nthree\none two three\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].clean_command, "$ echo \\\none \\\ntwo \\\nthree");
        assert_eq!(blocks[0].output, "one two three");
    }

    #[test]
    fn test_complex_pipeline_multiline() {
        let content = "$ cat data.json |\njq '.items[]' |\ngrep -v null\nitem1\nitem2\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].clean_command.contains("jq"));
        assert!(blocks[0].clean_command.contains("grep"));
        assert_eq!(blocks[0].output, "item1\nitem2");
    }

    #[test]
    fn test_real_world_zsh_ansi() {
        // Real captured output from tmux with ANSI codes
        let content = "\x1b[36m~/c/workmux \x1b[32m%\x1b[39m gst\n\x1b[32mM\x1b[39m  tests/conftest.py\n\x1b[36m~/c/workmux \x1b[32m%\x1b[39m ls\nCargo.toml\nsrc\n";
        let re = Regex::new(r"^.+% ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 2);
        // Newest first
        assert_eq!(blocks[0].clean_command, "~/c/workmux % ls");
        assert_eq!(blocks[0].output, "Cargo.toml\nsrc");
        assert_eq!(blocks[1].clean_command, "~/c/workmux % gst");
        // Output still has ANSI codes
        assert!(blocks[1].output.contains("\x1b[32mM"));
    }

    // === Default regex tests ===
    // These test the actual DEFAULT_PROMPT_REGEX constant

    #[test]
    fn test_default_regex_zsh_percent() {
        let content = "~/code % echo hello\nhello\n~/code % ls\nfile\n";
        let re = Regex::new(super::DEFAULT_PROMPT_REGEX).unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn test_default_regex_with_ansi_colors() {
        // Colored zsh prompt like the user has
        let content = "\x1b[36m~/c/project \x1b[32m%\x1b[39m gst\nM file.rs\n";
        let re = Regex::new(super::DEFAULT_PROMPT_REGEX).unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].clean_command, "~/c/project % gst");
    }

    #[test]
    fn test_default_regex_ignores_code_snippets() {
        // Code with $ or # inside quotes should NOT be matched as prompts
        let content = "~/code % cat test.rs\nlet x = \"$ echo hello\";\nlet y = '# comment';\n~/code % ls\nfile.rs\n";
        let re = Regex::new(super::DEFAULT_PROMPT_REGEX).unwrap();
        let blocks = parse_history(content, &re);

        // Only the two actual prompts should match, not the code lines
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].clean_command.contains("ls"));
        assert!(blocks[1].clean_command.contains("cat test.rs"));
        // The code lines should be in the output of the cat command
        assert!(blocks[1].output.contains("$ echo hello"));
    }

    #[test]
    fn test_default_regex_ignores_indented_output() {
        // Cargo build output with progress bars containing % should NOT match as prompts
        // This was a real bug: "100% " in indented cargo output matched as zsh prompt
        let content = "~/code % cargo build\n   Compiling foo v0.1.0\n    Finished `dev` profile target(s)\n~/code % cargo run\n   Compiling foo v0.1.0\n       100% building\n    Finished `dev` profile target(s)\n     Running `target/debug/foo`\nhello world\n";
        let re = Regex::new(super::DEFAULT_PROMPT_REGEX).unwrap();
        let blocks = parse_history(content, &re);

        // Only the two actual prompts should match
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].clean_command.contains("cargo run"));
        assert!(blocks[1].clean_command.contains("cargo build"));
        // The indented "100%" line should be in the output, not a separate block
        assert!(blocks[0].output.contains("100%"));
    }

    #[test]
    fn test_default_regex_ignores_empty_prompts() {
        // Empty prompts (just prompt char + whitespace) should NOT match
        // This happens with tmux scrollback when prompt is redrawn or user presses Enter
        let content = "~/code %                                                        \n~/code %                                                        \n~/code % echo foo                                               \nfoo\n~/code %                                                        \n~/code % ls\nfile.txt\n";
        let re = Regex::new(super::DEFAULT_PROMPT_REGEX).unwrap();
        let blocks = parse_history(content, &re);

        // Only prompts with actual commands should match
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].clean_command.contains("ls"));
        assert!(blocks[1].clean_command.contains("echo foo"));
    }

    #[test]
    fn test_command_text_strips_right_prompt() {
        // Real zsh prompt with right-aligned git status separated by many spaces
        // The command_text field should contain only "history", not the git branch
        let content = "~/c/tmux-copy-tool % history                                                                                                                main\n  123  ls\n  124  cd foo\n";
        let re = Regex::new(super::DEFAULT_PROMPT_REGEX).unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        // clean_command contains the full line (for display)
        assert!(blocks[0].clean_command.contains("main"));
        // command_text should have right-prompt stripped (for copying)
        assert_eq!(blocks[0].command_text, "history");
    }

    #[test]
    fn test_command_text_without_right_prompt() {
        // Command without right-aligned prompt should work normally
        let content = "~/code % echo hello\nworld\n";
        let re = Regex::new(super::DEFAULT_PROMPT_REGEX).unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].command_text, "echo hello");
    }

    #[test]
    fn test_command_text_multiline() {
        // Multiline command should preserve continuation lines
        let content = "$ echo \\\ncontinued\nthe output\n";
        let re = Regex::new(r"^\$ ").unwrap();
        let blocks = parse_history(content, &re);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].command_text, "echo \\\ncontinued");
    }
}
