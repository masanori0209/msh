use std::process::Command;

fn run_c(command: &str) -> (String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_msh"))
        .env("MSH_SKIP_RC", "1")
        .arg("-c")
        .arg(command)
        .output()
        .expect("failed to spawn msh -c");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (stdout, output.status.code().unwrap_or(1))
}

#[test]
fn chain_and_success() {
    let (stdout, code) = run_c("true && echo ok");
    assert_eq!(code, 0);
    assert!(stdout.contains("ok"));
}

#[test]
fn chain_and_failure() {
    let (stdout, code) = run_c("false && echo skip");
    assert_ne!(code, 0);
    assert!(!stdout.contains("skip"));
}

#[test]
fn chain_or_fallback() {
    let (stdout, code) = run_c("false || echo fallback");
    assert_eq!(code, 0);
    assert!(stdout.contains("fallback"));
}

#[test]
fn chain_semicolon() {
    let (stdout, code) = run_c("echo a; echo b");
    assert_eq!(code, 0);
    assert!(stdout.contains("a"));
    assert!(stdout.contains("b"));
}

#[test]
fn last_status_expansion() {
    let (stdout, code) = run_c("false; echo $?");
    assert_eq!(code, 0);
    assert!(stdout.trim().ends_with('1'));
}

#[test]
fn command_substitution() {
    let (stdout, code) = run_c("echo $(echo hi)");
    assert_eq!(code, 0);
    assert!(stdout.contains("hi"));
}

#[test]
fn command_substitution_trims_trailing_newline() {
    // bash 同様、$(...) の末尾改行は除去される。
    let (stdout, code) = run_c("echo \"[$(echo hi)]\"");
    assert_eq!(code, 0);
    assert_eq!(stdout, "[hi]\n");
}

#[test]
fn command_not_found_exit_code_is_127() {
    // bash 互換: 不在コマンドは終了コード 127。
    let (_stdout, code) = run_c("definitely_no_such_cmd_xyz");
    assert_eq!(code, 127);
}

#[test]
fn command_not_found_does_not_abort_line() {
    // 不在コマンドは致命的エラーにせず、後続コマンドを続行する（bash 互換）。
    let (stdout, code) = run_c("definitely_no_such_cmd_xyz; echo after");
    assert!(stdout.contains("after"), "got: {stdout}");
    assert_eq!(code, 0);
}

#[test]
fn command_not_found_or_fallback_runs() {
    let (stdout, code) = run_c("definitely_no_such_cmd_xyz || echo fallback");
    assert!(stdout.contains("fallback"), "got: {stdout}");
    assert_eq!(code, 0);
}

#[test]
fn exit_without_arg_uses_last_status() {
    // bash 互換: 引数なし exit は直前コマンドの終了ステータスを返す。
    let (_stdout, code) = run_c("false; exit");
    assert_eq!(code, 1);
}

#[test]
fn process_substitution_input() {
    let (stdout, code) = run_c("cat <(echo proc)");
    assert_eq!(code, 0);
    assert!(stdout.contains("proc"), "got: {stdout}");
}

#[test]
fn param_assign_default_persists() {
    let (stdout, code) = run_c(": ${z:=persist}; echo $z");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "persist");
}

#[test]
fn export_with_expansion() {
    let (stdout, code) = run_c("export MSH_COMPAT_TEST=hello; echo $MSH_COMPAT_TEST");
    assert_eq!(code, 0);
    assert!(stdout.contains("hello"));
}

#[test]
fn inline_for_loop() {
    let (stdout, code) = run_c("for f in a b; do echo $f; done");
    assert_eq!(code, 0);
    assert!(stdout.contains("a"));
    assert!(stdout.contains("b"));
}

#[test]
fn function_definition_and_call() {
    let (stdout, code) = run_c("greet() { echo hi; }; greet");
    assert_eq!(code, 0);
    assert!(stdout.contains("hi"));
}

#[test]
fn inline_if_then() {
    let (stdout, code) = run_c("if true; then echo yes; fi");
    assert_eq!(code, 0);
    assert!(stdout.contains("yes"));
}

#[test]
fn pipeline_with_and() {
    let (stdout, code) = run_c("echo hello | wc -c && echo done");
    assert_eq!(code, 0);
    assert!(stdout.contains("done"));
}

#[test]
fn array_assignment_and_index() {
    let (stdout, code) = run_c("arr=(a b c); echo ${arr[1]}");
    assert_eq!(code, 0);
    assert!(stdout.contains("b"));
}

#[test]
fn array_expand_all() {
    let (stdout, code) = run_c("arr=(x y); echo ${arr[@]}");
    assert_eq!(code, 0);
    assert!(stdout.contains("x"));
    assert!(stdout.contains("y"));
}

#[test]
fn single_quote_literal() {
    let (stdout, code) = run_c("echo '$MSH_SKIP_RC'");
    assert_eq!(code, 0);
    assert!(stdout.contains("$MSH_SKIP_RC"));
    assert!(!stdout.contains("1"));
}

#[test]
fn heredoc_inline() {
    let (stdout, code) = run_c("cat <<EOF\nhello heredoc\nEOF");
    assert_eq!(code, 0);
    assert!(stdout.contains("hello heredoc"));
}

#[test]
fn pushd_popd() {
    let (stdout, code) = run_c("pushd /tmp >/dev/null; popd >/dev/null; pwd");
    assert_eq!(code, 0);
    assert!(!stdout.trim().is_empty());
}

#[test]
fn double_bracket_string_equality() {
    let (stdout, code) = run_c("[[ abc == abc ]] && echo eq");
    assert_eq!(code, 0);
    assert!(stdout.contains("eq"));
}

#[test]
fn double_bracket_empty_string() {
    let (stdout, code) = run_c("[[ -n \"\" ]] && echo bad || echo good");
    assert_eq!(code, 0);
    assert!(stdout.contains("good"));
    assert!(!stdout.contains("bad"));
}

#[test]
fn double_bracket_numeric() {
    let (stdout, code) = run_c("[[ 3 -lt 5 ]] && echo lt");
    assert_eq!(code, 0);
    assert!(stdout.contains("lt"));
}

#[test]
fn plain_assignment() {
    let (stdout, code) = run_c("X=hello; echo $X");
    assert_eq!(code, 0);
    assert!(stdout.contains("hello"));
}

#[test]
fn empty_assignment() {
    let (stdout, code) = run_c("X=; echo \"[$X]\"");
    assert_eq!(code, 0);
    assert!(stdout.contains("[]"));
}

#[test]
fn and_or_chain_fallback() {
    let (stdout, code) = run_c("false && echo a || echo b");
    assert_eq!(code, 0);
    assert!(stdout.contains("b"));
    assert!(!stdout.contains("a"));
}

#[test]
fn set_errexit_aborts() {
    let (stdout, _) = run_c("set -e; false; echo should_not_run");
    assert!(!stdout.contains("should_not_run"));
}

#[test]
fn set_nounset_errors() {
    let (_, code) = run_c("set -u; echo $UNDEFINED_VAR_ABC");
    assert_ne!(code, 0);
}

#[test]
fn pipestatus_first() {
    let (stdout, code) = run_c("true | false; echo ${PIPESTATUS[0]}");
    assert_eq!(code, 0);
    assert!(stdout.contains('0'));
}

#[test]
fn arithmetic_expansion() {
    let (stdout, code) = run_c("echo $((1 + 2 * 3))");
    assert_eq!(code, 0);
    assert!(stdout.contains('7'));
}

#[test]
fn arithmetic_with_variable() {
    let (stdout, code) = run_c("i=5; echo $((i + 1))");
    assert_eq!(code, 0);
    assert!(stdout.contains('6'));
}

#[test]
fn assoc_array_declare_and_get() {
    let (stdout, code) =
        run_c("declare -A m; m[name]=msh; m[lang]=rust; echo ${m[name]}-${m[lang]}");
    assert_eq!(code, 0);
    assert!(stdout.contains("msh-rust"), "stdout: {stdout}");
}

#[test]
fn assoc_array_keys_and_count() {
    let (stdout, code) = run_c("declare -A m; m[a]=1; m[b]=2; echo ${!m[@]}/${#m[@]}");
    assert_eq!(code, 0);
    // BTreeMap でキー順が安定。
    assert!(stdout.contains("a b/2"), "stdout: {stdout}");
}

#[test]
fn assoc_array_subscript_expansion() {
    let (stdout, code) = run_c("declare -A m; k=foo; m[$k]=bar; echo ${m[$k]}");
    assert_eq!(code, 0);
    assert!(stdout.contains("bar"), "stdout: {stdout}");
}

#[test]
fn assoc_array_missing_key_is_empty() {
    let (stdout, code) = run_c("declare -A m; m[x]=1; echo [${m[y]}]");
    assert_eq!(code, 0);
    assert!(stdout.contains("[]"), "stdout: {stdout}");
}

#[test]
fn indexed_array_element_assignment() {
    let (stdout, code) = run_c("arr[0]=x; arr[2]=z; echo ${arr[2]}/${#arr[@]}");
    assert_eq!(code, 0);
    assert!(stdout.contains("z/3"), "stdout: {stdout}");
}

#[test]
fn inline_while_with_leading_assignment() {
    let (stdout, code) = run_c("i=0; while [ $i -lt 1 ]; do echo w; i=1; done");
    assert_eq!(code, 0);
    assert!(stdout.contains('w'), "stdout: {stdout}");
}

#[test]
fn while_break_exits_loop() {
    let (stdout, code) = run_c("while true; do echo w; break; done");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "w");
}

#[test]
fn param_expansion_default() {
    let (stdout, code) = run_c("echo ${UNSET_XYZ:-fallback}");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "fallback");
}

#[test]
fn param_expansion_suffix_removal() {
    let (stdout, code) = run_c("f=archive.tar.gz; echo ${f%.gz} ${f%%.*}");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "archive.tar archive");
}

#[test]
fn param_expansion_replace_all() {
    let (stdout, code) = run_c("p=a:b:c; echo ${p//:/-}");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "a-b-c");
}

#[test]
fn param_expansion_case_and_substring() {
    let (stdout, code) = run_c("s=hello; echo ${s^^} ${s:1:3}");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "HELLO ell");
}

#[test]
fn param_expansion_error_unset_exits_nonzero() {
    let (_stdout, code) = run_c("echo ${MUST_SET_XYZ:?required}");
    assert_ne!(code, 0);
}
