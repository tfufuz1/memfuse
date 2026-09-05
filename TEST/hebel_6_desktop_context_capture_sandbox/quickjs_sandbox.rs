use rquickjs::{Context, Function, Runtime};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// Maximum execution time for user scripts (§ 22 SETTINGS_SCHEMA default).
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(3);

/// Maximum input size (2 MB as per spec).
const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;

/// Maximum output size (512 KB as per spec).
const MAX_OUTPUT_BYTES: usize = 512 * 1024;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScriptExecutionResultDto {
    pub output: String,
    pub execution_time_ms: u32,
    pub console_logs: Vec<String>,
    pub error: Option<String>,
}

/// Public async entry point: runs the script with a timeout guard.
///
/// If the script exceeds `SCRIPT_TIMEOUT`, the blocking thread is abandoned
/// and a timeout error is returned. The QuickJS runtime will be dropped
/// when the abandoned thread eventually finishes or is cleaned up.
pub async fn run_script_in_sandbox(
    js_code: String,
    input_text: String,
    params_json: Option<String>,
) -> ScriptExecutionResultDto {
    let start = std::time::Instant::now();

    if input_text.len() > MAX_INPUT_BYTES {
        return ScriptExecutionResultDto {
            output: input_text.clone(),
            execution_time_ms: 0,
            console_logs: vec![],
            error: Some(format!(
                "Input size ({} bytes) exceeds maximum limit ({} bytes)",
                input_text.len(),
                MAX_INPUT_BYTES
            )),
        };
    }

    let input_text_fallback = input_text.clone();

    let result = tokio::time::timeout(
        SCRIPT_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            run_script_sync(&js_code, &input_text, params_json.as_deref())
        }),
    )
    .await;

    match result {
        // Completed within timeout
        Ok(Ok(dto)) => dto,
        // spawn_blocking panicked
        Ok(Err(join_err)) => ScriptExecutionResultDto {
            output: input_text_fallback,
            execution_time_ms: start.elapsed().as_millis() as u32,
            console_logs: vec![],
            error: Some(format!("Script thread panicked: {}", join_err)),
        },
        // Timeout elapsed
        Err(_) => ScriptExecutionResultDto {
            output: String::new(),
            execution_time_ms: start.elapsed().as_millis() as u32,
            console_logs: vec![],
            error: Some(format!(
                "Script execution timed out ({}s limit)",
                SCRIPT_TIMEOUT.as_secs()
            )),
        },
    }
}

/// Synchronous script execution inside QuickJS with console.log capture.
fn run_script_sync(
    js_code: &str,
    input_text: &str,
    params_json: Option<&str>,
) -> ScriptExecutionResultDto {
    let start = std::time::Instant::now();

    let runtime = match Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            return ScriptExecutionResultDto {
                output: input_text.to_string(),
                execution_time_ms: start.elapsed().as_millis() as u32,
                console_logs: vec![],
                error: Some(format!("Failed to create JS runtime: {}", e)),
            }
        }
    };

    let context = match Context::full(&runtime) {
        Ok(c) => c,
        Err(e) => {
            return ScriptExecutionResultDto {
                output: input_text.to_string(),
                execution_time_ms: start.elapsed().as_millis() as u32,
                console_logs: vec![],
                error: Some(format!("Failed to create JS context: {}", e)),
            }
        }
    };

    let result = context.with(|ctx| -> (Result<String, String>, Vec<String>) {
        let globals = ctx.globals();

        // --- Set up console.log/warn/error capture ---
        let console_logs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

        let console_obj = match rquickjs::Object::new(ctx.clone()) {
            Ok(obj) => obj,
            Err(e) => return (Err(format!("Failed to create console object: {}", e)), vec![]),
        };

        // Helper macro to avoid repetition for log/warn/error
        macro_rules! register_console_fn {
            ($name:expr, $logs:expr) => {{
                let logs_ref = $logs.clone();
                let func = Function::new(ctx.clone(), move |args: rquickjs::function::Rest<rquickjs::Value>| {
                    let parts: Vec<String> = args.0.iter().map(|v| {
                        if let Some(s) = v.as_string() {
                            s.to_string().unwrap_or_default()
                        } else {
                            format!("{:?}", v)
                        }
                    }).collect();
                    logs_ref.borrow_mut().push(parts.join(" "));
                });
                match func {
                    Ok(f) => { let _ = console_obj.set($name, f); },
                    Err(e) => return (Err(format!("Failed to create console.{}: {}", $name, e)), vec![]),
                }
            }};
        }

        register_console_fn!("log", console_logs);
        register_console_fn!("warn", console_logs);
        register_console_fn!("error", console_logs);
        register_console_fn!("info", console_logs);

        if let Err(e) = globals.set("console", console_obj) {
            return (Err(format!("Failed to set console global: {}", e)), vec![]);
        }

        // --- Set input variables ---
        if let Err(e) = globals.set("inputText", input_text) {
            return (Err(e.to_string()), vec![]);
        }
        if let Err(e) = globals.set("sandboxParamsJson", params_json.unwrap_or("{}")) {
            return (Err(e.to_string()), vec![]);
        }

        let prelude = r#"
            const utils = {
                lines: (s) => String(s || '').split(/\r?\n/),
                unlines: (arr) => Array.isArray(arr) ? arr.join('\n') : String(arr || ''),
                words: (s) => String(s || '').trim().split(/\s+/).filter(Boolean),
                unwords: (arr) => Array.isArray(arr) ? arr.join(' ') : String(arr || ''),
                trim: (s) => String(s || '').trim(),
                trimLines: (s) => utils.unlines(utils.lines(s).map(l => l.trim())),
                truncate: (s, n) => {
                    const str = String(s || '');
                    return str.length <= n ? str : str.slice(0, n) + '…';
                },
                wrap: (s, n) => {
                    const words = String(s || '').split(' ');
                    const result = [];
                    let line = '';
                    for (const w of words) {
                        if ((line + w).length > n) { result.push(line.trimEnd()); line = ''; }
                        line += w + ' ';
                    }
                    if (line.trim()) result.push(line.trimEnd());
                    return result.join('\n');
                },
                padLeft: (s, n, c) => String(s || '').padStart(n, c || ' '),
                padRight: (s, n, c) => String(s || '').padEnd(n, c || ' '),
                repeat: (s, n) => String(s || '').repeat(n),
                reverse: (s) => [...String(s || '')].reverse().join(''),
                capitalize: (s) => {
                    const str = String(s || '');
                    return str.charAt(0).toUpperCase() + str.slice(1).toLowerCase();
                },
                titleCase: (s) => String(s || '').replace(/\b\w/g, c => c.toUpperCase()),
                slugify: (s) => String(s || '').toLowerCase().replace(/[^\w\s-]/g, '').replace(/[\s_-]+/g, '-').replace(/^-+|-+$/g, ''),
                camelToSnake: (s) => String(s || '').replace(/[A-Z]/g, c => '_' + c.toLowerCase()),
                snakeToCamel: (s) => String(s || '').replace(/_([a-z])/g, (_, c) => c.toUpperCase()),
                count: (s, sub) => (String(s || '').match(new RegExp(String(sub || '').replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g')) || []).length,

                sortLines: (s) => utils.unlines(utils.lines(s).sort()),
                uniqueLines: (s) => utils.unlines([...new Set(utils.lines(s))]),
                reverseLines: (s) => utils.unlines(utils.lines(s).reverse()),
                filterLines: (s, pred) => utils.unlines(utils.lines(s).filter(pred)),
                mapLines: (s, fn) => utils.unlines(utils.lines(s).map(fn)),
                numberedLines: (s) => utils.unlines(utils.lines(s).map((l, i) => `${i + 1}. ${l}`)),
                prefixLines: (s, pfx) => utils.mapLines(s, l => pfx + l),
                suffixLines: (s, sfx) => utils.mapLines(s, l => l + sfx),
                indentLines: (s, n) => utils.prefixLines(s, ' '.repeat(n)),

                prettyJSON: (s) => JSON.stringify(JSON.parse(s), null, 2),
                minifyJSON: (s) => JSON.stringify(JSON.parse(s)),
                parseJSON: (s) => JSON.parse(s),
                stringifyJSON: (v, ind) => JSON.stringify(v, null, ind ?? 2),
                jsonKeys: (s) => Object.keys(JSON.parse(s)).join('\n'),

                wrapMarkdown: (s, lang) => '```' + (lang || '') + '\n' + String(s || '') + '\n```',
                stripMarkdown: (s) => String(s || '').replace(/#{1,6}\s|(\*\*|__)(.*?)\1|\*|_|\[([^\]]*)\]\([^)]*\)|`[^`]*`|```[\s\S]*?```/g, '$3'),
                extractCodeBlocks: (s) => {
                    const blocks = [...String(s || '').matchAll(/```(?:\w+)?\n([\s\S]*?)```/g)].map(m => m[1]);
                    return blocks.join('\n---\n');
                },

                redact: (s) => String(s || '')
                    .replace(/\b(?:\d{1,3}\.){3}\d{1,3}\b/g, '[IP]')
                    .replace(/[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/g, '[EMAIL]')
                    .replace(/\b(sk|pk|api)[-_][\w\-]{8,}/gi, '[API_KEY]')
                    .replace(/Bearer\s+[\w\-._~+/]+=*/gi, 'Bearer [TOKEN]')
                    .replace(/\b(?:\d[ \-]?){13,16}\b/g, '[CARD]'),

                base64Encode: (s) => btoa(String(s || '')),
                base64Decode: (s) => atob(String(s || '')),
                urlEncode: (s) => encodeURIComponent(String(s || '')),
                urlDecode: (s) => decodeURIComponent(String(s || '')),
                htmlEscape: (s) => String(s || '').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;'),
                htmlUnescape: (s) => String(s || '').replace(/&amp;/g,'&').replace(/&lt;/g,'<').replace(/&gt;/g,'>').replace(/&quot;/g,'"').replace(/&#39;/g,"'"),

                charCount: (s) => String(s || '').length,
                wordCount: (s) => String(s || '').trim() === '' ? 0 : String(s || '').trim().split(/\s+/).length,
                lineCount: (s) => String(s || '').split('\n').length,
                tokenEstimate: (s) => Math.round(Math.max(String(s || '').length / 4, utils.wordCount(s) * 0.75)),

                today: () => new Date().toISOString().split('T')[0],
                timestamp: () => new Date().toISOString(),
                unixMs: () => Date.now(),
            };
        "#;

        let wrapped_code = format!(
            r#"
            (function() {{
                {prelude}
                const input = inputText;
                let params = {{}};
                try {{
                    params = JSON.parse(sandboxParamsJson);
                }} catch(e) {{}}
                let transform = function(input, params, utils) {{
                    {js_code}
                }};
                let res = transform(input, params, utils);
                if (res === undefined || res === null) {{
                    return "";
                }}
                if (typeof res === "string") {{
                    return res;
                }}
                if (typeof res === "object") {{
                    return JSON.stringify(res, null, 2);
                }}
                return String(res);
            }})()
            "#
        );

        let eval_res: Result<String, _> = ctx.eval(wrapped_code);
        let logs = console_logs.borrow().clone();
        match eval_res {
            Ok(mut res) => {
                // Enforce output size limit safely at UTF-8 char boundary
                if res.len() > MAX_OUTPUT_BYTES {
                    let mut end = MAX_OUTPUT_BYTES;
                    while end > 0 && !res.is_char_boundary(end) {
                        end -= 1;
                    }
                    res.truncate(end);
                }
                (Ok(res), logs)
            }
            Err(e) => (Err(e.to_string()), logs),
        }
    });

    let elapsed = start.elapsed().as_millis() as u32;

    match result {
        (Ok(output), logs) => ScriptExecutionResultDto {
            output,
            execution_time_ms: elapsed,
            console_logs: logs,
            error: None,
        },
        (Err(err), logs) => ScriptExecutionResultDto {
            output: input_text.to_string(),
            execution_time_ms: elapsed,
            console_logs: logs,
            error: Some(err),
        },
    }
}
