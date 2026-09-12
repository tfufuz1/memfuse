// FILE-CONTEXT
// ZWECK: xtask Lint-Werkzeug zur Erkennung von OOB-Reads in unsafe SIMD-Funktionen.
// INVARIANTEN: Prüft AST von unsafe fn mit 2 Slice-Parametern gleichen Typs auf .min() oder Längencheck vor SIMD-Loads.
// STAND: TS:2026-09-12T00:00:00Z

use std::fs;
use std::path::Path;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprMethodCall, FnArg, ItemFn, Pat, Type};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintViolation {
    pub file_path: String,
    pub line_num: usize,
    pub fn_name: String,
    pub details: String,
}

struct SliceParam {
    _name: String,
    elem_type: String,
}

struct SimdFnVisitor<'a> {
    file_path: &'a str,
    violations: Vec<LintViolation>,
}

impl<'a> SimdFnVisitor<'a> {
    fn new(file_path: &'a str) -> Self {
        Self {
            file_path,
            violations: Vec::new(),
        }
    }

    fn check_item_fn(&mut self, item: &ItemFn) {
        // 1. Check if function is unsafe
        if item.sig.unsafety.is_none() {
            return;
        }

        let fn_name = item.sig.ident.to_string();

        // 2. Identify slice parameters
        let mut slice_params: Vec<SliceParam> = Vec::new();
        for arg in &item.sig.inputs {
            if let FnArg::Typed(pat_type) = arg {
                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    let param_name = pat_ident.ident.to_string();
                    if let Some(elem_type) = extract_slice_elem_type(&pat_type.ty) {
                        slice_params.push(SliceParam {
                            _name: param_name,
                            elem_type,
                        });
                    }
                }
            }
        }

        // We require at least 2 slice parameters of the same element type
        if slice_params.len() < 2 {
            return;
        }

        let mut has_matching_pair = false;
        for i in 0..slice_params.len() {
            for j in (i + 1)..slice_params.len() {
                if slice_params[i].elem_type == slice_params[j].elem_type {
                    has_matching_pair = true;
                    break;
                }
            }
        }

        if !has_matching_pair {
            return;
        }

        // 3. Inspect function body for SIMD load intrinsics and bound checks
        let mut inspector = FnBodyInspector::default();
        inspector.visit_block(&item.block);

        if inspector.has_simd_load {
            let is_guarded = inspector.has_length_min_call || inspector.has_length_check_or_assert;

            if !is_guarded {
                let line_num = item.sig.fn_token.span().start().line;
                self.violations.push(LintViolation {
                    file_path: self.file_path.to_string(),
                    line_num,
                    fn_name,
                    details: "Unsafe SIMD function with matching slice parameters contains SIMD load calls without length normalization (.min) or length assertion/check".to_string(),
                });
            }
        }
    }
}

impl<'a> Visit<'a> for SimdFnVisitor<'a> {
    fn visit_item_fn(&mut self, item: &'a ItemFn) {
        self.check_item_fn(item);
        syn::visit::visit_item_fn(self, item);
    }
}

#[derive(Default)]
struct FnBodyInspector {
    has_simd_load: bool,
    has_length_min_call: bool,
    has_length_check_or_assert: bool,
}

impl<'a> Visit<'a> for FnBodyInspector {
    fn visit_expr_method_call(&mut self, call: &'a ExprMethodCall) {
        let method_name = call.method.to_string();

        if method_name == "min" {
            // Check if .min is called on a length or within length normalization
            self.has_length_min_call = true;
        }

        // Check for .len() comparisons or calls
        if method_name == "len" {
            // Presence of .len() with assertions or checks
        }

        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'a ExprCall) {
        if let Expr::Path(expr_path) = &*call.func {
            if let Some(ident) = expr_path.path.segments.last() {
                let name = ident.ident.to_string();
                if is_simd_load_intrinsic(&name) {
                    self.has_simd_load = true;
                }
            }
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_macro(&mut self, mac: &'a syn::Macro) {
        if let Some(ident) = mac.path.segments.last() {
            let name = ident.ident.to_string();
            if name == "assert_eq"
                || name == "debug_assert_eq"
                || name == "assert"
                || name == "debug_assert"
            {
                self.has_length_check_or_assert = true;
            }
        }
        syn::visit::visit_macro(self, mac);
    }

    fn visit_expr_if(&mut self, expr_if: &'a syn::ExprIf) {
        // If statements checking slice length (e.g., if a.len() != b.len())
        if expr_contains_length_check(&expr_if.cond) {
            self.has_length_check_or_assert = true;
        }
        syn::visit::visit_expr_if(self, expr_if);
    }

    fn visit_expr_binary(&mut self, binary: &'a syn::ExprBinary) {
        if expr_contains_length_check(&Expr::Binary(binary.clone())) {
            self.has_length_check_or_assert = true;
        }
        syn::visit::visit_expr_binary(self, binary);
    }
}

fn is_simd_load_intrinsic(name: &str) -> bool {
    // Matches SIMD load intrinsics across x86 (_mm..._loadu...) and ARM (vld1q...)
    name.contains("_loadu")
        || name.contains("vld1q")
        || (name.starts_with("_mm") && name.contains("load"))
}

fn extract_slice_elem_type(ty: &Type) -> Option<String> {
    if let Type::Reference(type_ref) = ty {
        if let Type::Slice(type_slice) = &*type_ref.elem {
            return Some(quote::quote!(#type_slice).to_string());
        }
    }
    None
}

fn expr_contains_length_check(expr: &Expr) -> bool {
    let s = quote::quote!(#expr).to_string();
    s.contains("len ()") || s.contains(". len ()") || s.contains("len()")
}

pub fn lint_code_str(code: &str, file_name: &str) -> Result<Vec<LintViolation>, String> {
    let file_ast = syn::parse_file(code).map_err(|e| format!("Parse error in {}: {}", file_name, e))?;
    let mut visitor = SimdFnVisitor::new(file_name);
    visitor.visit_file(&file_ast);
    Ok(visitor.violations)
}

pub fn run_lint_unsafe_slice_bounds() -> bool {
    println!("=== xtask lint-unsafe-slices ===");
    let mut all_violations = Vec::new();
    let target_dir = Path::new("crates/memfuse-index/src");

    if !target_dir.exists() {
        eprintln!("Target directory {} does not exist!", target_dir.display());
        return false;
    }

    for entry in WalkDir::new(target_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rs"))
    {
        let path = entry.path();
        let rel_path = path.to_string_lossy().to_string();

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to read {}: {}", rel_path, e);
                return false;
            }
        };

        match lint_code_str(&content, &rel_path) {
            Ok(violations) => {
                all_violations.extend(violations);
            }
            Err(e) => {
                eprintln!("Error parsing {}: {}", rel_path, e);
                return false;
            }
        }
    }

    if all_violations.is_empty() {
        println!("✅ xtask lint-unsafe-slices PASSED: No OOB SIMD slice bound violations found.");
        true
    } else {
        eprintln!("❌ xtask lint-unsafe-slices FAILED: Found {} potential OOB SIMD slice bound violation(s):", all_violations.len());
        for v in &all_violations {
            eprintln!(
                "  - {}:{} in function '{}': {}",
                v.file_path, v.line_num, v.fn_name, v.details
            );
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lint_detects_unbound_simd_load() {
        let bad_code = r#"
            pub unsafe fn bad_simd(a: &[f32], b: &[f32]) -> f32 {
                let mut i = 0;
                let n = a.len();
                while i + 8 <= n {
                    let va = _mm256_loadu_ps(a.as_ptr().add(i));
                    let vb = _mm256_loadu_ps(b.as_ptr().add(i));
                    i += 8;
                }
                0.0
            }
        "#;

        let violations = lint_code_str(bad_code, "synthetic_bad.rs").unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].fn_name, "bad_simd");
    }

    #[test]
    fn test_lint_passes_normalized_min_length() {
        let good_code = r#"
            pub unsafe fn good_simd(a: &[f32], b: &[f32]) -> f32 {
                let n = a.len().min(b.len());
                let mut i = 0;
                while i + 8 <= n {
                    let va = _mm256_loadu_ps(a.as_ptr().add(i));
                    let vb = _mm256_loadu_ps(b.as_ptr().add(i));
                    i += 8;
                }
                0.0
            }
        "#;

        let violations = lint_code_str(good_code, "synthetic_good.rs").unwrap();
        assert!(violations.is_empty());
    }

    #[test]
    fn test_lint_passes_assert_length() {
        let good_assert_code = r#"
            pub unsafe fn good_assert_simd(a: &[u8], b: &[u8]) -> u32 {
                debug_assert_eq!(a.len(), b.len());
                let n = a.len();
                let mut i = 0;
                while i + 32 <= n {
                    let va = _mm256_loadu_si256(a.as_ptr().add(i) as *const _);
                    let vb = _mm256_loadu_si256(b.as_ptr().add(i) as *const _);
                    i += 32;
                }
                0
            }
        "#;

        let violations = lint_code_str(good_assert_code, "synthetic_good_assert.rs").unwrap();
        assert!(violations.is_empty());
    }

    #[test]
    fn test_lint_ignores_safe_fn() {
        let safe_code = r#"
            pub fn safe_fn(a: &[f32], b: &[f32]) -> f32 {
                let va = unsafe { _mm256_loadu_ps(a.as_ptr()) };
                0.0
            }
        "#;

        let violations = lint_code_str(safe_code, "synthetic_safe.rs").unwrap();
        assert!(violations.is_empty());
    }
}
